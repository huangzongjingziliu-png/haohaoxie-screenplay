//! DOCX 导入。
//!
//! DOCX 是 ZIP 容器，正文位于 `word/document.xml`。
//! 文本节点为 `<w:t>`，段落边界为 `<w:p>`。
//! 本模块提取所有段落文本，保留空行，交由核心解析器推断结构。

use quick_xml::events::Event;
use quick_xml::Reader;
use screenplay_core::parser::{self, ParseOptions, ParseOutcome};
use std::io::{Cursor, Read};
use zip::ZipArchive;

/// 从 DOCX 字节提取纯文本（按段落换行）
pub fn extract_docx_text(docx_bytes: &[u8]) -> Result<String, anyhow::Error> {
    let cursor = Cursor::new(docx_bytes);
    let mut archive = ZipArchive::new(cursor)?;
    let mut xml = String::new();
    match archive.by_name("word/document.xml") {
        Ok(mut f) => {
            anyhow::ensure!(f.size() <= 32 * 1024 * 1024, "DOCX 正文 XML 超过 32 MiB，已拒绝解压");
            f.read_to_string(&mut xml)?;
        }
        Err(e) => return Err(anyhow::anyhow!("DOCX 中未找到 word/document.xml: {e}")),
    }

    let mut reader = Reader::from_str(&xml);
    reader.config_mut().trim_text(false);

    let mut paragraphs: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_text = false;

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.name();
                let local = name.as_ref();
                if local == b"w:p" {
                    // 新段落开始
                    if !current.trim().is_empty() {
                        paragraphs.push(std::mem::take(&mut current));
                    } else {
                        current.clear();
                    }
                } else if local == b"w:t" {
                    in_text = true;
                } else if local == b"w:tab" {
                    current.push('\t');
                } else if local == b"w:br" {
                    current.push('\n');
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name();
                let local = name.as_ref();
                if local == b"w:t" {
                    in_text = false;
                } else if local == b"w:p" {
                    if !current.trim().is_empty() {
                        paragraphs.push(std::mem::take(&mut current));
                    } else {
                        current.clear();
                    }
                }
            }
            Ok(Event::Text(e)) => {
                if in_text {
                    if let Ok(t) = e.decode() {
                        current.push_str(&quick_xml::escape::unescape(&t)?);
                    }
                }
            }
            Ok(Event::GeneralRef(reference)) if in_text => {
                let entity = format!("&{};", reference.decode()?);
                current.push_str(&quick_xml::escape::unescape(&entity)?);
            }
            Ok(Event::CData(text)) if in_text => {
                current.push_str(&text.decode()?);
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(anyhow::anyhow!("DOCX XML 解析错误: {e}")),
            _ => {}
        }
        buf.clear();
    }
    if !current.trim().is_empty() {
        paragraphs.push(current);
    }

    Ok(paragraphs.join("\n\n"))
}

/// 从 DOCX 字节导入为剧本文档
pub fn import_docx(docx_bytes: &[u8], title: Option<String>) -> Result<ParseOutcome, anyhow::Error> {
    let text = extract_docx_text(docx_bytes)?;
    let opts = ParseOptions {
        title,
        denoise: true,
        known_characters: Vec::new(),
        layout_hints: Vec::new(),
    };
    Ok(parser::parse(&text, &opts))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

    fn make_docx(paragraphs: &[&str]) -> Vec<u8> {
        let mut body = String::new();
        for p in paragraphs {
            body.push_str("<w:p><w:r><w:t>");
            body.push_str(p);
            body.push_str("</w:t></w:r></w:p>");
        }
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>{body}</w:body>
</w:document>"#
        );
        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut buf));
            let opts = SimpleFileOptions::default()
                .compression_method(CompressionMethod::Deflated);
            zip.start_file("word/document.xml", opts).unwrap();
            zip.write_all(xml.as_bytes()).unwrap();
            zip.finish().unwrap();
        }
        buf
    }

    use std::io::Write;

    #[test]
    fn t01_extract_docx_text() {
        let bytes = make_docx(&["外 酒肆 夜", "剑客：你来了。"]);
        let text = extract_docx_text(&bytes).unwrap();
        assert!(text.contains("外 酒肆 夜"));
        assert!(text.contains("剑客：你来了。"));
    }

    #[test]
    fn xml_entities_are_imported_as_original_characters() {
        let bytes = make_docx(&["他说：A &amp; B &lt; C &gt; D &quot;对白&quot; &#x4E2D;"]);
        assert_eq!(extract_docx_text(&bytes).unwrap(), "他说：A & B < C > D \"对白\" 中");
    }

    #[test]
    fn t02_import_docx_produces_blocks() {
        let bytes = make_docx(&["外 酒肆 夜", "剑客：你来了。"]);
        let outcome = import_docx(&bytes, None).unwrap();
        assert!(outcome.document.blocks.len() > 0);
    }
}
