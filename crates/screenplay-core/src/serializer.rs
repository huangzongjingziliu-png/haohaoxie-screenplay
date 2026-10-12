//! 序列化：AST → 自由文本 / Fountain / FDX，以及反向解析。

use crate::ast::*;
use crate::parser::{self, ParseOptions};
use regex::Regex;
use std::sync::OnceLock;

fn strip_markdown(s: &str) -> String {
    let mut s = s.trim_start().to_string();
    while s.starts_with('#') {
        s.remove(0);
        s = s.trim_start().to_string();
    }
    s = s.trim_start_matches('>').trim_start().to_string();
    s
}

// ---------------------------------------------------------------------------
// 自由文本（右侧编辑器，极简中文语法）
// ---------------------------------------------------------------------------

/// AST → 自由文本。连续的 character(+parenthetical)+dialogue 合并为一行 `人物（括注）：对白`
pub fn to_compact_source(doc: &ScreenplayDocument) -> String {
    let mut parts: Vec<String> = Vec::new();
    let blocks = &doc.blocks;
    let mut i = 0usize;
    while i < blocks.len() {
        let b = &blocks[i];
        match b.block_type {
            BlockType::Character => {
                let has_paren = blocks
                    .get(i + 1)
                    .is_some_and(|x| x.block_type == BlockType::Parenthetical);
                let d_idx = if has_paren { i + 2 } else { i + 1 };
                let is_dialogue = blocks
                    .get(d_idx)
                    .is_some_and(|x| x.block_type == BlockType::Dialogue);
                if is_dialogue {
                    let mut line = b.text.clone();
                    if has_paren {
                        line.push('（');
                        line.push_str(&blocks[i + 1].text);
                        line.push('）');
                    }
                    line.push('：');
                    line.push_str(&blocks[d_idx].text);
                    parts.push(line);
                    i = d_idx + 1;
                    continue;
                }
                parts.push(b.text.clone());
                i += 1;
            }
            BlockType::Parenthetical => {
                parts.push(format!("（{}）", b.text));
                i += 1;
            }
            BlockType::SceneHeading => {
                let display = if parser::parse_scene_heading(&b.text).is_some() { b.text.clone() } else { format!(".{}",b.text) };
                let num = b
                    .scene_heading
                    .as_ref()
                    .and_then(|d| d.scene_number.clone());
                match num {
                    Some(n) if display.starts_with('.') => parts.push(format!(".{}. {}", n, &display[1..])),
                    Some(n) => parts.push(format!("{}. {}", n, strip_markdown(&display))),
                    None => parts.push(display),
                }
                i += 1;
            }
            BlockType::PageBreak => {
                parts.push("===".to_string());
                i += 1;
            }
            BlockType::Note => {
                parts.push(format!("[[{}]]", b.text));
                i += 1;
            }
            BlockType::Section => {
                parts.push(format!("# {}", b.text));
                i += 1;
            }
            BlockType::Synopsis => {
                parts.push(format!("= {}", b.text));
                i += 1;
            }
            _ => {
                parts.push(b.text.clone());
                i += 1;
            }
        }
    }
    parts.join("\n\n")
}

// ---------------------------------------------------------------------------
// Fountain
// ---------------------------------------------------------------------------

pub fn to_fountain(doc: &ScreenplayDocument) -> String {
    let mut out = String::new();
    if !doc.title_page.title.is_empty() {
        out.push_str(&format!("Title: {}\n", doc.title_page.title));
        if let Some(w) = &doc.title_page.written_by {
            out.push_str(&format!("Credit: {}\n", w));
        }
        if let Some(a) = &doc.title_page.author {
            out.push_str(&format!("Author: {}\n", a));
        }
        if let Some(d) = &doc.title_page.draft_date {
            out.push_str(&format!("Draft date: {}\n", d));
        }
        if let Some(c) = &doc.title_page.contact {
            out.push_str(&format!("Contact: {}\n", c));
        }
        out.push('\n');
    }

    for b in &doc.blocks {
        match b.block_type {
            BlockType::SceneHeading => out.push_str(&format!("{}\n", b.text.to_uppercase())),
            BlockType::Character | BlockType::DualDialogue => {
                out.push_str(&format!("@{}\n", b.text))
            }
            BlockType::Parenthetical => out.push_str(&format!("({})\n", b.text)),
            BlockType::PageBreak => out.push_str("===\n"),
            BlockType::Note => out.push_str(&format!("[[{}]]\n", b.text)),
            BlockType::Section => out.push_str(&format!("# {}\n", b.text)),
            BlockType::Synopsis => out.push_str(&format!("= {}\n", b.text)),
            _ => out.push_str(&format!("{}\n", b.text)),
        }
        out.push('\n');
    }
    out
}

