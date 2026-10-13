//! Shared feature adapter for GUI bridge, CLI and MCP. No duplicate editing logic.
use crate::{Workspace,CommandError,CommandResult,db::Origin,bridge::{BridgeResponse,BridgeError}};
use serde::{Serialize,Deserialize};
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(tag="action",rename_all="snake_case")]
pub enum WorkbenchRequest{
 Overview,
 Episode{project_id:String,episode_id:String},
 EditEpisodes{project_id:String,operation:String,#[serde(default)]episode_id:String,#[serde(default)]title:String,#[serde(default)]order:Vec<String>,expected_revision:u64},
 SavePlanning{project_id:String,patches:Vec<screenplay_core::series_planning::PlanningPatch>,expected_revision:u64},
 PreviewReplacement{project_id:String,query:String,replacement:String,scope:String},
 ApplyReplacement{project_id:String,query:String,replacement:String,scope:String,selected:Vec<String>,expected_revision:u64},
 CompareDrafts{project_id:String,version_a:String,#[serde(default)]version_b:Option<String>},
}
fn error(e:CommandError)->BridgeError{match e{CommandError::RevisionConflict{expected,actual,message}=>BridgeError::RevisionConflict{expected,actual,message},CommandError::NotFound{what,id}=>BridgeError::NotFound{what,id},CommandError::InvalidArgument{message}=>BridgeError::InvalidArgument{message},CommandError::BlockLocked{id}=>BridgeError::BlockLocked{id},CommandError::Internal{message}=>BridgeError::Internal{message}}}
pub fn execute(ws:&Workspace,request:WorkbenchRequest,origin:Origin)->BridgeResponse{
 let doc=ws.document();let rev=doc.revision;
 let check=|id:&str|if doc.id==id{Ok(())}else{Err(CommandError::InvalidArgument{message:"项目已切换，请重新读取 workbench overview".into()})};
 let commit=|r:Result<CommandResult,CommandError>|match r{Ok(r)=>{ws.touch();BridgeResponse::ok(serde_json::to_value(&r).unwrap(),r.revision)},Err(e)=>BridgeResponse::fail(error(e),ws.revision())};
 let read=(||->Result<serde_json::Value,CommandError>{match request{
 WorkbenchRequest::Overview=>Ok(serde_json::json!({"project_id":doc.id,"revision":rev,"active_episode_id":doc.series.as_ref().map(|s|&s.active_id),"series":screenplay_core::series::ui_snapshot(&doc).series,"draft_label":doc.draft_label,"capabilities":["episodes","planning","reviewed_replacement","draft_comparison"],"write_contract":"Read project_id and revision; preview replacements before applying selected keys. Writes share GUI undo."})),
 WorkbenchRequest::Episode{project_id,episode_id}=>{check(&project_id)?;let d=screenplay_core::series_planning::episode_document(&doc,&episode_id).ok_or_else(||CommandError::NotFound{what:"episode".into(),id:episode_id.clone()})?;let mut d=d.clone();d.series=None;Ok(serde_json::json!({"project_id":doc.id,"revision":rev,"episode_id":episode_id,"document":d}))},
 WorkbenchRequest::EditEpisodes{project_id,operation,episode_id,title,order,expected_revision}=>{check(&project_id)?;return Ok(serde_json::to_value(commit(ws.service.edit_series_from(origin,&operation,&episode_id,&title,&order,expected_revision))).unwrap());},
 WorkbenchRequest::SavePlanning{project_id,patches,expected_revision}=>{check(&project_id)?;return Ok(serde_json::to_value(commit(ws.service.update_series_planning_from(origin,&patches,expected_revision))).unwrap());},
 WorkbenchRequest::PreviewReplacement{project_id,query,replacement,scope}=>{check(&project_id)?;let hits=screenplay_core::text_replace::preview(&doc,&query,&replacement,&scope).map_err(|message|CommandError::InvalidArgument{message})?;Ok(serde_json::json!({"project_id":doc.id,"revision":rev,"active_episode_id":doc.series.as_ref().map(|s|&s.active_id),"hits":hits}))},
 WorkbenchRequest::ApplyReplacement{project_id,query,replacement,scope,selected,expected_revision}=>{check(&project_id)?;return Ok(serde_json::to_value(commit(ws.service.replace_text_reviewed_from(origin,&project_id,&query,&replacement,&scope,&selected,expected_revision))).unwrap());},
 WorkbenchRequest::CompareDrafts{project_id,version_a,version_b}=>{check(&project_id)?;let a=ws.service.store.version_document(&version_a).ok_or_else(||CommandError::NotFound{what:"version".into(),id:version_a})?;let b=match version_b.filter(|s|!s.is_empty()){Some(id)=>ws.service.store.version_document(&id).ok_or_else(||CommandError::NotFound{what:"version".into(),id})?,None=>doc.clone()};Ok(serde_json::to_value(screenplay_core::revision_compare::compare(&a,&b)).unwrap())}
 }})();
 match read{Ok(v) if v.get("ok").is_some()&&v.get("revision").is_some()=>serde_json::from_value(v).unwrap(),Ok(v)=>BridgeResponse::ok(v,rev),Err(e)=>BridgeResponse::fail(error(e),ws.revision())}
}
#[cfg(test)]mod tests{use super::*;
 #[test]fn overview_and_reviewed_writes_share_revision_undo_and_identity(){let ws=Workspace::blank("测试").unwrap();let overview=execute(&ws,WorkbenchRequest::Overview,Origin::Cli);let id=overview.data.unwrap()["project_id"].as_str().unwrap().to_string();let req=|pid:String,rev|WorkbenchRequest::EditEpisodes{project_id:pid,operation:"enable".into(),episode_id:"".into(),title:"第一集".into(),order:vec![],expected_revision:rev};assert!(!execute(&ws,req("wrong".into(),0),Origin::Cli).ok);let r=execute(&ws,req(id.clone(),0),Origin::Cli);assert!(r.ok);assert!(!execute(&ws,req(id,0),Origin::Mcp).ok);ws.service.undo(Origin::Cli).unwrap();assert!(ws.document().series.is_none());}
}
