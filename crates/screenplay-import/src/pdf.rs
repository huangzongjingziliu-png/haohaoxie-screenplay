//! PDF 文本提取与布局恢复。
//!
//! 使用 `lopdf` 解析页面内容流，追踪文本矩阵以获取每个文本片段的 (x, y) 坐标，
//! 按 Y 坐标归并为行，再按 X 坐标（缩进）推断剧本块类型，最终交给
//! `screenplay-core` 的解析器生成 AST。

use lopdf::{Document, Object, ObjectId};
use screenplay_core::ast::BlockType;
use screenplay_core::parser::{self, ParseOptions, ParseOutcome};
use std::collections::HashMap;

/// 带坐标的文本片段
#[derive(Debug, Clone)]
struct TextSpan {
    x: f64,
    y: f64, // 已翻转为自上而下
    text: String,
}

/// 归并后的文本行
#[derive(Debug, Clone)]
pub struct TextLine {
    pub y: f64,
    pub x_min: f64,
    pub x_max: f64,
    pub text: String,
}

/// 从 PDF 字节中提取所有页面的文本行（带坐标）
pub fn extract_lines(pdf_bytes: &[u8]) -> Result<Vec<TextLine>, anyhow::Error> {
    extract_lines_controlled(pdf_bytes, &crate::ImportControl::default())
}

fn extract_lines_controlled(pdf_bytes: &[u8], control: &crate::ImportControl) -> anyhow::Result<Vec<TextLine>> {
    control.check()?;
    let doc = Document::load_mem(pdf_bytes)?;
    let mut all_lines: Vec<TextLine> = Vec::new();

    let total = doc.get_pages().len();
    for (index, page_id) in doc.page_iter().enumerate() {
        control.check()?;
        control.report("pdf_pages", index, total);
        let page_h = page_media_box_height(&doc, page_id);
        let content = match doc.get_page_content(page_id) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let spans = parse_content_stream(&content, page_h);
        let lines = group_into_lines(spans);
        all_lines.extend(lines);
    }

    Ok(all_lines)
}

/// 获取页面高度（来自 MediaBox）
fn page_media_box_height(doc: &Document, page_id: ObjectId) -> f64 {
    if let Ok(page) = doc.get_dictionary(page_id) {
        if let Ok(mb) = page.get(b"MediaBox") {
            if let Ok(arr) = mb.as_array() {
                if arr.len() >= 4 {
                    let y0 = arr[1].as_float().unwrap_or(0.0) as f64;
                    let y1 = arr[3].as_float().unwrap_or(792.0) as f64;
                    return y1 - y0;
                }
            }
        }
    }
    792.0 // US Letter 默认
}

