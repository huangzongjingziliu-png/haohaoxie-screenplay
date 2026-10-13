//! Conservative layout recognition before asking a model to infer screenplay roles.
//! Accept only a repeated, consistent cue/dialogue indentation pattern.
use crate::parser::{parse_scene_heading,strip_markdown};
#[derive(Clone,Copy,PartialEq)]enum Kind{Scene,Character,Parenthetical,Dialogue,Action,Transition}
fn indent(s:&str)->usize{s.chars().take_while(|c|*c==' '||*c=='\t').map(|c|if c=='\t'{4}else{1}).sum()}
fn cue(s:&str)->bool{let s=s.trim();!s.is_empty()&&s.chars().count()<=35&&!s.ends_with(['。','！','？','!','?','，',',','：',':'])&&!s.starts_with(['(','（'])}
pub fn structured_fountain(text:&str)->Option<String>{
 let lines=text.lines().collect::<Vec<_>>();
 let candidates=lines.iter().enumerate().filter(|(i,l)|{let n=indent(l);n>=20&&n<=42&&cue(l)&&lines[*i+1..].iter().find(|s|!s.trim().is_empty()).is_some_and(|next|indent(next)>=8&&indent(next)<n)}).map(|(_,l)|indent(l)).collect::<Vec<_>>();
 if candidates.len()<3 || !lines.iter().any(|l|parse_scene_heading(&strip_markdown(l.trim())).is_some()){return None;}
 let mut sorted=candidates.clone();sorted.sort();let cue_indent=sorted[sorted.len()/2];
 if candidates.iter().filter(|n|n.abs_diff(cue_indent)<=2).count()*100<candidates.len()*85{return None;}
 let mut parts:Vec<(Kind,String)>=Vec::new();let mut has_speaker=false;let mut action_break=false;
 for raw in lines {
  let value=strip_markdown(raw.trim());let value=value.trim();if value.is_empty(){action_break=true;continue;}
  let n=indent(raw);
  if n>=40&&value.trim_end_matches(['.','．']).chars().all(|c|c.is_ascii_digit()){continue;}
  let kind=if parse_scene_heading(value).is_some(){has_speaker=false;Kind::Scene}
   else if n.abs_diff(cue_indent)<=2&&cue(value){has_speaker=true;Kind::Character}
   else if n>=40&&matches!(value,"硬切"|"切至"|"淡出"|"淡入"|"CUT TO:"|"FADE OUT."){has_speaker=false;Kind::Transition}
   else if has_speaker&&n>=8&&n<cue_indent-2 {
    if value.starts_with(['(','（']) || (n>=cue_indent.saturating_sub(8))&&parts.last().is_some_and(|(k,v)|*k==Kind::Parenthetical&&!v.ends_with([')', '）'])){Kind::Parenthetical}else{Kind::Dialogue}
   }else{has_speaker=false;Kind::Action};
  if (kind!=Kind::Action||!action_break)&&matches!(kind,Kind::Action|Kind::Dialogue|Kind::Parenthetical)&&parts.last().is_some_and(|(k,v)|*k==kind&&(kind!=Kind::Parenthetical||!v.ends_with([')', '）']))) {
   let previous=&mut parts.last_mut().unwrap().1;
   if previous.chars().last().is_some_and(|c|c.is_ascii_alphanumeric())&&value.chars().next().is_some_and(|c|c.is_ascii_alphanumeric()){previous.push(' ');}
   previous.push_str(value);
  }else{parts.push((kind,value.to_string()));}
  action_break=false;
 }
 let mut result=String::new();
 for (kind,value) in parts {match kind{
  Kind::Scene=>{result.push_str(&format!("\n\n.{}\n",value.trim_start_matches('.')));},
  Kind::Character=>{result.push_str(&format!("\n\n@{}\n",crate::typography::normalize_continuation(&value)));},
  Kind::Parenthetical=>{let mut value=value;if !value.ends_with([')', '）']){value.push(if value.starts_with('（'){'）'}else{')'});}result.push_str(&format!("{value}\n"));},
  Kind::Dialogue=>{result.push_str(&format!("{value}\n"));},
  Kind::Action=>{result.push_str(&format!("\n\n!{value}\n"));},
  Kind::Transition=>{result.push_str(&format!("\n\n>{value}\n"));},
 }}
 Some(result.trim().into())
}
#[cfg(test)]mod tests{
 use super::*;use crate::ast::BlockType;
 #[test]fn layout_preserves_speakers_wrapped_speech_and_acting_notes(){
  let source="INT. 办公室 - 日\n动作。\n                        人物甲\n            第一句对白，\n            接着说完。\n                        人物乙\n                  （抬头，\n                  看着对方）\n            回答。\n                        人物甲（CONTD）\n            再说。";
  let result=structured_fountain(source).unwrap();let doc=crate::parser::parse(&result,&Default::default()).document;
  assert_eq!(doc.blocks.iter().filter(|b|b.block_type==BlockType::Character).count(),3);
  assert!(doc.blocks.iter().any(|b|b.block_type==BlockType::Dialogue&&b.text=="第一句对白，接着说完。"));
  assert!(doc.blocks.iter().any(|b|b.block_type==BlockType::Parenthetical&&b.text=="抬头，看着对方"));
 }
 #[test]fn ordinary_prose_and_markdown_do_not_enter_layout_path(){assert!(structured_fountain("INT. 办公室 - 日\n甲：你好。\n乙：回答。").is_none());}
}

#[cfg(test)]mod incomplete_parenthetical_regression {
 #[test]fn missing_bracket_does_not_swallow_the_next_less_indented_speech(){
  let input="INT. OFFICE - DAY\n                        人物甲\n                  （站起\n            说话。\n                        人物乙\n            回答。\n                        人物甲\n            再说。";
  let output=super::structured_fountain(input).unwrap();let d=crate::parser::parse(&output,&Default::default()).document;
  assert!(d.blocks.iter().any(|b|b.block_type==crate::ast::BlockType::Dialogue&&b.text=="说话。"));
  assert!(d.blocks.iter().any(|b|b.block_type==crate::ast::BlockType::Parenthetical&&b.text=="站起"));
 }
}
