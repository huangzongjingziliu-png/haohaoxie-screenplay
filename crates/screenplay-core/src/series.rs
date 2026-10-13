//! Episodic projects keep the active draft in the document and archive only inactive drafts.
use crate::ast::*;
fn valid_title(title:&str)->Result<String,String>{let title=title.trim();if title.is_empty()||title.chars().count()>120{return Err("分集名称不能为空，且不能超过 120 字".into());}Ok(title.into())}
fn enable(doc:&mut ScreenplayDocument){if doc.series.is_none(){let id=new_id("episode");doc.series=Some(Series{season_outline:String::new(),checks:Vec::new(),active_id:id.clone(),episodes:vec![Episode{planning:Default::default(),id,title:"第 1 集".into(),draft:None}]});}}
fn switch(doc:&mut ScreenplayDocument,id:&str)->Result<(),String>{
 let mut series=doc.series.clone().ok_or("请先创建分集")?;
 if series.active_id==id{return Ok(());}
 let target=series.episodes.iter().position(|e|e.id==id).ok_or("分集不存在")?;
 let mut next=series.episodes[target].draft.take().ok_or("分集数据不完整")?.as_ref().clone();
 if next.series.is_some(){return Err("分集包含非法嵌套数据".into());}
 let old=series.episodes.iter().position(|e|e.id==series.active_id).ok_or("当前分集不存在")?;
 let mut previous=doc.clone();previous.series=None;previous.settings.project_memory=Default::default();series.episodes[old].draft=Some(std::sync::Arc::new(previous));
 series.active_id=id.into();
 // Identity and project-wide author notes/memory stay with the engineering file.
 next.draft_label=doc.draft_label.clone();next.id=doc.id.clone();next.revision=doc.revision;next.metadata=doc.metadata.clone();next.created_at=doc.created_at.clone();next.updated_at=doc.updated_at.clone();next.story_bible=doc.story_bible.clone();next.project_rules=doc.project_rules.clone();next.settings.project_memory=doc.settings.project_memory.clone();next.series=Some(series);next.rebuild_derived();*doc=next;Ok(())
}
pub fn validate(doc:&ScreenplayDocument)->Result<(),String>{
 let Some(series)=&doc.series else{return Ok(());};
 let ids:std::collections::HashSet<_>=series.episodes.iter().map(|e|e.id.as_str()).collect();
 if series.episodes.is_empty()||series.episodes.len()>200||ids.len()!=series.episodes.len()||!ids.contains(series.active_id.as_str()){return Err("分集列表不完整或无效".into());}
 for e in &series.episodes{valid_title(&e.title)?;if e.id==series.active_id{if e.draft.is_some(){return Err("当前分集数据重复".into());}}else if e.draft.as_ref().is_none_or(|d|d.series.is_some()){return Err("分集正文缺失或含非法嵌套".into());}}
 Ok(())
}
pub fn edit(doc:&mut ScreenplayDocument,action:&str,id:&str,title:&str,order:&[String])->Result<(),String>{
 validate(doc)?;
 match action{
 "add"=>{let title=valid_title(title)?;if doc.series.as_ref().is_some_and(|s|s.episodes.len()>=200){return Err("一个工程最多 200 集".into());}enable(doc);let new=new_id("episode");let mut draft=ScreenplayDocument::blank(&doc.metadata.title);draft.title_page=doc.title_page.clone();draft.settings=doc.settings.clone();draft.settings.scene_board.cards.clear();draft.settings.scene_board.groups.clear();draft.settings.scene_board.links.clear();doc.series.as_mut().unwrap().episodes.push(Episode{planning:Default::default(),id:new.clone(),title,draft:Some(std::sync::Arc::new(draft))});switch(doc,&new)?;},
 "enable"=>{let fresh=doc.series.is_none();enable(doc);if fresh&&!title.trim().is_empty(){doc.series.as_mut().unwrap().episodes[0].title=valid_title(title)?;}},
 "disable"=>{let series=doc.series.as_ref().ok_or("当前已经是单篇剧本")?;if series.episodes.len()!=1{return Err("取消分集前需只保留一集；请先另存工程或导出其他分集".into());}doc.series=None;},
 "switch"=>switch(doc,id)?,
 "rename"=>{let title=valid_title(title)?;let ep=doc.series.as_mut().ok_or("请先创建分集")?.episodes.iter_mut().find(|e|e.id==id).ok_or("分集不存在")?;ep.title=title;},
 "delete"=>{let series=doc.series.as_ref().ok_or("请先创建分集")?;if series.episodes.len()<=1{return Err("至少保留一集".into());}if !series.episodes.iter().any(|e|e.id==id){return Err("分集不存在".into());}if series.active_id==id{let next=series.episodes.iter().find(|e|e.id!=id).unwrap().id.clone();switch(doc,&next)?;}doc.series.as_mut().unwrap().episodes.retain(|e|e.id!=id);},
 "reorder"=>{let series=doc.series.as_mut().ok_or("请先创建分集")?;let ids:std::collections::HashSet<_>=order.iter().collect();if ids.len()!=series.episodes.len()||order.len()!=ids.len()||!series.episodes.iter().all(|e|ids.contains(&e.id)){return Err("分集排序不完整".into());}series.episodes.sort_by_key(|e|order.iter().position(|id|id==&e.id).unwrap());},
 _=>return Err("未知分集操作".into())
 };validate(doc)
}
pub fn export_drafts(doc:&ScreenplayDocument)->Result<Vec<(String,ScreenplayDocument)>,String>{
 validate(doc)?;
 let Some(series)=&doc.series else{return Ok(vec![(doc.metadata.title.clone(),publication_copy(doc))]);};
 series.episodes.iter().enumerate().map(|(i,e)|{let mut draft=if e.id==series.active_id{doc.clone()}else{e.draft.as_ref().ok_or("分集数据不完整")?.as_ref().clone()};draft.series=None;draft.metadata=doc.metadata.clone();draft.metadata.title=format!("{} · {}",doc.metadata.title,e.title);draft.title_page.title=draft.metadata.title.clone();let name=format!("{:02}-{}{}",i+1,e.title,doc.draft_label.as_ref().map(|s|format!("-{s}")).unwrap_or_default());Ok((name,publication_copy(&draft)))}).collect()
}
#[cfg(test)]mod tests{use super::*;
 #[test]fn switching_keeps_edits_and_project_memory(){let mut doc=ScreenplayDocument::blank("剧集");doc.blocks.push(ScreenplayBlock::new(BlockType::Action,"第一集"));edit(&mut doc,"add","","第二集",&[]).unwrap();let first=doc.series.as_ref().unwrap().episodes[0].id.clone();let second=doc.series.as_ref().unwrap().active_id.clone();doc.blocks.push(ScreenplayBlock::new(BlockType::Action,"第二集"));doc.settings.project_memory.markdown="共享设定".into();edit(&mut doc,"switch",&first,"",&[]).unwrap();assert_eq!(doc.blocks[0].text,"第一集");assert_eq!(doc.settings.project_memory.markdown,"共享设定");edit(&mut doc,"switch",&second,"",&[]).unwrap();assert_eq!(doc.blocks[0].text,"第二集");let saved=serde_json::to_string(&doc).unwrap();let restored:ScreenplayDocument=serde_json::from_str(&saved).unwrap();assert_eq!(export_drafts(&restored).unwrap().len(),2);assert!(restored.series.unwrap().episodes.into_iter().filter_map(|e|e.draft).all(|d|d.series.is_none()));}
 #[test]fn reorder_delete_and_export_use_current_active_draft(){let mut doc=ScreenplayDocument::blank("剧集");edit(&mut doc,"add","","新集",&[]).unwrap();doc.blocks.push(ScreenplayBlock::new(BlockType::Action,"最新正文"));let ids=doc.series.as_ref().unwrap().episodes.iter().rev().map(|e|e.id.clone()).collect::<Vec<_>>();edit(&mut doc,"reorder","","",&ids).unwrap();assert_eq!(export_drafts(&doc).unwrap()[0].1.blocks[0].text,"最新正文");edit(&mut doc,"delete",&ids[0],"",&[]).unwrap();assert_eq!(doc.series.as_ref().unwrap().episodes.len(),1);assert!(edit(&mut doc,"delete",&ids[1],"",&[]).is_err());}
}

