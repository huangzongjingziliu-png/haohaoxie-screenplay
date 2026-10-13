use crate::ast::*;
use serde::{Serialize,Deserialize};
use std::hash::{Hash,Hasher};
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct PlanningPatch{pub episode_id:String,pub field:String,pub value:String,pub expected_value:String}
pub fn episode_document<'a>(doc:&'a ScreenplayDocument,id:&str)->Option<&'a ScreenplayDocument>{let s=doc.series.as_ref()?;if s.active_id==id{Some(doc)}else{s.episodes.iter().find(|e|e.id==id)?.draft.as_deref()}}
pub fn apply_patches(doc:&mut ScreenplayDocument,patches:&[PlanningPatch])->Result<(),String>{
 if patches.len()>2000{return Err("修改项过多".into());}
 let series=doc.series.as_mut().ok_or("请先启用分集")?;
 for patch in patches{
  if patch.value.chars().count()>20000{return Err("每个规划字段最多 20000 字".into());}
  let field=if patch.episode_id.is_empty()&&patch.field=="season_outline"{&mut series.season_outline}else{let p=&mut series.episodes.iter_mut().find(|e|e.id==patch.episode_id).ok_or("分集不存在")?.planning;match patch.field.as_str(){"synopsis"=>&mut p.synopsis,"conflict"=>&mut p.conflict,"turning_point"=>&mut p.turning_point,"cliffhanger"=>&mut p.cliffhanger,"status"=>{if !["","idea","outline","draft","revision","ready"].contains(&patch.value.as_str()){return Err("未知分集进度".into());}&mut p.status},_=>return Err("未知规划字段".into())}};
  if *field!=patch.expected_value{return Err("规划字段已被修改，请重新读取后合并，未覆盖其他修改".into());}*field=patch.value.clone();
 };Ok(())
}
pub fn fingerprint(doc:&ScreenplayDocument,ids:&[String])->Result<String,String>{let series=doc.series.as_ref().ok_or("请先启用分集")?;let mut hash=std::collections::hash_map::DefaultHasher::new();series.season_outline.hash(&mut hash);doc.story_bible.world.hash(&mut hash);doc.story_bible.era.hash(&mut hash);doc.story_bible.relationships.hash(&mut hash);doc.story_bible.principles.hash(&mut hash);doc.story_bible.character_bios.iter().for_each(|(k,v)|{k.hash(&mut hash);v.hash(&mut hash);});doc.settings.project_memory.author_notes.hash(&mut hash);
 for id in ids{let e=series.episodes.iter().find(|e|&e.id==id).ok_or("检查中的分集已经删除")?;id.hash(&mut hash);e.title.hash(&mut hash);e.planning.synopsis.hash(&mut hash);e.planning.conflict.hash(&mut hash);e.planning.turning_point.hash(&mut hash);e.planning.cliffhanger.hash(&mut hash);let draft=episode_document(doc,id).ok_or("分集正文缺失")?;for b in &draft.blocks{b.id.hash(&mut hash);b.text.hash(&mut hash);b.block_type.as_str().hash(&mut hash);}}
 Ok(format!("{:016x}",hash.finish()))}
pub fn validate_evidence(doc:&ScreenplayDocument,e:&ContinuityEvidence)->bool{
 if e.quote.trim().is_empty()||e.quote.chars().count()>400{return false;}
 if let Some(field)=e.block_id.strip_prefix("planning:"){let Some(plan)=doc.series.as_ref().and_then(|s|s.episodes.iter().find(|p|p.id==e.episode_id)).map(|p|&p.planning)else{return false;};let text=match field{"synopsis"=>&plan.synopsis,"conflict"=>&plan.conflict,"turning_point"=>&plan.turning_point,"cliffhanger"=>&plan.cliffhanger,_=>return false};return text.contains(&e.quote);}
 episode_document(doc,&e.episode_id).and_then(|d|d.block(&e.block_id)).is_some_and(|b|b.text.contains(&e.quote))
}
#[cfg(test)]mod tests{use super::*;
 #[test]fn planning_conflicts_are_detected_and_fingerprints_follow_text(){let mut doc=ScreenplayDocument::blank("剧集");crate::series::edit(&mut doc,"enable","","",&[]).unwrap();let id=doc.series.as_ref().unwrap().active_id.clone();let ids=vec![id.clone()];let before=fingerprint(&doc,&ids).unwrap();apply_patches(&mut doc,&[PlanningPatch{episode_id:id.clone(),field:"synopsis".into(),value:"重逢".into(),expected_value:"".into()}]).unwrap();assert_ne!(before,fingerprint(&doc,&ids).unwrap());assert!(apply_patches(&mut doc,&[PlanningPatch{episode_id:id,field:"synopsis".into(),value:"覆盖".into(),expected_value:"".into()}]).is_err());assert_eq!(doc.series.unwrap().episodes[0].planning.synopsis,"重逢");}
 #[test]fn evidence_must_exist_verbatim(){let mut doc=ScreenplayDocument::blank("剧集");crate::series::edit(&mut doc,"enable","","",&[]).unwrap();let b=ScreenplayBlock::new(BlockType::Action,"老王昨天失去了左手。");let e=ContinuityEvidence{episode_id:doc.series.as_ref().unwrap().active_id.clone(),block_id:b.id.clone(),quote:"失去了左手".into()};doc.blocks.push(b);assert!(validate_evidence(&doc,&e));let mut bad=e;bad.quote="右手".into();assert!(!validate_evidence(&doc,&bad));}
}
#[cfg(test)]mod planning_evidence_tests{use super::*;
 #[test]fn planning_quotes_are_validated_and_progress_alone_does_not_make_report_stale(){let mut doc=ScreenplayDocument::blank("剧集");crate::series::edit(&mut doc,"enable","","",&[]).unwrap();let id=doc.series.as_ref().unwrap().active_id.clone();doc.series.as_mut().unwrap().episodes[0].planning.synopsis="女孩第一次见到父亲".into();let evidence=ContinuityEvidence{episode_id:id.clone(),block_id:"planning:synopsis".into(),quote:"第一次见到父亲".into()};assert!(validate_evidence(&doc,&evidence));let before=fingerprint(&doc,&[id.clone()]).unwrap();doc.series.as_mut().unwrap().episodes[0].planning.status="ready".into();assert_eq!(before,fingerprint(&doc,&[id]).unwrap());}
}
