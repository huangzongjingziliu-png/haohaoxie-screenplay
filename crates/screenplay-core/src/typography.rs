//! Explicit presentation profiles; screenplay semantics remain unchanged.
use crate::ast::{BlockType, ScreenplaySettings};
pub fn chinese(s: &ScreenplaySettings) -> bool { s.typography == "chinese-reading" }
pub fn indent(s: &ScreenplaySettings, t: BlockType) -> f64 {
    if !chinese(s) { return crate::pagination::indent_of(t); }
    match t { BlockType::Dialogue | BlockType::Parenthetical => 1.0, _ => 0.0 }
}
pub fn size_ratio(s: &ScreenplaySettings, t: BlockType) -> f64 {
    if chinese(s) && t == BlockType::Parenthetical { 0.85 } else { 1.0 }
}
pub fn bold(t: BlockType) -> bool { matches!(t, BlockType::SceneHeading | BlockType::Character | BlockType::Transition) }
/// Normalize the OCR spelling only in cue metadata, never in dialogue/actions.
pub fn normalize_continuation(text:&str)->String {
 let trimmed=text.trim();
 if matches!(trimmed.to_ascii_uppercase().as_str(),"CONTD"|"CONT'D"|"CONT’D") {return "CONT'D".into();}
 for (open,close) in [('（','）'),('(',')')] {
  if let Some(i)=trimmed.rfind(open).filter(|_|trimmed.ends_with(close)) {
   let inner=&trimmed[i+open.len_utf8()..trimmed.len()-close.len_utf8()];
   if matches!(inner.trim().to_ascii_uppercase().as_str(),"CONTD"|"CONT'D"|"CONT’D") {return format!("{}{open}CONT'D{close}",&trimmed[..i]);}
  }
 }
 text.into()
}
pub fn display_text(t: BlockType, text: &str) -> String {
    let canonical=if matches!(t,BlockType::Character|BlockType::Parenthetical){normalize_continuation(text)}else{text.into()};
    let text = canonical.trim();
    if t == BlockType::Parenthetical && !(text.starts_with('(') || text.starts_with('（')) { format!("（{text}）") } else { text.into() }
}
/// Only delivery qualifiers belong beside the name, never performance directions.
pub fn character_parts(text: &str) -> (&str, Option<&str>) {
    let text = text.trim();
    for (open, close) in [('（', '）'), ('(', ')')] {
        if let Some(i) = text.rfind(open) {
            if text.ends_with(close) {
                let qualifier = &text[i + open.len_utf8()..text.len() - close.len_utf8()];
                if is_delivery(qualifier) { return (text[..i].trim_end(), Some(&text[i..])); }
            }
        }
    }
    (text, None)
}
pub fn is_delivery(s: &str) -> bool {
    if s.contains([',','，']) {return s.split([',','，']).all(is_delivery);}
    matches!(s.trim().to_ascii_uppercase().as_str(), "内心独白" | "画外音" | "画外" | "旁白" | "离画" | "电话音" | "V.O." | "V.O" | "O.S." | "O.S" | "OFF" | "CONTD" | "CONT'D" | "CONT’D")
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn voice_qualifier_is_distinct_from_performance() {
        assert_eq!(character_parts("玛丽·卢（画外音）"), ("玛丽·卢", Some("（画外音）")));
        assert_eq!(character_parts("海关官员 (V.O.)"), ("海关官员", Some("(V.O.)")));
        assert_eq!(character_parts("小林（轻声）"), ("小林（轻声）", None));
        assert_eq!(display_text(BlockType::Parenthetical, "怀疑"), "（怀疑）");
        assert_eq!(display_text(BlockType::Parenthetical, "（怀疑）"), "（怀疑）");
    }
}

#[cfg(test)] mod compatibility_tests {
    use super::*;
    #[test] fn old_project_keeps_its_original_profile() {
        let mut old = serde_json::to_value(ScreenplaySettings::default()).unwrap();
        old.as_object_mut().unwrap().remove("typography");
        let restored: ScreenplaySettings = serde_json::from_value(old).unwrap();
        assert_eq!(restored.typography, "hollywood");
    }
}