/// 解析页面内容流，返回带坐标的文本片段。
///
/// 维护当前文本矩阵 Tm [a b c d e f]，其中 e/f 为平移。
/// 遇到 Tj/TJ/'/" 等显示文本算子时，用 e/f 作为当前文本位置。
fn parse_content_stream(content: &[u8], page_height: f64) -> Vec<TextSpan> {
    let mut spans: Vec<TextSpan> = Vec::new();
    let mut tm: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut tl: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut operands: Vec<Object> = Vec::new();

    let mut i = 0usize;
    let bytes = content;
    while i < bytes.len() {
        skip_whitespace(bytes, &mut i);
        if i >= bytes.len() {
            break;
        }
        let token = match read_token(bytes, &mut i) {
            Some(t) => t,
            None => break,
        };

        match token.as_str() {
            "Tm" => {
                if operands.len() >= 6 {
                    tm = [
                        obj_f64(&operands[0]),
                        obj_f64(&operands[1]),
                        obj_f64(&operands[2]),
                        obj_f64(&operands[3]),
                        obj_f64(&operands[4]),
                        obj_f64(&operands[5]),
                    ];
                    tl = tm;
                }
                operands.clear();
            }
            "Td" | "TD" => {
                if operands.len() >= 2 {
                    let tx = obj_f64(&operands[0]);
                    let ty = obj_f64(&operands[1]);
                    tl[4] = tm[4] + tx;
                    tl[5] = tm[5] + ty;
                    tm = tl;
                }
                operands.clear();
            }
            "T*" => {
                tl[5] = tm[5] - 12.0;
                tl[4] = tm[4];
                tm = tl;
                operands.clear();
            }
            "Tj" | "'" | "\"" => {
                if token == "\"" && operands.len() >= 3 {
                    operands.pop(); // aw
                    operands.pop(); // ac
                    if let Some(s) = operands.pop() {
                        record_span(&mut spans, &s, tm[4], tm[5], page_height);
                    }
                } else if token == "'" {
                    tl[5] = tm[5] - 12.0;
                    tl[4] = tm[4];
                    tm = tl;
                    if let Some(s) = operands.pop() {
                        record_span(&mut spans, &s, tm[4], tm[5], page_height);
                    }
                } else if let Some(s) = operands.pop() {
                    record_span(&mut spans, &s, tm[4], tm[5], page_height);
                }
                operands.clear();
            }
            "TJ" => {
                if let Some(arr_obj) = operands.pop() {
                    if let Ok(arr) = arr_obj.as_array() {
                        for item in arr {
                            match item {
                                Object::String(b, _) => {
                                    let t = pdf_string_to_text(b);
                                    if !t.is_empty() {
                                        spans.push(TextSpan {
                                            x: tm[4],
                                            y: page_height - tm[5],
                                            text: t,
                                        });
                                    }
                                }
                                Object::Real(f) => {
                                    tm[4] += (*f as f64) * 0.001 * 12.0;
                                }
                                Object::Integer(n) => {
                                    tm[4] += (*n as f64) * 0.001 * 12.0;
                                }
                                _ => {}
                            }
                        }
                    }
                }
                operands.clear();
            }
            "q" | "Q" | "BT" | "ET" => {
                operands.clear();
            }
            _ => {
                if let Ok(obj) = parse_operand(&token) {
                    operands.push(obj);
                } else {
                    operands.clear();
                }
            }
        }
    }
    spans
}

fn record_span(spans: &mut Vec<TextSpan>, obj: &Object, x: f64, y_pdf: f64, page_height: f64) {
    let t = match obj {
        Object::String(b, _) => pdf_string_to_text(b),
        _ => return,
    };
    if t.is_empty() {
        return;
    }
    spans.push(TextSpan {
        x,
        y: page_height - y_pdf,
        text: t,
    });
}

/// PDF 字符串 → Rust String（处理 UTF-16BE/LE BOM 与 PDFDocEncoding 回退）
fn pdf_string_to_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFE, 0xFF]) && bytes.len() >= 4 {
        let u16s: Vec<u16> = bytes[2..]
            .chunks(2)
            .map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)]))
            .collect();
        String::from_utf16_lossy(&u16s)
    } else if bytes.starts_with(&[0xFF, 0xFE]) && bytes.len() >= 4 {
        let u16s: Vec<u16> = bytes[2..]
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)]))
            .collect();
        String::from_utf16_lossy(&u16s)
    } else {
        match std::str::from_utf8(bytes) {
            Ok(s) => s.to_string(),
            Err(_) => bytes.iter().map(|&b| b as char).collect(),
        }
    }
}

fn obj_f64(o: &Object) -> f64 {
    match o {
        Object::Real(f) => *f as f64,
        Object::Integer(i) => *i as f64,
        _ => 0.0,
    }
}

fn skip_whitespace(bytes: &[u8], i: &mut usize) {
    while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
        *i += 1;
    }
}

