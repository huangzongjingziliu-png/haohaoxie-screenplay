//! Structural AI Markdown adapter. Keeps the original text in import_info.
use crate::ast::{BlockType, StoryBible};
#[derive(Default)]
pub(crate) struct Prepared {
    pub lines: Vec<(String, Option<BlockType>)>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub bible: StoryBible,
}
fn heading(s: &str) -> Option<(usize, &str)> {
    let n = s.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&n) && s.as_bytes().get(n).is_some_and(u8::is_ascii_whitespace) { Some((n, s[n..].trim())) } else { None }
}
fn clean(s: &str) -> String {
    let mut s = s.trim().replace("&#x20;", " ").replace("&nbsp;", " ");
    for marker in ["**", "__"] { s = s.replace(marker, ""); }
    if s.starts_with('*') && s.ends_with('*') && s.len() > 2 { s = s[1..s.len()-1].to_string(); }
    s.trim().to_string()
}
fn separator(s: &str) -> bool {
    let s = s.trim_start_matches('\\').replace(' ', "");
    s.len() >= 3 && s.chars().all(|c| "-*_".contains(c))
}
fn cue_parts(s: &str) -> (&str, Option<&str>) {
    for (o, c) in [('（','）'), ('(',')')] {
        if let Some(i) = s.rfind(o) { if s.ends_with(c) { return (s[..i].trim(), Some(s[i+o.len_utf8()..s.len()-c.len_utf8()].trim())); } }
    }
    (s.trim(), None)
}
fn narrative(s: &str, names: &[String]) -> bool {
    names.iter().any(|name| s.starts_with(name) && s != name)
        || ["他的", "他抬", "他走", "他检查", "他看", "他勒", "他最终", "他没有", "他停", "她用", "她倒", "她把", "她伸", "她端", "她想", "她笑", "她停", "她趴", "她最终", "她仍", "她的", "她像", "她似乎", "二人", "两人", "两个人", "两只酒碗", "酒肆里", "酒落", "风继续", "片刻", "停顿", "顿了顿", "一饮而尽", "没有回头", "许久", "很轻", "很短", "然后继续"]
            .iter().any(|prefix| s.starts_with(prefix))
}
pub(crate) fn prepare(text: &str) -> Option<Prepared> {
    // Do not reinterpret ordinary Fountain sections as Markdown speaker headings.
    if !text.lines().any(|l| heading(l.trim()).is_some_and(|(n,s)| n >= 3 && !s.is_empty())) { return None; }
    let mut out = Prepared::default();
    let mut names = Vec::new();
    let mut bios = false;
    let mut bio_name: Option<String> = None;
    let mut spoken = false;
    let mut seen_scene = false;
    for line in text.lines() {
        let raw = line.trim();
        let h = heading(raw);
        let s = clean(h.map_or(raw, |(_,s)| s));
        if s.is_empty() || separator(&s) { continue; }
        if let Some((level,_)) = h {
            if level == 1 && (s.starts_with('《') && s.ends_with('》')) {
                if out.title.is_none() { out.title = Some(s.clone()); }
                continue;
            }
            if s == "人物" || s == "人物介绍" { bios = true; spoken = false; continue; }
            if s == "正文" || s == "剧本正文" { bios = false; bio_name = None; spoken = false; continue; }
        }
        if !seen_scene {
            if let Some(author) = s.strip_prefix("编剧：").or_else(|| s.strip_prefix("编剧:")) { out.author = Some(author.trim().into()); continue; }
            if let Some(era) = s.strip_prefix("时代背景：").or_else(|| s.strip_prefix("时代背景:")) { out.bible.era = era.trim().into(); continue; }
        }
        if crate::parser::parse_scene_heading(&s).is_some() {
            bios = false; seen_scene = true; spoken = false;
            out.lines.push((s, None)); continue;
        }
        if bios {
            if raw.starts_with("**") && raw.ends_with("**") || h.is_some_and(|(n,_)| n >= 3) {
                bio_name = Some(s.clone()); names.push(s.clone()); out.bible.character_bios.entry(s).or_default();
            } else if let Some(name) = &bio_name {
                let bio = out.bible.character_bios.entry(name.clone()).or_default();
                if !bio.is_empty() { bio.push('\n'); } bio.push_str(&s);
            } else { out.bible.world.push_str(&format!("{s}\n")); }
            continue;
        }
        if h.is_some_and(|(n,_)| n >= 3) {
            let (name, extra) = cue_parts(&s);
            if !name.is_empty() && name.chars().count() <= 24 && !name.contains(['。','！','？',':','：']) {
                if !names.iter().any(|n| n == name) { names.push(name.into()); }
                let delivery = extra.is_some_and(crate::typography::is_delivery);
                out.lines.push((if delivery { s.clone() } else { name.into() }, Some(BlockType::Character)));
                if let Some(extra) = extra.filter(|_| !delivery) { out.lines.push((extra.into(), Some(BlockType::Parenthetical))); }
                spoken = true; continue;
            }
        }
        if s == "完" && seen_scene { spoken = false; out.lines.push((s, Some(BlockType::Action))); continue; }
        if crate::parser::is_transition(&s) || crate::parser::is_shot(&s) { spoken = false; out.lines.push((s, None)); continue; }
        if narrative(&s, &names) { spoken = false; }
        out.lines.push((s, Some(if spoken { BlockType::Dialogue } else { BlockType::Action })));
    }
    Some(out)
}

#[cfg(test)] mod tests {
    use crate::{parse, ParseOptions, ast::BlockType};
    #[test] fn synthetic_markdown_has_eleven_scenes_and_clean_cues() {
        let text = include_str!("../fixtures/synthetic-markdown.md");
        let doc = parse(text, &ParseOptions { denoise: true, ..Default::default() }).document;
        let scenes: Vec<_> = doc.blocks.iter().filter(|b| b.block_type == BlockType::SceneHeading).collect();
        assert_eq!(scenes.len(), 11);
        assert_eq!(scenes[2].scene_heading.as_ref().unwrap().time_of_day, "入夜");
        assert_eq!(doc.metadata.title, "《测试剧本》");
        assert_eq!(doc.metadata.author, "Test Author");
        assert!(doc.story_bible.character_bios["林"].contains("三十余岁"));
        assert!(doc.blocks.iter().all(|b| !b.text.contains('#') && !b.text.contains("**")));
        assert!(doc.blocks.iter().any(|b| b.block_type == BlockType::Character && b.text == "林（内心独白）"));
        assert!(doc.blocks.iter().any(|b| b.block_type == BlockType::Parenthetical && b.text == "悄声"));
        assert!(doc.blocks.iter().any(|b| b.block_type == BlockType::Dialogue && b.text == "请保持安静。"));
        assert!(doc.blocks.iter().any(|b| b.block_type == BlockType::Action && b.text == "林检查窗户。"));
        assert_eq!(doc.import_info.as_ref().unwrap().original_text, text);
    }
    #[test] fn markdown_does_not_strip_ordinary_fountain_sections() {
        assert!(super::prepare("# 第一幕\n\n外 城门 黄昏\n\n普通动作。").is_none());
        assert!(crate::parser::parse_scene_heading("外面的马偶尔打一个响鼻。").is_none());
        assert!(crate::parser::parse_scene_heading("外面的街已经完全安静。").is_none());
    }
}