/// GUI and command responses carry only the active script and episode directory.
/// Project saves still serialize the complete document through the normal AST serializer.
pub fn ui_snapshot(doc:&ScreenplayDocument)->ScreenplayDocument{let mut snapshot=doc.clone();if let Some(series)=&mut snapshot.series{for episode in &mut series.episodes{episode.draft=None;}}snapshot}
pub fn serialize_ui_document<S:serde::Serializer>(doc:&ScreenplayDocument,serializer:S)->Result<S::Ok,S::Error>{serde::Serialize::serialize(&ui_snapshot(doc),serializer)}
#[cfg(test)]mod efficiency_tests{use super::*;
 #[test]fn archived_drafts_share_storage_while_ui_payload_excludes_their_text(){let mut doc=ScreenplayDocument::blank("剧集");doc.blocks.push(ScreenplayBlock::new(BlockType::Action,"仅第一集可见的内容"));edit(&mut doc,"add","","第二集",&[]).unwrap();let cloned=doc.clone();let archive=doc.series.as_ref().unwrap().episodes[0].draft.as_ref().unwrap();let second=cloned.series.as_ref().unwrap().episodes[0].draft.as_ref().unwrap();assert!(std::sync::Arc::ptr_eq(archive,second));let ui=serde_json::to_string(&ui_snapshot(&doc)).unwrap();assert!(!ui.contains("仅第一集可见的内容"));let saved=serde_json::to_string(&doc).unwrap();assert!(saved.contains("仅第一集可见的内容"));}
}

