use crate::{AppState,IpcError,IpcResult,broadcast};
use screenplay_store::command::CommandResult;
use tauri::{AppHandle,State};
#[tauri::command]
pub fn edit_series(app:AppHandle,state:State<'_,AppState>,action:String,id:String,title:String,order:Vec<String>,expected_revision:u64)->IpcResult<CommandResult>{
 let result=state.project.run(|svc|svc.edit_series(&action,&id,&title,&order,expected_revision))?;broadcast(&app,"gui-left",&result);Ok(result)
}
#[tauri::command]
pub async fn export_series_files(state:State<'_,AppState>,directory:String,format:String)->IpcResult<Vec<String>>{
 let doc=state.project.document()?;
 tauri::async_runtime::spawn_blocking(move||export_all(&doc,&directory,&format)).await.map_err(|e|IpcError::Internal{message:e.to_string()})?
}
fn export_all(doc:&screenplay_core::ast::ScreenplayDocument,directory:&str,format:&str)->IpcResult<Vec<String>>{
 let folder=std::path::Path::new(directory);if !folder.is_dir(){return Err(IpcError::InvalidArgument{message:"请选择导出文件夹".into()});}
 let fmt=crate::bridge_host::resolve_export_format(&format!("episode.{format}"),Some(format)).map_err(|e|IpcError::InvalidArgument{message:e.message()})?;
 let drafts=screenplay_core::series::export_drafts(doc).map_err(|message|IpcError::InvalidArgument{message})?;
 let mut prepared=Vec::new();
 for (name,draft) in drafts{
  let mut safe=String::new();for c in name.chars(){let c=if c.is_control()||"/\\:*?\"<>|".contains(c){'_'}else{c};if safe.len()+c.len_utf8()>180{break;}safe.push(c);}
  let path=folder.join(format!("{safe}.{format}"));if path.exists(){return Err(IpcError::InvalidArgument{message:format!("文件已存在，请选择空文件夹，避免覆盖 {}",path.display())});}
  let bytes=screenplay_export::export(&draft,fmt).map_err(|e|IpcError::Internal{message:format!("导出失败 {e}")})?;prepared.push((path,bytes));
 }
 let mut written:Vec<std::path::PathBuf>=Vec::new();
 for (path,bytes) in prepared{use std::io::Write;let output=std::fs::OpenOptions::new().write(true).create_new(true).open(&path);let result=match output{Ok(mut f)=>{written.push(path.clone());f.write_all(&bytes).and_then(|_|f.sync_all())},Err(e)=>Err(e)};if let Err(e)=result{for created in &written{let _=std::fs::remove_file(created);}return Err(IpcError::Internal{message:format!("导出失败 {e}")});}}
 Ok(written.iter().map(|p|p.to_string_lossy().into()).collect())
}

#[cfg(test)]mod tests{use super::*;use screenplay_core::ast::*;
 #[test]fn batch_export_separates_episodes_and_never_overwrites(){let folder=std::env::temp_dir().join(new_id("episode-export-test"));std::fs::create_dir(&folder).unwrap();let mut doc=ScreenplayDocument::blank("剧集");doc.blocks.push(ScreenplayBlock::new(BlockType::Action,"第一集正文"));screenplay_core::series::edit(&mut doc,"add","","第二集",&[]).unwrap();doc.blocks.push(ScreenplayBlock::new(BlockType::Action,"第二集正文"));for format in ["pdf","md","fdx","docx"]{let paths=export_all(&doc,folder.to_str().unwrap(),format).unwrap();assert_eq!(paths.len(),2);assert!(paths.iter().all(|p|std::fs::metadata(p).unwrap().len()>0));if format=="md"{let first=std::fs::read_to_string(&paths[0]).unwrap();let second=std::fs::read_to_string(&paths[1]).unwrap();assert!(first.contains("第一集正文")&&!first.contains("第二集正文"));assert!(second.contains("第二集正文")&&!second.contains("第一集正文"));}let original=std::fs::read(&paths[0]).unwrap();assert!(export_all(&doc,folder.to_str().unwrap(),format).is_err());assert_eq!(std::fs::read(&paths[0]).unwrap(),original);}std::fs::remove_dir_all(folder).unwrap();}
}

#[tauri::command]
pub fn save_series_planning(app:AppHandle,state:State<'_,AppState>,project_id:String,patches:Vec<screenplay_core::series_planning::PlanningPatch>,expected_revision:u64)->IpcResult<CommandResult>{let result=state.project.run(|svc|{if svc.store.snapshot().id!=project_id{return Err(screenplay_store::command::CommandError::InvalidArgument{message:"项目已切换，规划未保存".into()});}svc.update_series_planning(&patches,expected_revision)})?;broadcast(&app,"planning",&result);Ok(result)}
#[tauri::command]
pub fn review_continuity(app:AppHandle,state:State<'_,AppState>,report_id:String,finding_id:String,status:String,expected_revision:u64)->IpcResult<CommandResult>{let result=state.project.run(|svc|svc.review_continuity_finding(&report_id,&finding_id,&status,expected_revision))?;broadcast(&app,"review",&result);Ok(result)}
#[tauri::command]
pub fn series_report_status(state:State<'_,AppState>)->IpcResult<Vec<(String,bool)>>{let doc=state.project.document()?;Ok(doc.series.as_ref().map(|s|s.checks.iter().map(|r|(r.id.clone(),screenplay_core::series_planning::fingerprint(&doc,&r.episode_ids).ok().as_deref()!=Some(r.source_fingerprint.as_str()))).collect()).unwrap_or_default())}
#[tauri::command]
pub fn compare_project_versions(state:State<'_,AppState>,version_a:String,version_b:Option<String>)->IpcResult<screenplay_core::revision_compare::RevisionComparison>{let ws=state.project.require()?;let old=ws.service.store.version_document(&version_a).ok_or_else(||IpcError::NotFound{what:"version".into(),id:version_a})?;let new=if let Some(id)=version_b.filter(|s|!s.is_empty()){ws.service.store.version_document(&id).ok_or_else(||IpcError::NotFound{what:"version".into(),id})?}else{ws.document()};Ok(screenplay_core::revision_compare::compare(&old,&new))}

#[tauri::command]
pub fn preview_text_replacement(state:State<'_,AppState>,query:String,replacement:String,scope:String)->IpcResult<serde_json::Value>{let doc=state.project.document()?;let hits=screenplay_core::text_replace::preview(&doc,&query,&replacement,&scope).map_err(|message|IpcError::InvalidArgument{message})?;Ok(serde_json::json!({"projectId":doc.id,"revision":doc.revision,"hits":hits}))}
#[tauri::command]
pub fn apply_text_replacement(app:AppHandle,state:State<'_,AppState>,project_id:String,query:String,replacement:String,scope:String,selected:Vec<String>,expected_revision:u64)->IpcResult<CommandResult>{let result=state.project.run(|svc|svc.replace_text_reviewed(&project_id,&query,&replacement,&scope,&selected,expected_revision))?;broadcast(&app,"replace",&result);Ok(result)}