/// Display-only label: never insert generated numbering into editable source text.
pub fn scene_label(doc: &crate::ast::ScreenplayDocument, id: &str) -> Option<String> {
    if !doc.settings.show_scene_numbers { return None; }
    let index = doc.blocks.iter().filter(|b| b.block_type == crate::ast::BlockType::SceneHeading).position(|b| b.id == id)?;
    let block = doc.blocks.iter().find(|b| b.id == id)?;
    let number = block.scene_heading.as_ref().and_then(|s| s.scene_number.as_deref()).map(str::to_owned).unwrap_or_else(|| (index + 1).to_string());
    Some(if chinese(&doc.settings) { format!("第{number}场") } else { format!("SCENE {number}") })
}

#[cfg(test)]
mod scene_tests {
    #[test]
    fn scene_labels_import_and_pagination() {
        let mut doc = crate::parser::parse("第十二场\n外景 纽约 - 白天\n\n行人穿过广场。", &Default::default()).document;
        assert_eq!(doc.blocks[0].block_type, crate::ast::BlockType::SceneHeading);
        doc.settings.typography = "chinese-reading".into();
        let plain = crate::pagination::paginate(&doc);
        doc.settings.show_scene_numbers = true;
        let numbered = crate::pagination::paginate(&doc);
        assert_eq!(super::scene_label(&doc, &doc.blocks[0].id).as_deref(), Some("第十二场"));
        assert_eq!(numbered.pages[0].blocks[0].line_count, plain.pages[0].blocks[0].line_count + 1.0);
        assert!(crate::serializer::to_html(&doc).contains("第十二场\n"));
        let roundtrip = crate::parser::parse(&crate::serializer::to_compact_source(&doc), &Default::default()).document;
        assert_eq!(roundtrip.blocks[0].scene_heading.as_ref().unwrap().scene_number.as_deref(), Some("十二"));
    }
}

/// A common baseline grid reserves room for the largest custom element.
pub fn grid_size(s: &ScreenplaySettings) -> f64 {
    s.element_styles.values().fold(s.font_size, |size, style| size.max(style.font_size))
}
pub fn style(s: &ScreenplaySettings, t: BlockType) -> crate::ast::ElementStyle {
    let mut style = s.element_styles.get(t.as_str()).cloned().unwrap_or_else(|| crate::ast::ElementStyle {
        font_family: if chinese(s) && t == BlockType::Character { "PingFang SC".into() } else { s.font_family.clone() },
        font_size: s.font_size * size_ratio(s, t), bold: bold(t), italic: false, color: "#000000".into(),
    });
    if t == BlockType::Parenthetical { style.bold = false; }
    style
}
pub fn units(s: &ScreenplaySettings, t: BlockType) -> f64 {
    let mut effective = s.clone();
    // Preserve historical default pagination; custom elements use their own size.
    if let Some(style) = s.element_styles.get(t.as_str()) { effective.font_size = style.font_size; }
    crate::pagination::units_per_line(&effective, indent(s, t))
}
pub fn validate_styles(styles: &std::collections::BTreeMap<String, crate::ast::ElementStyle>) -> bool {
    styles.len() <= 13 && styles.iter().all(|(key, style)| {
        ["scene_heading","action","character","parenthetical","dialogue","dual_dialogue","transition","shot","note","section","synopsis","general","page_break"].contains(&key.as_str())
        && style.font_size.is_finite() && (8.0..=36.0).contains(&style.font_size)
        && !style.font_family.trim().is_empty() && style.font_family.len() <= 200
        && style.font_family.chars().all(|c| c.is_alphanumeric() || " ,-'\"".contains(c))
        && style.color.len() == 7 && style.color.starts_with('#') && style.color[1..].bytes().all(|c| c.is_ascii_hexdigit())
    })
}
#[cfg(test)] mod custom_style_tests {
    use super::*;
    use crate::ast::{ElementStyle, ScreenplayBlock, ScreenplayDocument};
    #[test] fn project_roundtrip_default_compatibility_and_style_validation() {
        let mut doc = ScreenplayDocument::blank("样式测试");
        let old = serde_json::to_value(&doc).unwrap();
        let mut old = old; old["settings"].as_object_mut().unwrap().remove("element_styles"); old["settings"].as_object_mut().unwrap().remove("style_preset_name");
        let restored: ScreenplayDocument = serde_json::from_value(old).unwrap();
        assert!(restored.settings.element_styles.is_empty());
        doc.settings.element_styles.insert("action".into(), ElementStyle { font_family: "PingFang SC".into(), font_size: 24.0, bold: true, italic: true, color: "#2355aa".into() });
        doc.settings.style_preset_name = "我的模板".into();
        assert!(validate_styles(&doc.settings.element_styles));
        let restored: ScreenplayDocument = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
        assert_eq!(doc.settings, restored.settings);
        for _ in 0..100 { doc.blocks.push(ScreenplayBlock::new(BlockType::Action, "大字号动作。".repeat(5))); }
        let p = crate::pagination::paginate(&doc);
        assert!(p.total_pages > 2);
        assert!(p.pages.iter().all(|page| page.used_lines <= crate::pagination::usable_lines(&doc.settings)));
        let html = crate::serializer::to_html(&doc);
        assert!(html.contains("font-size:24pt;font-weight:700;font-style:italic;color:#2355aa"));
        doc.settings.element_styles.get_mut("action").unwrap().font_family = "Arial; color:red".into();
        assert!(!validate_styles(&doc.settings.element_styles));
    }
}