/// Fountain → AST
pub fn from_fountain(text: &str, title: Option<String>) -> ScreenplayDocument {
    let o = parser::parse(text, &ParseOptions {
        title,
        denoise: false,
        ..Default::default()
    });
    o.document
}

// ---------------------------------------------------------------------------
// FDX (Final Draft XML)
// ---------------------------------------------------------------------------

pub fn to_fdx(doc: &ScreenplayDocument) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\n");
    out.push_str("<FinalDraft DocumentType=\"Script\" Template=\"No\" Version=\"2\">\n");
    out.push_str("  <Content>\n");
    for b in &doc.blocks {
        let t = b.block_type.to_fdx();
        out.push_str(&format!("    <Paragraph Type=\"{}\">\n", t));
        out.push_str(&format!("      <Text>{}</Text>\n", escape_xml(&b.text)));
        if b.block_type == BlockType::SceneHeading {
            if let Some(d) = &b.scene_heading {
                out.push_str(&format!(
                    "      <SceneProperties INT=\"{}\" EXT=\"{}\" />\n",
                    if d.int_ext == IntExt::Int { "Yes" } else { "No" },
                    if d.int_ext == IntExt::Ext { "Yes" } else { "No" }
                ));
            }
        }
        out.push_str("    </Paragraph>\n");
    }
    out.push_str("  </Content>\n");
    out.push_str("</FinalDraft>\n");
    out
}

/// FDX XML → AST
///
/// 使用轻量扫描而非完整 XML 解析器，但**必须**对非法 XML 给出错误而不是 panic。
pub fn from_fdx(xml: &str, title: Option<String>) -> Result<ScreenplayDocument, String> {
    // 基本合法性检查
    if !xml.contains("<FinalDraft") {
        return Err("不是有效的 FDX 文件：缺少 <FinalDraft> 根元素".into());
    }

    let para_re = paragraph_regex();
    let text_re = text_regex();
    let int_re = Regex::new(r#"INT="(Yes|No)""#).unwrap();

    let mut blocks: Vec<ScreenplayBlock> = Vec::new();
    for cap in para_re.captures_iter(xml) {
        let whole = cap.get(0).unwrap();
        let ptype = cap.name("type").map(|m| m.as_str()).unwrap_or("General");
        let bt = BlockType::from_fdx(ptype).unwrap_or(BlockType::General);
        let inner = &xml[whole.start()..whole.end()];
        let text = text_re
            .captures(inner)
            .map(|c| unescape_xml(c.name("t").unwrap().as_str()))
            .unwrap_or_default();
        let mut b = ScreenplayBlock::new(bt, text);
        if bt == BlockType::SceneHeading {
            let is_int = int_re
                .captures(inner)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str() == "Yes")
                .unwrap_or(false);
            let loc = b.text.clone();
            b.scene_heading = Some(SceneHeadingData {
                scene_number: None,
                int_ext: if is_int { IntExt::Int } else { IntExt::Ext },
                location: loc,
                time_of_day: String::new(),
                display_language: "en".into(),
            });
        }
        blocks.push(b);
    }

    let mut doc = ScreenplayDocument::blank(title.as_deref().unwrap_or("导入的剧本"));
    doc.blocks = blocks;
    doc.rebuild_derived();
    doc.import_info = Some(ImportInfo {
        source_type: SourceType::Fdx,
        source_name: None,
        imported_at: now_iso(),
        original_text: xml.to_string(),
        removed_noise: Vec::new(),
        preserve_original: true,
    });
    Ok(doc)
}

fn paragraph_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?s)<Paragraph\s+Type="(?P<type>[^"]*)"\s*>(?P<body>.*?)</Paragraph>"#).unwrap())
}

fn text_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?s)<Text>(?P<t>.*?)</Text>"#).unwrap())
}

