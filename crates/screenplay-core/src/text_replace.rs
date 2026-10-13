//! Literal replacement with occurrence-level review and stable revision-scoped keys.
use crate::ast::*;
use serde::{Deserialize,Serialize};
use std::hash::{Hash,Hasher};
fn context(doc:&ScreenplayDocument,query:&str,replacement:&str,scope:&str)->String{let mut h=std::collections::hash_map::DefaultHasher::new();doc.id.hash(&mut h);doc.revision.hash(&mut h);doc.series.as_ref().map(|s|&s.active_id).hash(&mut h);query.hash(&mut h);replacement.hash(&mut h);scope.hash(&mut h);format!("{:016x}",h.finish())}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct ReplacementHit {pub key:String,pub episode:String,pub field:String,pub before:String,pub after:String,pub suspicious:bool}
fn fields(doc:&mut ScreenplayDocument,scope:&str,mut visit:impl FnMut(&str,&str,&str,&mut String)){
 fn draft(d:&mut ScreenplayDocument,id:&str,title:&str,v:&mut impl FnMut(&str,&str,&str,&mut String)){
  for b in &mut d.blocks{v(id,title,&format!("block:{}",b.id),&mut b.text);}
  for (idc,c) in &mut d.settings.scene_board.cards{v(id,title,&format!("summary:{idc}"),&mut c.summary);for (i,t) in c.comments.iter_mut().enumerate(){v(id,title,&format!("comment:{idc}:{i}"),t);}}
  for g in &mut d.settings.scene_board.groups{v(id,title,&format!("group:{}",g.id),&mut g.name);}
  for l in &mut d.settings.scene_board.links{v(id,title,&format!("link:{}",l.id),&mut l.label);}
 }
 let active=doc.series.as_ref().map(|s|(s.active_id.clone(),s.episodes.iter().find(|e|e.id==s.active_id).unwrap().title.clone())).unwrap_or(("single".into(),doc.metadata.title.clone()));
 draft(doc,&active.0,&active.1,&mut visit);
 if scope=="all"{if let Some(series)=&mut doc.series{for e in &mut series.episodes{if e.id!=active.0{if let Some(d)=&mut e.draft{draft(std::sync::Arc::make_mut(d),&e.id,&e.title,&mut visit);}}}}}
}
fn names(doc:&ScreenplayDocument)->Vec<String>{let mut all=Vec::new();all.extend(doc.characters.iter().map(|c|c.name.clone()));all.extend(doc.locations.iter().map(|c|c.name.clone()));if let Some(s)=&doc.series{for e in &s.episodes{if let Some(d)=&e.draft{all.extend(d.characters.iter().map(|c|c.name.clone()));all.extend(d.locations.iter().map(|c|c.name.clone()));}}}all}
pub fn preview(doc:&ScreenplayDocument,query:&str,replacement:&str,scope:&str)->Result<Vec<ReplacementHit>,String>{
 if query.is_empty()||query.chars().count()>200||replacement.chars().count()>200{return Err("查找文字不能为空，查找与替换文字最多 200 字".into());}if !["current","all"].contains(&scope){return Err("未知替换范围".into());}if query==replacement{return Ok(Vec::new());}
 let token=context(doc,query,replacement,scope);let longer=names(doc).into_iter().filter(|n|n!=query&&n.contains(query)).collect::<Vec<_>>();let mut copy=doc.clone();let mut hits=Vec::new();let mut overflow=false;
 fields(&mut copy,scope,|id,title,field,text|{for (pos,_) in text.match_indices(query){if hits.len()>=10000{overflow=true;break;}let end=pos+query.len();let latin= query.chars().all(|c|c.is_ascii_alphanumeric()||c=='_');let suspicious=longer.iter().any(|n|text.match_indices(n).any(|(a,_)|pos>=a&&end<=a+n.len()))||latin&&(text[..pos].chars().next_back().is_some_and(|c|c.is_ascii_alphanumeric()||c=='_')||text[end..].chars().next().is_some_and(|c|c.is_ascii_alphanumeric()||c=='_'));let start=text[..pos].char_indices().rev().nth(100).map(|(i,_)|i).unwrap_or(0);let stop=text[end..].char_indices().nth(100).map(|(i,_)|end+i).unwrap_or(text.len());let before=text[start..stop].to_string();let mut after=before.clone();after.replace_range(pos-start..end-start,replacement);hits.push(ReplacementHit{key:format!("{id}/{field}/{pos}/{token}"),episode:title.into(),field:field.into(),before,after,suspicious});}});
 if overflow{return Err("匹配超过一万处，请缩小查找范围".into());}Ok(hits)
}
pub fn apply(doc:&mut ScreenplayDocument,query:&str,replacement:&str,scope:&str,selected:&[String])->Result<(),String>{
 let token=context(doc,query,replacement,scope);let hits=preview(doc,query,replacement,scope)?;let valid_keys=hits.iter().map(|h|h.key.as_str()).collect::<std::collections::HashSet<_>>();let keys=selected.iter().collect::<std::collections::HashSet<_>>();if keys.len()!=selected.len()||selected.is_empty()||selected.iter().any(|k|!valid_keys.contains(k.as_str())){return Err("所选替换位置无效，请重新预览".into());}
 fields(doc,scope,|id,_,field,text|{let positions=text.match_indices(query).map(|(p,_)|p).collect::<Vec<_>>();for pos in positions.into_iter().rev(){if keys.contains(&format!("{id}/{field}/{pos}/{token}")){text.replace_range(pos..pos+query.len(),replacement);}}});
 fn sync(d:&mut ScreenplayDocument,id:&str,selected:&[String]){for b in &mut d.blocks{if b.block_type==BlockType::SceneHeading&&selected.iter().any(|k|k.starts_with(&format!("{id}/block:{}/",b.id))){if let Some((text,mut data))=crate::heading_format::format_heading(&b.text,&d.settings).or_else(||crate::parser::parse_scene_heading(&b.text).map(|(data,_)|(b.text.clone(),data))){if data.scene_number.is_none(){data.scene_number=b.scene_heading.as_ref().and_then(|h|h.scene_number.clone());}b.text=text;b.scene_heading=Some(data);}else{b.scene_heading=None;}}}d.rebuild_derived();}
 let active=doc.series.as_ref().map(|s|s.active_id.clone()).unwrap_or("single".into());sync(doc,&active,selected);if let Some(s)=&mut doc.series{for e in &mut s.episodes{if selected.iter().any(|k|k.starts_with(&format!("{}/",e.id))){if let Some(d)=&mut e.draft{sync(std::sync::Arc::make_mut(d),&e.id,selected);}}}}Ok(())
}
#[cfg(test)]mod tests{use super::*;
 #[test]fn occurrences_are_individual_and_unicode_safe(){let mut d=ScreenplayDocument::blank("作品");d.blocks.push(ScreenplayBlock::new(BlockType::Action,"张三看着张三"));let h=preview(&d,"张三","李四","current").unwrap();assert_eq!(h.len(),2);apply(&mut d,"张三","李四","current",&[h[1].key.clone()]).unwrap();assert_eq!(d.blocks[0].text,"张三看着李四");}
 #[test]fn all_episodes_and_boards_are_updated(){let mut d=ScreenplayDocument::blank("作品");d.blocks.push(ScreenplayBlock::new(BlockType::Action,"张三"));crate::series::edit(&mut d,"add","","第二集",&[]).unwrap();d.blocks.push(ScreenplayBlock::new(BlockType::Action,"张三"));d.settings.scene_board.cards.insert("card".into(),SceneCard{summary:"张三走了".into(),..Default::default()});let h=preview(&d,"张三","李四","all").unwrap();assert_eq!(h.len(),3);apply(&mut d,"张三","李四","all",&h.into_iter().map(|h|h.key).collect::<Vec<_>>()).unwrap();assert_eq!(d.blocks[0].text,"李四");assert_eq!(d.series.as_ref().unwrap().episodes[0].draft.as_ref().unwrap().blocks[0].text,"李四");assert_eq!(d.settings.scene_board.cards["card"].summary,"李四走了");}
 #[test]fn invalid_selection_does_not_mutate(){let mut d=ScreenplayDocument::blank("作品");d.blocks.push(ScreenplayBlock::new(BlockType::Action,"原文"));assert!(apply(&mut d,"原文","新文","current",&["bad".into()]).is_err());assert_eq!(d.blocks[0].text,"原文");}
}