/// 读取一个 token（算子或操作数）
fn read_token(bytes: &[u8], i: &mut usize) -> Option<String> {
    if *i >= bytes.len() {
        return None;
    }
    let b = bytes[*i];
    if b == b'(' {
        return read_literal_string(bytes, i);
    }
    if b == b'<' {
        if *i + 1 < bytes.len() && bytes[*i + 1] == b'<' {
            *i += 2;
            return Some("<<".to_string());
        }
        return read_hex_string(bytes, i);
    }
    if b == b'[' {
        *i += 1;
        return Some("[".to_string());
    }
    if b == b']' {
        *i += 1;
        return Some("]".to_string());
    }
    if b == b'/' {
        let mut s = String::from("/");
        *i += 1;
        while *i < bytes.len() && !bytes[*i].is_ascii_whitespace() {
            let c = bytes[*i];
            if matches!(c, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'/') {
                break;
            }
            s.push(c as char);
            *i += 1;
        }
        return Some(s);
    }
    // 普通 token
    let mut s = String::new();
    while *i < bytes.len() {
        let c = bytes[*i];
        if c.is_ascii_whitespace() {
            break;
        }
        if matches!(c, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'/') {
            break;
        }
        s.push(c as char);
        *i += 1;
    }
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn read_literal_string(bytes: &[u8], i: &mut usize) -> Option<String> {
    *i += 1; // skip (
    let mut buf = Vec::new();
    let mut depth = 1i32;
    while *i < bytes.len() && depth > 0 {
        let b = bytes[*i];
        if b == b'\\' && *i + 1 < bytes.len() {
            let next = bytes[*i + 1];
            match next {
                b'n' => buf.push(b'\n'),
                b'r' => buf.push(b'\r'),
                b't' => buf.push(b'\t'),
                b'b' => buf.push(0x08),
                b'f' => buf.push(0x0c),
                b'(' => buf.push(b'('),
                b')' => buf.push(b')'),
                b'\\' => buf.push(b'\\'),
                _ => buf.push(next),
            }
            *i += 2;
        } else if b == b'(' {
            depth += 1;
            buf.push(b);
            *i += 1;
        } else if b == b')' {
            depth -= 1;
            if depth > 0 {
                buf.push(b);
            }
            *i += 1;
        } else {
            buf.push(b);
            *i += 1;
        }
    }
    Some(format!("({})", pdf_string_to_text(&buf)))
}

fn read_hex_string(bytes: &[u8], i: &mut usize) -> Option<String> {
    *i += 1; // skip <
    let mut hex = String::new();
    while *i < bytes.len() {
        let b = bytes[*i];
        if b == b'>' {
            *i += 1;
            break;
        }
        if !b.is_ascii_whitespace() {
            hex.push(b as char);
        }
        *i += 1;
    }
    let mut buf = Vec::new();
    let chars: Vec<char> = hex.chars().collect();
    let mut j = 0;
    while j + 1 < chars.len() {
        let hi = chars[j].to_digit(16).unwrap_or(0);
        let lo = chars[j + 1].to_digit(16).unwrap_or(0);
        buf.push(((hi << 4) | lo) as u8);
        j += 2;
    }
    Some(format!("({})", pdf_string_to_text(&buf)))
}

/// 将 token 解析为 Object
fn parse_operand(token: &str) -> Result<Object, ()> {
    if let Some(inner) = token.strip_prefix('(').and_then(|t| t.strip_suffix(')')) {
        return Ok(Object::string_literal(inner));
    }
    if let Ok(n) = token.parse::<i64>() {
        return Ok(Object::Integer(n));
    }
    if let Ok(f) = token.parse::<f64>() {
        return Ok(Object::Real(f as f32));
    }
    if let Some(name) = token.strip_prefix('/') {
        return Ok(Object::Name(name.as_bytes().to_vec()));
    }
    Err(())
}

/// 按 Y 坐标将片段归并为行（容差 ±3 点），同行内按 X 排序拼接
fn group_into_lines(spans: Vec<TextSpan>) -> Vec<TextLine> {
    let mut buckets: Vec<Vec<TextSpan>> = Vec::new();
    for span in spans {
        let mut placed = false;
        for bucket in buckets.iter_mut() {
            let by = bucket[0].y;
            if (span.y - by).abs() < 3.0 {
                bucket.push(span.clone());
                placed = true;
                break;
            }
        }
        if !placed {
            buckets.push(vec![span]);
        }
    }

    let mut lines: Vec<TextLine> = Vec::new();
    for bucket in &mut buckets {
        bucket.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
        let text: String = bucket.iter().map(|s| s.text.as_str()).collect();
        let y = bucket[0].y;
        let x_min = bucket.iter().map(|s| s.x).fold(f64::INFINITY, f64::min);
        let x_max = bucket
            .iter()
            .map(|s| s.x + s.text.len() as f64 * 6.0)
            .fold(f64::NEG_INFINITY, f64::max);
        lines.push(TextLine {
            y,
            x_min,
            x_max,
            text: text.trim().to_string(),
        });
    }
    lines.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
    lines
}

/// 根据行的 X 坐标缩进推断块类型，生成布局提示
pub fn infer_layout_hints(lines: &[TextLine], _page_width: f64) -> Vec<(String, BlockType)> {
    let mut hints: Vec<(String, BlockType)> = Vec::new();

    let mut x_freq: HashMap<i32, usize> = HashMap::new();
    for l in lines {
        if l.text.trim().is_empty() {
            continue;
        }
        let bucket = (l.x_min / 10.0).round() as i32;
        *x_freq.entry(bucket).or_insert(0) += 1;
    }
    let mut sorted_x: Vec<(i32, usize)> = x_freq.into_iter().collect();
    sorted_x.sort_by(|a, b| b.1.cmp(&a.1));
    let action_x = sorted_x.first().map(|(x, _)| *x as f64 * 10.0).unwrap_or(0.0);
    let dialogue_x = sorted_x
        .iter()
        .skip(1)
        .find(|(x, _)| (*x as f64 * 10.0 - action_x).abs() > 30.0)
        .map(|(x, _)| *x as f64 * 10.0);

    for l in lines {
        let t = l.text.trim();
        if t.is_empty() {
            continue;
        }
        let dx = (l.x_min - action_x).abs();
        if dx < 20.0 {
            if looks_like_scene_heading(t) {
                hints.push((t.to_string(), BlockType::SceneHeading));
            }
        } else if let Some(dx0) = dialogue_x {
            if (l.x_min - dx0).abs() < 25.0 {
                if t.chars()
                    .all(|c| c.is_uppercase() || c.is_ascii_whitespace() || c.is_ascii_punctuation())
                    && t.len() < 30
                {
                    hints.push((t.to_string(), BlockType::Character));
                }
            }
        }
    }
    hints
}

fn looks_like_scene_heading(t: &str) -> bool {
    let upper = t.to_ascii_uppercase();
    upper.starts_with("INT.")
        || upper.starts_with("EXT.")
        || upper.starts_with("INT/EXT")
        || upper.starts_with("I/E")
        || t.starts_with("内.")
        || t.starts_with("外.")
        || t.starts_with("内/")
        || t.starts_with("外/")
}

/// 将提取到的行拼接为纯文本（用空行分隔段落）
pub fn lines_to_text(lines: &[TextLine]) -> String {
    let mut out = String::new();
    let mut prev_y = f64::NEG_INFINITY;
    for l in lines {
        if l.text.trim().is_empty() {
            continue;
        }
        if prev_y != f64::NEG_INFINITY && l.y - prev_y > 20.0 {
            out.push('\n');
        }
        out.push_str(&l.text);
        out.push('\n');
        prev_y = l.y;
    }
    out
}

/// 主入口：从 PDF 字节导入为剧本文档
pub fn import_pdf(pdf_bytes: &[u8], title: Option<String>) -> Result<ParseOutcome, anyhow::Error> {
    import_pdf_controlled(pdf_bytes, title, &crate::ImportControl::default())
}

pub fn import_pdf_controlled(pdf_bytes: &[u8], title: Option<String>, control: &crate::ImportControl) -> anyhow::Result<ParseOutcome> {
    let lines = extract_lines_controlled(pdf_bytes, control)?;
    control.check()?;
    let text = lines_to_text(&lines);
    let hints = infer_layout_hints(&lines, 612.0);
    let opts = ParseOptions {
        title,
        denoise: true,
        known_characters: Vec::new(),
        layout_hints: hints,
    };
    Ok(parser::parse(&text, &opts))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一个极简的单页 PDF：包含场景标题、人物、对白
    fn make_sample_pdf() -> Vec<u8> {
        use lopdf::{Dictionary, Object, Stream};
        let content = b"BT\n/F1 12 Tf\n72 720 Td\n(EXT. BAR - NIGHT) Tj\n0 -20 Td\n(JOHN) Tj\n0 -16 Td\n(Hello, world.) Tj\nET\n";
        let mut doc = Document::with_version("1.4");
        let catalog_id = doc.new_object_id();
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        let font_id = doc.new_object_id();
        let content_id = doc.new_object_id();

        let font_dict = Dictionary::from_iter(vec![
            (b"Type".to_vec(), Object::Name(b"Font".to_vec())),
            (b"Subtype".to_vec(), Object::Name(b"Type1".to_vec())),
            (b"BaseFont".to_vec(), Object::Name(b"Helvetica".to_vec())),
        ]);
        doc.objects.insert(font_id, Object::Dictionary(font_dict));

        let content_stream = Stream::new(Dictionary::new(), content.to_vec());
        doc.objects.insert(content_id, Object::Stream(content_stream));

        let page_dict = Dictionary::from_iter(vec![
            (b"Type".to_vec(), Object::Name(b"Page".to_vec())),
            (
                b"MediaBox".to_vec(),
                Object::Array(vec![
                    Object::Integer(0),
                    Object::Integer(0),
                    Object::Integer(612),
                    Object::Integer(792),
                ]),
            ),
            (b"Parent".to_vec(), Object::Reference(pages_id)),
            (b"Contents".to_vec(), Object::Reference(content_id)),
        ]);
        doc.objects.insert(page_id, Object::Dictionary(page_dict));

        let pages_dict = Dictionary::from_iter(vec![
            (b"Type".to_vec(), Object::Name(b"Pages".to_vec())),
            (b"Kids".to_vec(), Object::Array(vec![Object::Reference(page_id)])),
            (b"Count".to_vec(), Object::Integer(1)),
        ]);
        doc.objects.insert(pages_id, Object::Dictionary(pages_dict));

        let catalog_dict = Dictionary::from_iter(vec![
            (b"Type".to_vec(), Object::Name(b"Catalog".to_vec())),
            (b"Pages".to_vec(), Object::Reference(pages_id)),
        ]);
        doc.objects.insert(catalog_id, Object::Dictionary(catalog_dict));

        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        buf
    }

    #[test]
    fn controlled_pdf_reports_pages_and_can_cancel_without_ocr_fallback() {
        use std::sync::{Arc, Mutex};
        let events = Arc::new(Mutex::new(Vec::new()));
        let recorded = events.clone();
        let control = crate::ImportControl::with_progress(move |stage, done, total| {
            recorded.lock().unwrap().push((stage.to_string(), done, total));
        });
        let bytes = make_sample_pdf();
        let controlled = crate::import_controlled(&bytes, crate::ImportFormat::Pdf, None, &control).unwrap();
        let ordinary = crate::import(&bytes, crate::ImportFormat::Pdf, None).unwrap();
        assert_eq!(controlled.document.blocks.iter().map(|b| (&b.block_type, &b.text)).collect::<Vec<_>>(), ordinary.document.blocks.iter().map(|b| (&b.block_type, &b.text)).collect::<Vec<_>>());
        assert!(events.lock().unwrap().iter().any(|e| e.0 == "pdf_pages" && e.2 == 1));
        control.cancel();
        assert!(crate::import_controlled(&bytes, crate::ImportFormat::Pdf, None, &control).unwrap_err().to_string().contains("取消"));
    }

    #[test]
    fn t01_extract_lines_from_minimal_pdf() {
        let bytes = make_sample_pdf();
        let lines = extract_lines(&bytes).unwrap();
        assert!(!lines.is_empty(), "应提取到至少一行");
        let joined: String = lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("|");
        assert!(joined.contains("EXT. BAR - NIGHT"), "场景标题应被提取: {joined}");
        assert!(joined.contains("JOHN"), "人物名应被提取: {joined}");
        assert!(joined.contains("Hello, world."), "对白应被提取: {joined}");
    }

    #[test]
    fn t02_import_pdf_produces_document() {
        let bytes = make_sample_pdf();
        let outcome = import_pdf(&bytes, Some("test".to_string())).unwrap();
        assert!(outcome.document.blocks.len() > 0, "应生成至少一个块");
    }

    #[test]
    fn t03_lines_to_text_preserves_order() {
        let lines = vec![
            TextLine { y: 10.0, x_min: 0.0, x_max: 100.0, text: "第一行".to_string() },
            TextLine { y: 30.0, x_min: 0.0, x_max: 100.0, text: "第二行".to_string() },
        ];
        let t = lines_to_text(&lines);
        assert!(t.contains("第一行"));
        assert!(t.contains("第二行"));
    }

    #[test]
    fn t04_group_into_lines_handles_y_tolerance() {
        let spans = vec![
            TextSpan { x: 0.0, y: 100.0, text: "A".to_string() },
            TextSpan { x: 50.0, y: 101.0, text: "B".to_string() },
            TextSpan { x: 0.0, y: 200.0, text: "C".to_string() },
        ];
        let lines = group_into_lines(spans);
        assert_eq!(lines.len(), 2, "应归并为 2 行");
        assert!(lines[0].text.contains("A"));
        assert!(lines[0].text.contains("B"));
    }
}