pub fn publication_copy(doc:&ScreenplayDocument)->ScreenplayDocument{let mut draft=doc.clone();draft.series=None;if let Some(label)=doc.draft_label.as_deref().filter(|s|!s.trim().is_empty()){draft.metadata.title=format!("{} · {}",draft.metadata.title,label);draft.title_page.title=draft.metadata.title.clone();}draft}
#[cfg(test)]mod publication_tests{use super::*;
 #[test]fn published_draft_is_labelled_without_changing_original(){let mut doc=ScreenplayDocument::blank("剧集");doc.draft_label=Some("V2.1".into());doc.blocks.push(ScreenplayBlock::new(BlockType::Action,"原文"));let copy=publication_copy(&doc);assert!(copy.title_page.title.contains("V2.1"));assert_eq!(doc.metadata.title,"剧集");assert_eq!(copy.blocks[0].text,"原文");}
}

#[cfg(test)]mod disable_tests{use super::*;
 #[test]fn disabling_single_episode_preserves_script_and_board(){let mut doc=ScreenplayDocument::blank("作品");doc.blocks.push(ScreenplayBlock::new(BlockType::Action,"原文"));edit(&mut doc,"enable","","第一集",&[]).unwrap();let before=doc.blocks.clone();let board=serde_json::to_string(&doc.settings.scene_board).unwrap();edit(&mut doc,"disable","","",&[]).unwrap();assert!(doc.series.is_none());assert_eq!(serde_json::to_string(&before).unwrap(),serde_json::to_string(&doc.blocks).unwrap());assert_eq!(board,serde_json::to_string(&doc.settings.scene_board).unwrap());}
 #[test]fn disabling_multiple_episodes_is_rejected_without_loss(){let mut doc=ScreenplayDocument::blank("作品");edit(&mut doc,"add","","第二集",&[]).unwrap();let before=serde_json::to_string(&doc).unwrap();assert!(edit(&mut doc,"disable","","",&[]).is_err());assert_eq!(before,serde_json::to_string(&doc).unwrap());}
}