#[cfg(test)]mod heading_tests{use super::*;
 #[test]fn archived_location_metadata_tracks_replacement(){let mut d=crate::serializer::from_fountain("INT. CAFE - DAY\n\nAction",None);crate::series::edit(&mut d,"add","","第二集",&[]).unwrap();let h=preview(&d,"CAFE","BAR","all").unwrap();apply(&mut d,"CAFE","BAR","all",&h.into_iter().map(|h|h.key).collect::<Vec<_>>()).unwrap();let first=d.series.as_ref().unwrap().episodes[0].draft.as_ref().unwrap();assert_eq!(first.scenes[0].location,"BAR");assert!(first.locations.iter().any(|l|l.name=="BAR"));}
}

#[cfg(test)]mod isolation_tests{use super::*;
 #[test]fn current_episode_replacement_does_not_change_archived_character_ids(){let mut d=crate::serializer::from_fountain("INT. ROOM - DAY\n\nALICE\nHello.",None);crate::series::edit(&mut d,"add","","第二集",&[]).unwrap();d.blocks.push(ScreenplayBlock::new(BlockType::Action,"目标"));let archive=d.series.as_ref().unwrap().episodes[0].draft.as_ref().unwrap().clone();let before=serde_json::to_string(archive.as_ref()).unwrap();let h=preview(&d,"目标","修改","current").unwrap();apply(&mut d,"目标","修改","current",&[h[0].key.clone()]).unwrap();let after=d.series.as_ref().unwrap().episodes[0].draft.as_ref().unwrap();assert_eq!(before,serde_json::to_string(after.as_ref()).unwrap());assert!(std::sync::Arc::ptr_eq(&archive,after));}
}

#[cfg(test)]mod preview_contract_tests{use super::*;
 #[test]fn preview_keys_cannot_be_reused_with_different_parameters(){let mut d=ScreenplayDocument::blank("测试");d.blocks.push(ScreenplayBlock::new(BlockType::Action,"张三来了"));let keys=preview(&d,"张三","李四","current").unwrap().into_iter().map(|h|h.key).collect::<Vec<_>>();for (q,r,s) in [("张","李四","current"),("张三","王五","current"),("张三","李四","all")]{assert!(apply(&mut d,q,r,s,&keys).is_err());assert_eq!(d.blocks[0].text,"张三来了");}d.revision+=1;assert!(apply(&mut d,"张三","李四","current",&keys).is_err());}
}
