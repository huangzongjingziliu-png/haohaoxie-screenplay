use crate::{ast::*,diff::{self,DiffEntry}};
use serde::Serialize;
use ts_rs::TS;
#[derive(Debug,Clone,Serialize,TS)]
#[ts(export,export_to="../../../src/ipc/types/")]
pub struct RevisionChange {pub kind:String,pub label:String,pub before:String,pub after:String}
#[derive(Debug,Clone,Serialize,TS)]
#[ts(export,export_to="../../../src/ipc/types/")]
pub struct EpisodeRevisionDiff {pub episode_id:String,pub title:String,pub added_blocks:usize,pub removed_blocks:usize,pub changed_blocks:usize,pub changes:Vec<RevisionChange>}
#[derive(Debug,Clone,Serialize,TS)]
#[ts(export,export_to="../../../src/ipc/types/")]
pub struct RevisionComparison {pub from_label:String,pub to_label:String,pub episodes:Vec<EpisodeRevisionDiff>}
fn drafts(doc:&ScreenplayDocument,legacy_id:&str)->Vec<(String,String,ScreenplayDocument)>{if let Some(series)=&doc.series{series.episodes.iter().filter_map(|e|crate::series_planning::episode_document(doc,&e.id).map(|d|(e.id.clone(),e.title.clone(),d.clone()))).collect()}else{vec![(legacy_id.into(),doc.metadata.title.clone(),doc.clone())]}}
fn heading(doc:&ScreenplayDocument,id:&str)->String{doc.block(id).map(|b|b.text.clone()).unwrap_or_else(||id.into())}
pub fn compare(old:&ScreenplayDocument,new:&ScreenplayDocument)->RevisionComparison{
 let legacy=new.series.as_ref().or(old.series.as_ref()).and_then(|s|s.episodes.first()).map(|e|e.id.as_str()).unwrap_or("single");let a=drafts(old,legacy);let b=drafts(new,legacy);let mut order=b.iter().map(|(id,_,_)|id.clone()).collect::<Vec<_>>();for (id,_,_) in &a{if !order.contains(id){order.push(id.clone());}}
 let mut episodes=Vec::new();
 for id in order{let before=a.iter().find(|(e,_,_)|e==&id);let after=b.iter().find(|(e,_,_)|e==&id);let empty=ScreenplayDocument::blank("");let left=before.map(|e|&e.2).unwrap_or(&empty);let right=after.map(|e|&e.2).unwrap_or(&empty);let title=after.or(before).map(|e|e.1.clone()).unwrap_or_default();let d=diff::diff_documents(left,right,None,None);let mut changes=Vec::new();
 for e in d.entries{let (kind,label,before,after)=match e{
 DiffEntry::BlockAdded{block_type,text,..}=>("added",block_type,String::new(),text),DiffEntry::BlockRemoved{block_type,text,..}=>("removed",block_type,text,String::new()),DiffEntry::BlockChanged{block_type,old_text,new_text,..}=>("changed",block_type,old_text,new_text),DiffEntry::BlockRetyped{old_type,new_type,text,..}=>("retyped",format!("{old_type} → {new_type}"),text.clone(),text),DiffEntry::SceneAdded{scene,..}=>("scene_added","场景".into(),String::new(),heading(right,&scene)),DiffEntry::SceneRemoved{scene,..}=>("scene_removed","场景".into(),heading(left,&scene),String::new()),DiffEntry::SceneMoved{scene,from,to}=>("moved",heading(right,&scene),format!("{}",from+1),format!("{}",to+1)),DiffEntry::TitlePageChanged{field,old_value,new_value}|DiffEntry::SettingsChanged{field,old_value,new_value}=>("metadata",field,old_value,new_value)};changes.push(RevisionChange{kind:kind.into(),label,before,after});}
 if before.is_none(){changes.insert(0,RevisionChange{kind:"episode_added".into(),label:"分集".into(),before:String::new(),after:title.clone()});}if after.is_none(){changes.insert(0,RevisionChange{kind:"episode_removed".into(),label:"分集".into(),before:title.clone(),after:String::new()});}
 if before.is_some()&&after.is_some(){let from=a.iter().position(|e|e.0==id).unwrap();let to=b.iter().position(|e|e.0==id).unwrap();if from!=to{changes.push(RevisionChange{kind:"moved".into(),label:"分集排序".into(),before:format!("{}",from+1),after:format!("{}",to+1)});}}
 if let (Some(x),Some(y))=(before,after){if x.1!=y.1{changes.push(RevisionChange{kind:"metadata".into(),label:"分集名称".into(),before:x.1.clone(),after:y.1.clone()});}}
 let old_plan=old.series.as_ref().and_then(|s|s.episodes.iter().find(|e|e.id==id)).map(|e|e.planning.clone()).unwrap_or_default();let new_plan=new.series.as_ref().and_then(|s|s.episodes.iter().find(|e|e.id==id)).map(|e|e.planning.clone()).unwrap_or_default();for (field,x,y) in [("梗概",old_plan.synopsis,new_plan.synopsis),("冲突",old_plan.conflict,new_plan.conflict),("转折",old_plan.turning_point,new_plan.turning_point),("悬念",old_plan.cliffhanger,new_plan.cliffhanger),("进度",old_plan.status,new_plan.status)]{if x!=y{changes.push(RevisionChange{kind:"planning".into(),label:field.into(),before:x,after:y});}}
 episodes.push(EpisodeRevisionDiff{episode_id:id,title,added_blocks:d.added_blocks,removed_blocks:d.removed_blocks,changed_blocks:d.changed_blocks,changes});
 }
 let season_a=old.series.as_ref().map(|s|s.season_outline.clone()).unwrap_or_default();let season_b=new.series.as_ref().map(|s|s.season_outline.clone()).unwrap_or_default();if season_a!=season_b{episodes.insert(0,EpisodeRevisionDiff{episode_id:"season".into(),title:"季纲".into(),added_blocks:0,removed_blocks:0,changed_blocks:0,changes:vec![RevisionChange{kind:"planning".into(),label:"季纲".into(),before:season_a,after:season_b}]});}
 RevisionComparison{from_label:old.draft_label.clone().unwrap_or_default(),to_label:new.draft_label.clone().unwrap_or_default(),episodes}
}
#[cfg(test)]mod tests{use super::*;
 #[test]fn compares_matching_episodes_even_when_active_episode_changes(){let mut a=ScreenplayDocument::blank("剧集");a.blocks.push(ScreenplayBlock::new(BlockType::Action,"第一集"));crate::series::edit(&mut a,"add","","第二集",&[]).unwrap();a.blocks.push(ScreenplayBlock::new(BlockType::Action,"第二集"));let mut b=a.clone();let first=b.series.as_ref().unwrap().episodes[0].id.clone();crate::series::edit(&mut b,"switch",&first,"",&[]).unwrap();assert!(compare(&a,&b).episodes.iter().all(|e|e.changes.is_empty()));b.blocks[0].text="第一集修改".into();let d=compare(&a,&b);assert!(!d.episodes[0].changes.is_empty());assert!(d.episodes[1].changes.is_empty());}
}