/// Repair legacy inline performance notes while preserving cue identity and voice extensions.
/// Locked blocks remain untouched; their presentation must be reviewed explicitly.
pub fn normalize_parentheticals(doc: &mut crate::ast::ScreenplayDocument) -> bool {
    use crate::ast::ScreenplayBlock;
    let mut changed=false; let mut blocks=Vec::with_capacity(doc.blocks.len());
    for mut block in std::mem::take(&mut doc.blocks) {
        let mut note=None;
        if block.block_type==BlockType::Character && !block.metadata.locked {
            let text=block.text.trim();
            for (open,close) in [('（','）'),('(',')')] {
                if let Some(index)=text.rfind(open).filter(|_|text.ends_with(close)) {
                    let qualifier=text[index+open.len_utf8()..text.len()-close.len_utf8()].trim();
                    if index>0 && !qualifier.is_empty() && !is_delivery(qualifier) {
                        let mut p=ScreenplayBlock::new(BlockType::Parenthetical,qualifier);
                        p.scene_id=block.scene_id.clone();note=Some(p);
                        block.text=text[..index].trim().to_string();changed=true;
                    }
                    break;
                }
            }
        }
        blocks.push(block);if let Some(note)=note {blocks.push(note);}
    }
    doc.blocks=blocks;
    if let Some(style)=doc.settings.element_styles.get_mut("parenthetical") {if style.bold {style.bold=false;changed=true;}}
    changed
}
#[cfg(test)] mod legacy_note_tests {
    use super::*;use crate::ast::*;
    #[test] fn legacy_cue_repair_keeps_identity_order_voice_and_plain_weight(){
        let mut doc=ScreenplayDocument::blank("旧工程");
        let cue=ScreenplayBlock::new(BlockType::Character,"小林（轻声）");let id=cue.id.clone();
        doc.blocks=vec![cue,ScreenplayBlock::new(BlockType::Dialogue,"等等。"),ScreenplayBlock::new(BlockType::Character,"小林（画外音）")];
        let mut custom=style(&doc.settings,BlockType::Parenthetical);custom.bold=true;doc.settings.element_styles.insert("parenthetical".into(),custom);
        assert!(!style(&doc.settings,BlockType::Parenthetical).bold);
        assert!(normalize_parentheticals(&mut doc));assert_eq!(doc.blocks[0].id,id);assert_eq!(doc.blocks[0].text,"小林");
        assert_eq!(doc.blocks[1].block_type,BlockType::Parenthetical);assert_eq!(doc.blocks[1].text,"轻声");assert_eq!(doc.blocks[2].text,"等等。");assert_eq!(doc.blocks[3].text,"小林（画外音）");
        assert!(!normalize_parentheticals(&mut doc));assert!(!doc.settings.element_styles["parenthetical"].bold);
    }
}