pub fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn unescape_xml(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

// ---------------------------------------------------------------------------
// 纯文本 / HTML
// ---------------------------------------------------------------------------

pub fn to_plain_text(doc: &ScreenplayDocument) -> String {
    doc.blocks
        .iter()
        .map(|b| b.text.as_str())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn to_html(doc: &ScreenplayDocument) -> String {
    let mut body = String::new();
    for b in &doc.blocks {
        let cls = b.block_type.as_str();
        let mut display = crate::typography::display_text(b.block_type, &b.text);
        if let Some(label) = crate::typography::scene_label(doc, &b.id) { display = format!("{label}\n{display}"); }
        let (name, extension) = crate::typography::character_parts(&display);
        let txt = if b.block_type == BlockType::Character && extension.is_some() {
            format!("<span class=\"cue-name\">{}</span><span class=\"cue-extension\">{}</span>", escape_html(name), escape_html(extension.unwrap()))
        } else if crate::typography::chinese(&doc.settings) && ["镜头切换。", "镜头切换.", "蒙太奇。", "蒙太奇."].iter().any(|p| display.starts_with(p)) {
            let (i, c) = display.char_indices().find(|(_, c)| *c == '。' || *c == '.').unwrap();
            let n = if display.starts_with("蒙太奇") { i } else { i + c.len_utf8() };
            format!("<strong>{}</strong>{}", escape_html(&display[..n]), escape_html(&display[n..]))
        } else { escape_html(&display) };
        let custom = doc.settings.element_styles.get(cls).map(|style| format!(" style=\"font-family:{};font-size:{}pt;font-weight:{};font-style:{};color:{}\"", escape_html(&style.font_family), style.font_size, if b.block_type != BlockType::Parenthetical && style.bold { 700 } else { 400 }, if style.italic { "italic" } else { "normal" }, escape_html(&style.color))).unwrap_or_default();
        body.push_str(&format!("<div class=\"{}\"{}>{}</div>\n", cls, custom, txt));
    }
    let chinese = crate::typography::chinese(&doc.settings);
    let profile = if chinese { "chinese-reading" } else { "hollywood" };
    let extra = if chinese { format!("body{{font-family:'Songti SC',SimSun,serif;font-size:{}pt;line-height:{};}} .character{{margin-left:0;text-align:center;font-family:'PingFang SC',sans-serif;font-weight:700;}} .cue-extension{{position:absolute;margin-left:.3em;font-weight:400;font-size:85%;font-family:'Songti SC',SimSun,serif;}} .parenthetical{{margin:0 1in;text-align:center;font-size:85%;}} .dialogue{{margin:0 1in;}} .transition{{text-align:left;font-weight:700;}}", doc.settings.font_size, doc.settings.line_height) } else { ".character{font-weight:700}.cue-extension{font-weight:400}".into() };
    format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN"><head><meta charset="utf-8"><title>{}</title>
<style>
body{{font-family:"Courier Prime",Courier,"PingFang SC",monospace;max-width:8.5in;margin:0 auto;padding:1in;}}
.scene_heading{{font-weight:bold;text-transform:uppercase;margin-top:2em;}}
.character{{text-transform:uppercase;margin-left:2.5in;margin-top:1em;}}
.parenthetical{{margin-left:2in;}}
.dialogue{{margin-left:1.5in;margin-right:1.5in;}}
.transition{{text-align:right;text-transform:uppercase;}}
{extra}
div{{white-space:pre-wrap;}}
</style></head><body class="{profile}">
<h1>{}</h1>
{}</body></html>"#,
        escape_html(&doc.title_page.title),
        escape_html(&doc.title_page.title),
        body
    )
}

pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ParseOptions;

    fn doc_from(text: &str) -> ScreenplayDocument {
        parser::parse(
            text,
            &ParseOptions {
                denoise: false,
                ..Default::default()
            },
        )
        .document
    }

    const SAMPLE: &str = "\
外 镇北堡城门 黄昏

风从荒原吹来。

剑客（内心独白）：我是一个爱做梦的人。

老板娘：昨日，去了买卖城。

切至：

内 酒肆 夜

老板娘倒酒。";

    #[test]
    fn r01_fountain_roundtrip_no_text_loss() {
        let d1 = doc_from(SAMPLE);
        let f = to_fountain(&d1);
        let d2 = from_fountain(&f, Some("t".into()));

        // 所有非空文字必须在往返后仍存在
        for b in d1.blocks.iter().filter(|b| !b.text.trim().is_empty()) {
            assert!(
                d2.blocks.iter().any(|x| x.text.trim() == b.text.trim()),
                "往返丢失: {:?}\n--- fountain ---\n{}",
                b.text,
                f
            );
        }
    }

    #[test]
    fn r02_fdx_roundtrip_preserves_types_and_text() {
        let d1 = doc_from(SAMPLE);
        let xml = to_fdx(&d1);
        let d2 = from_fdx(&xml, Some("t".into())).unwrap();

        let t1: Vec<_> = d1
            .blocks
            .iter()
            .filter(|b| !b.text.trim().is_empty())
            .map(|b| (b.block_type, b.text.clone()))
            .collect();
        let t2: Vec<_> = d2
            .blocks
            .iter()
            .filter(|b| !b.text.trim().is_empty())
            .map(|b| (b.block_type, b.text.clone()))
            .collect();

        assert_eq!(t1.len(), t2.len(), "块数量不一致");
        for (i, (a, b)) in t1.iter().zip(t2.iter()).enumerate() {
            assert_eq!(a.0, b.0, "块 {i} 类型不一致");
            assert_eq!(a.1.trim(), b.1.trim(), "块 {i} 文本不一致");
        }
    }

    #[test]
    fn r03_compact_source_roundtrip_stable() {
        let d1 = doc_from(SAMPLE);
        let s1 = to_compact_source(&d1);
        let d2 = doc_from(&s1);
        let s2 = to_compact_source(&d2);
        assert_eq!(s1, s2, "compact source 往返不稳定");
    }

    #[test]
    fn compact_source_merges_dialogue_inline() {
        let d = doc_from("剑客（内心独白）：我是一个爱做梦的人。");
        let s = to_compact_source(&d);
        assert_eq!(s, "剑客（内心独白）：我是一个爱做梦的人。");
    }

    #[test]
    fn compact_source_scene_numbering_preserved() {
        let d = doc_from("1. 外 酒肆 夜\n\n2. 内 城门 日");
        let s = to_compact_source(&d);
        assert!(s.contains("1. 外. 酒肆 夜"), "{s}");
        assert!(s.contains("2. 内. 城门 日"), "{s}");
    }

    #[test]
    fn fdx_invalid_xml_returns_error_not_panic() {
        let r = from_fdx("this is not xml at all", None);
        assert!(r.is_err());
    }

    #[test]
    fn html_escapes_dangerous_chars() {
        let mut d = ScreenplayDocument::blank("a<b");
        d.blocks.push(ScreenplayBlock::new(
            BlockType::Action,
            "<script>alert(1)</script>",
        ));
        let h = to_html(&d);
        assert!(!h.contains("<script>alert"));
        assert!(h.contains("&lt;script&gt;"));
    }

    /// 回归测试：to_compact_source 在遇到 Parenthetical/Note/Section/Synopsis 时
    /// 必须前进 i，否则会死循环（曾经因为表达式 arm 漏写 i += 1 导致 OOM）。
    #[test]
    fn r02_compact_source_no_infinite_loop_on_parenthetical_etc() {
        let mut d = ScreenplayDocument::blank("test");
        // 构造包含所有"危险"块类型的序列：parenthetical/note/section/synopsis
        d.blocks.push(ScreenplayBlock::new(BlockType::SceneHeading, "EXT. X - DAY"));
        d.blocks.push(ScreenplayBlock::new(BlockType::Character, "JOHN"));
        d.blocks.push(ScreenplayBlock::new(BlockType::Parenthetical, "whispering"));
        d.blocks.push(ScreenplayBlock::new(BlockType::Note, "改写对白"));
        d.blocks.push(ScreenplayBlock::new(BlockType::Section, "# chapter 1"));
        d.blocks.push(ScreenplayBlock::new(BlockType::Synopsis, "一句话概要"));
        d.blocks.push(ScreenplayBlock::new(BlockType::PageBreak, ""));
        d.blocks.push(ScreenplayBlock::new(BlockType::Transition, "CUT TO:"));
        // 加 timeout 风格的断言：函数应快速返回
        let start = std::time::Instant::now();
        let s = to_compact_source(&d);
        let elapsed = start.elapsed();
        assert!(elapsed.as_millis() < 1000, "to_compact_source 应在 1s 内完成，实际 {elapsed:?}");
        // 验证内容也生成了
        assert!(s.contains("（whispering）"), "应包含括注: {s}");
        assert!(s.contains("[[改写对白]]"), "应包含注释: {s}");
        assert!(s.contains("# chapter 1"), "应包含节标题: {s}");
        assert!(s.contains("= 一句话概要"), "应包含概要: {s}");
    }

    /// 回归测试：解析器对 OCR 输出（含孤立 parenthetical）必须能往返
    #[test]
    fn r03_compact_source_handles_isolated_parenthetical_after_dialogue() {
        // 真实 OCR 输出片段：人物 → 对白 → 括注（顺序被打乱）
        let text = "EXT. CITY STREET - DAY\nJOHN\nI can't believe we made it.\n(whispering)\nCUT TO:\nINT. DINER - NIGHT\nMARIA\nYou're late.\n";
        let d = doc_from(text);
        // 必须在时限内完成
        let start = std::time::Instant::now();
        let _ = to_compact_source(&d);
        let elapsed = start.elapsed();
        assert!(elapsed.as_millis() < 1000, "to_compact_source 应在 1s 内完成，实际 {elapsed:?}");
    }
}
