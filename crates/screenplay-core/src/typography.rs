//! Explicit presentation profiles; screenplay semantics remain unchanged.
use crate::ast::{BlockType, ScreenplaySettings};
pub const HOLLYWOOD: &str = "hollywood";
pub const CHINESE_READING: &str = "chinese-reading";
pub const CHINESE_PRODUCTION: &str = "chinese-production";
pub const CHINESE_GENERAL: &str = "chinese-general";
pub fn chinese(s: &ScreenplaySettings) -> bool { s.typography == CHINESE_READING }
/// 中式剧本（△ 标记）：国内短剧/平台稿方言，动作行以 △ 引导、对白与人物名同一行。
pub fn production(s: &ScreenplaySettings) -> bool { s.typography == CHINESE_PRODUCTION }
/// 中文通用剧本：传统电影文学剧本版式，不加 △ 或平台稿符号。
pub fn general(s: &ScreenplaySettings) -> bool { s.typography == CHINESE_GENERAL }
pub fn indent(s: &ScreenplaySettings, t: BlockType) -> f64 {
    if production(s) || general(s) { return 0.0; }
    if !chinese(s) { return crate::pagination::indent_of(t); }
    match t { BlockType::Dialogue | BlockType::Parenthetical => 1.0, _ => 0.0 }
}
pub fn size_ratio(s: &ScreenplaySettings, t: BlockType) -> f64 {
    if chinese(s) && t == BlockType::Parenthetical { 0.85 } else { 1.0 }
}
pub fn bold(t: BlockType) -> bool { matches!(t, BlockType::SceneHeading | BlockType::Character | BlockType::Transition) }
pub fn bold_in(s: &ScreenplaySettings, t: BlockType) -> bool {
    if production(s) || general(s) { t == BlockType::SceneHeading } else { bold(t) }
}
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
/// 合并行「人物（情态）：台词」在跨页续排时需要用正文里的人物名做续页提示。
/// 只在两种合并预设下生效，且只取冒号前、长度合理的片段，避免把普通台词误当人物名。
pub fn merged_speaker(s: &ScreenplaySettings, text: &str) -> Option<String> {
    if !(production(s) || general(s)) { return None; }
    let cue = text.split_once('：')?.0.trim();
    if cue.is_empty() || cue.chars().count() > 40 { return None; }
    Some(cue.to_string())
}
/// Presentation-only text for the selected profile. 中式剧本的动作行补 `△`，
/// 但用户已经手打的 `△` 不会重复；其余预设保持原样。
pub fn display_text_with(s: &ScreenplaySettings, t: BlockType, text: &str) -> String {
    let base = display_text(t, text);
    if production(s) && t == BlockType::Action && !base.trim().is_empty() && !base.trim_start().starts_with('△') {
        format!("△{}", base.trim_start())
    } else { base }
}
/// 中式剧本的场景头：`12-日-办公室-内`（场号-时间-地点-内外）。
/// 中文通用剧本：`12、办公室，日，内`（场号、地点、时间、内外）。
/// 场号开关关闭时省略场号；内外景未知时省略最后一段。
/// 缓存的场头元数据是否仍能代表当前正文。
/// 只检查地点与时间：场号是独立编号，改名时应保留；
/// 内景/外景常被模板改写顺序，不作强校验。
fn heading_matches_text(data: &crate::ast::SceneHeadingData, text: &str) -> bool {
    let contained = |value: &str| value.trim().is_empty() || text.contains(value.trim());
    contained(&data.location) && contained(&data.time_of_day)
}
pub fn scene_heading_display(doc: &crate::ast::ScreenplayDocument, b: &crate::ast::ScreenplayBlock) -> Option<String> {
    if !(production(&doc.settings) || general(&doc.settings)) || b.block_type != BlockType::SceneHeading { return None; }
    // 正文是唯一事实来源：编辑器改过场景头后，缓存的元数据可能已过期，
    // 因此先用当前文本解析，元数据只用于补回只存在于元数据里的场号。
    let parsed = crate::heading_format::format_heading(&b.text, &doc.settings)
        .map(|(_, d)| d)
        .or_else(|| crate::parser::parse_scene_heading(&b.text).map(|(d, _)| d))
        .or_else(|| {
            // 自定义模板或纯文本场头可能解析不出结构化数据，此时才退回元数据；
            // 但若元数据里的地点/时间已不在正文中，说明它过期了，不能再用。
            b.scene_heading.clone().filter(|d| heading_matches_text(d, &b.text))
        });
    let Some(mut data) = parsed else {
        // 解析不了的场景头也保留场号，避免编辑器有号、导出没号。
        if !doc.settings.show_scene_numbers || b.text.trim().is_empty() { return None; }
        let index = doc.blocks.iter().filter(|x| x.block_type == BlockType::SceneHeading).position(|x| x.id == b.id)?;
        let number = b.scene_heading.as_ref().and_then(|d| d.scene_number.clone())
            .filter(|n| !n.trim().is_empty()).unwrap_or_else(|| (index + 1).to_string());
        let text = b.text.trim();
        return Some(if production(&doc.settings) { format!("{number}-{text}") } else { format!("{number}、{text}") });
    };
    if data.scene_number.as_deref().map_or(true, |n| n.trim().is_empty()) {
        if let Some(n) = b.scene_heading.as_ref().and_then(|d| d.scene_number.clone()).filter(|n| !n.trim().is_empty()) {
            data.scene_number = Some(n);
        }
    }
    if data.location.trim().is_empty() { return None; }
    let number = if doc.settings.show_scene_numbers {
        let index = doc.blocks.iter().filter(|x| x.block_type == BlockType::SceneHeading).position(|x| x.id == b.id)?;
        Some(data.scene_number.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| (index + 1).to_string()))
    } else { None };
    let int_ext = data.int_ext.to_zh();
    if production(&doc.settings) {
        let mut parts = Vec::new();
        if let Some(n) = number { parts.push(n); }
        if !data.time_of_day.trim().is_empty() { parts.push(data.time_of_day.trim().into()); }
        parts.push(data.location.trim().into());
        if !int_ext.is_empty() { parts.push(int_ext.into()); }
        Some(parts.join("-"))
    } else {
        let mut rest = vec![data.location.trim().to_string()];
        if !data.time_of_day.trim().is_empty() { rest.push(data.time_of_day.trim().into()); }
        if !int_ext.is_empty() { rest.push(int_ext.into()); }
        Some(match number { Some(n) => format!("{}、{}", n, rest.join("，")), None => rest.join("，") })
    }
}
/// 导出用的呈现副本：中式剧本与中文通用剧本把「人物 + 可选括注 + 对白」合并成一行。
/// 原始文档与撤销历史保持不变；其他预设原样返回。
pub fn export_view(doc: &crate::ast::ScreenplayDocument) -> crate::ast::ScreenplayDocument {
    if !(production(&doc.settings) || general(&doc.settings)) { return doc.clone(); }
    let mut out = doc.clone();
    let source = doc.blocks.clone();
    let mut blocks = Vec::with_capacity(source.len());
    let mut i = 0usize;
    while i < source.len() {
        let b = &source[i];
        if b.block_type == BlockType::Character {
            let (base, ext) = character_parts(&b.text);
            let mut j = i + 1;
            let mut paren: Option<String> = None;
            if let Some(next) = source.get(j).filter(|n| n.block_type == BlockType::Parenthetical) {
                let p = next.text.trim();
                let p = p.strip_prefix('（').and_then(|v| v.strip_suffix('）'))
                    .or_else(|| p.strip_prefix('(').and_then(|v| v.strip_suffix(')')))
                    .unwrap_or(p);
                if !p.trim().is_empty() { paren = Some(p.trim().into()); }
                j += 1;
            }
            if let Some(dialogue) = source.get(j).filter(|n| n.block_type == BlockType::Dialogue && !n.text.trim().is_empty()) {
                if !base.trim().is_empty() {
                    let mut text = format!("{}{}", base.trim(), ext.unwrap_or(""));
                    if let Some(p) = paren { text.push('（'); text.push_str(&p); text.push('）'); }
                    text.push('：');
                    text.push_str(dialogue.text.trim());
                    let mut merged = b.clone();
                    merged.block_type = BlockType::Dialogue;
                    merged.text = text;
                    blocks.push(merged);
                    i = j + 1;
                    continue;
                }
            }
        }
        blocks.push(b.clone());
        i += 1;
    }
    out.blocks = blocks;
    out
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
    if !doc.settings.show_scene_numbers || production(&doc.settings) || general(&doc.settings) { return None; }
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
        font_size: s.font_size * size_ratio(s, t), bold: bold_in(s, t), italic: false, color: "#000000".into(),
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

#[cfg(test)] mod production_tests {
 use super::*;use crate::ast::*;
 fn production_doc()->ScreenplayDocument{
  let mut d=crate::parser::parse("内景 办公室 - 日\n\n李明擦汗后退，撞到墙。\n\n李明（低声）：报告……",&Default::default()).document;
  d.settings.typography=CHINESE_PRODUCTION.into();d.settings.font_size=12.0;d.settings.line_height=1.5;d
 }
 #[test] fn scene_heading_is_number_time_location_int_ext(){
  let mut d=production_doc();d.settings.show_scene_numbers=true;
  assert_eq!(scene_heading_display(&d,&d.blocks[0]).as_deref(),Some("1-日-办公室-内"));
  d.settings.show_scene_numbers=false;
  assert_eq!(scene_heading_display(&d,&d.blocks[0]).as_deref(),Some("日-办公室-内"));
 }
 #[test] fn action_gets_single_delta_prefix(){
  assert_eq!(display_text_with(&production_doc().settings,BlockType::Action,"李明擦汗"),"△李明擦汗");
  assert_eq!(display_text_with(&production_doc().settings,BlockType::Action,"△李明擦汗"),"△李明擦汗");
  let mut d=production_doc();d.settings.typography=HOLLYWOOD.into();
  assert_eq!(display_text_with(&d.settings,BlockType::Action,"李明擦汗"),"李明擦汗");
 }
 #[test] fn production_heading_input_is_recognized(){
  let (data,display)=crate::parser::parse_scene_heading("12-日-办公室-内").unwrap();
  assert_eq!(data.scene_number.as_deref(),Some("12"));
  assert_eq!(data.location,"办公室");
  assert_eq!(data.time_of_day,"日");
  assert_eq!(data.int_ext.to_zh(),"内");
  assert!(display.contains("办公室"));
  assert!(crate::parser::parse_scene_heading("内景 办公室 - 日").is_some());
 }
 #[test] fn general_profile_has_no_platform_marks(){
  let mut d=production_doc();
  d.settings.typography=CHINESE_GENERAL.into();
  d.settings.show_scene_numbers=true;
  assert_eq!(scene_heading_display(&d,&d.blocks[0]).as_deref(),Some("1、办公室，日，内"));
  assert_eq!(display_text_with(&d.settings,BlockType::Action,"李明擦汗"),"李明擦汗");
  let html=crate::serializer::to_html(&d);
  assert!(html.contains("李明（低声）：报告"));
  assert!(!html.contains('△'));
  assert!(html.contains("chinese-general"));
 }
 #[test] fn export_view_merges_one_dialogue_line_without_touching_source(){
  let mut d=production_doc();
  d.blocks=vec![ScreenplayBlock::new(BlockType::Character,"李明"),ScreenplayBlock::new(BlockType::Parenthetical,"低声"),ScreenplayBlock::new(BlockType::Dialogue,"报告……")];
  let view=export_view(&d);
  assert_eq!(view.blocks.len(),1);
  assert_eq!(view.blocks[0].block_type,BlockType::Dialogue);
  assert_eq!(view.blocks[0].text,"李明（低声）：报告……");
  assert_eq!(d.blocks.len(),3);
  assert_eq!(d.blocks[0].block_type,BlockType::Character);
 }
 #[test] fn empty_action_gets_no_marker(){
  let d=production_doc();
  let s=&d.settings;
  assert_eq!(display_text_with(s,BlockType::Action,"  "),"");
  assert_eq!(display_text_with(s,BlockType::Action,"他站起。"),"△他站起。");
  assert_eq!(display_text_with(s,BlockType::Action,"△他站起。"),"△他站起。");
 }
 #[test] fn other_profiles_are_untouched(){
  let mut d=production_doc();d.settings.typography=CHINESE_READING.into();
  let view=export_view(&d);
  assert_eq!(view.blocks.len(),d.blocks.len());
 }
 #[test] fn edited_heading_text_wins_over_cached_metadata(){
  let mut d=production_doc();d.settings.show_scene_numbers=true;
  d.blocks[0].text="1-夜-卧室-外".into();
  assert_eq!(scene_heading_display(&d,&d.blocks[0]).as_deref(),Some("1-夜-卧室-外"));
 }
 #[test] fn unparsed_heading_keeps_number_in_exports(){
  let mut d=ScreenplayDocument::blank("x");
  d.settings.typography=CHINESE_GENERAL.into();
  d.settings.show_scene_numbers=true;
  d.blocks.push(ScreenplayBlock::new(BlockType::SceneHeading,"字幕：三日后"));
  assert_eq!(scene_heading_display(&d,&d.blocks[0]).as_deref(),Some("1、字幕：三日后"));
 }
 #[test] fn stale_heading_metadata_is_ignored_after_rename(){
  let mut d=ScreenplayDocument::blank("x");
  d.settings.typography=CHINESE_PRODUCTION.into();
  d.settings.show_scene_numbers=true;
  let mut b=ScreenplayBlock::new(BlockType::SceneHeading,"字幕：三日后");
  b.scene_heading=Some(SceneHeadingData{scene_number:Some("3".into()),location:"办公室".into(),time_of_day:"日".into(),int_ext:IntExt::Int,..Default::default()});
  d.blocks.push(b);
  assert_eq!(scene_heading_display(&d,&d.blocks[0]).as_deref(),Some("3-字幕：三日后"));
 }
 #[test] fn cached_scene_number_survives_text_edit(){
  let mut d=ScreenplayDocument::blank("x");
  let mut b=ScreenplayBlock::new(BlockType::SceneHeading,"咖啡馆 / 内景 / 日");
  b.scene_heading=Some(SceneHeadingData{scene_number:Some("12A".into()),..Default::default()});
  d.blocks.push(b);
  d.settings.typography=CHINESE_GENERAL.into();
  d.settings.show_scene_numbers=true;
  d.settings.scene_heading_template="{location} / {type} / {time}".into();
  assert_eq!(scene_heading_display(&d,&d.blocks[0]).as_deref(),Some("12A、咖啡馆，日，内"));
 }
}