#[cfg(test)] mod continuation_tests {
 use super::*;
 #[test] fn ocr_metadata_spelling_is_canonical(){
  assert_eq!(normalize_continuation("CONTD"),"CONT'D");
  assert_eq!(normalize_continuation("人物（CONTD）"),"人物（CONT'D）");
  assert_eq!(display_text(BlockType::Parenthetical,"CONTD"),"（CONT'D）");
  assert_eq!(display_text(BlockType::Action,"纸上写着 CONTD。"),"纸上写着 CONTD。");
 }
 #[test] fn continuation_is_a_cue_qualifier_not_acting_direction(){
  let d=crate::parser::parse("@人物（CONTD）\n你好。\n\n@人物\n（CONT’D）\n再见。",&Default::default()).document;
  assert_eq!(d.blocks[0].text,"人物（CONT'D）");
  assert!(!d.blocks.iter().any(|b|b.block_type==BlockType::Parenthetical));
 }
}

/// Derived presentation only: source text and undo history stay untouched.
pub fn continued_cues(doc:&crate::ast::ScreenplayDocument)->std::collections::HashSet<String>{
 let mut ids=std::collections::HashSet::new();let mut speaker=String::new();let mut interrupted=false;
 for b in &doc.blocks {
  match b.block_type {
   BlockType::SceneHeading|BlockType::Transition=>{speaker.clear();interrupted=false;},
   BlockType::Character=>{
    let canonical=normalize_continuation(&b.text);let (name,extension)=character_parts(&canonical);
    if name==speaker && interrupted && !extension.is_some_and(|s|s.contains("CONT'D")){ids.insert(b.id.clone());}
    speaker=name.into();interrupted=false;
   },
   BlockType::Action|BlockType::Shot=>{if !speaker.is_empty(){interrupted=true;}},
   _=>{},
  }
 }
 ids
}
pub fn continued_cue(text:&str)->String{
 let canonical=normalize_continuation(text);let (_,ext)=character_parts(&canonical);
 if ext.is_some_and(|s|s.contains("CONT'D")){return canonical;}
 if ext.is_some(){let close=canonical.chars().last().unwrap();let end=canonical.len()-close.len_utf8();format!("{}, CONT'D{close}",&canonical[..end])}else{format!("{canonical} (CONT'D)")}
}

#[cfg(test)] mod automatic_continuation_tests {
 use super::*;
 #[test] fn action_interruption_adds_marker_but_scene_and_speaker_changes_reset(){
  let source="INT. ROOM - DAY\n\n@甲\n你好。\n\n!他看着窗外。\n\n@甲\n继续。\n\n@乙\n回答。\n\n!他转身。\n\n@甲\n新话。\n\nEXT. STREET - DAY\n\n@甲\n新场。";
  let d=crate::parser::parse(source,&Default::default()).document;
  let ids=continued_cues(&d);assert_eq!(ids.len(),1);
  let page=crate::pagination::paginate(&d);
  assert_eq!(page.pages.iter().flat_map(|p|&p.blocks).filter(|b|b.auto_contd==Some(true)).count(),1);
  assert!(!d.blocks.iter().any(|b|b.text.contains("CONT'D")));
  let mut edited=d.clone();edited.blocks.retain(|b|b.block_type!=BlockType::Action);assert!(continued_cues(&edited).is_empty());
  assert!(crate::serializer::to_html(&d).contains("CONT'D"));
 }
 #[test] fn explicit_and_voice_qualifiers_do_not_duplicate(){
  assert_eq!(continued_cue("甲（CONT'D）"),"甲（CONT'D）");
  assert_eq!(continued_cue("甲（画外音）"),"甲（画外音, CONT'D）");
  assert!(is_delivery("画外音, CONT'D"));
 }
}
