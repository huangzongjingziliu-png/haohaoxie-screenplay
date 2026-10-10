//! RTF 导入。
//!
//! RTF 是纯文本格式，控制字以 `\` 开头，分组用 `{}`。
//! 本模块做轻量解析：跳过控制字，提取纯文本，以 `\par` 作为段落分隔。

use screenplay_core::parser::{self, ParseOptions, ParseOutcome};

/// 从 RTF 文本提取纯文本
pub fn extract_rtf_text(rtf: &str) -> String {
    let mut out: Vec<u8> = Vec::new();
    let bytes = rtf.as_bytes();
    let mut i = 0usize;
    let mut brace_depth = 0i32;
    let mut skip_group = false;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'{' {
            brace_depth += 1;
            i += 1;
            continue;
        }
        if b == b'}' {
            brace_depth -= 1;
            i += 1;
            if brace_depth <= 0 {
                skip_group = false;
            }
            continue;
        }
        if b == b'\\' {
            i += 1;
            if i >= bytes.len() {
                break;
            }
            let c = bytes[i];
            match c {
                b'\\' => {
                    out.push(b'\\');
                    i += 1;
                }
                b'{' => {
                    out.push(b'{');
                    i += 1;
                }
                b'}' => {
                    out.push(b'}');
                    i += 1;
                }
                b'~' => {
                    out.push(b' ');
                    i += 1;
                }
                b'-' => {
                    i += 1;
                }
                b'_' => {
                    out.push(0xc2);
                    out.push(0xa0); // U+00A0 non-breaking space in UTF-8
                    i += 1;
                }
                b'*' => {
                    skip_group = true;
                    i += 1;
                }
                b'p' => {
                    i += 1;
                    if i < bytes.len() && bytes[i] == b'a' {
                        i += 1;
                        if i < bytes.len() && bytes[i] == b'r' {
                            out.push(b'\n');
                            i += 1;
                        }
                    }
                    while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'-') {
                        i += 1;
                    }
                }
                b'\'' => {
                    i += 1;
                    if i + 1 < bytes.len() {
                        let hi = (bytes[i] as char).to_digit(16).unwrap_or(0);
                        let lo = (bytes[i + 1] as char).to_digit(16).unwrap_or(0);
                        let cp = (hi << 4) | lo;
                        if let Some(ch) = char::from_u32(cp) {
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        i += 2;
                    }
                }
                _ if c.is_ascii_alphabetic() => {
                    while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'-') {
                        i += 1;
                    }
                    if i < bytes.len() && bytes[i] == b' ' {
                        i += 1;
                    }
                }
                _ => {
                    i += 1;
                }
            }
            continue;
        }
        if skip_group {
            i += 1;
            continue;
        }
        out.push(b);
        i += 1;
    }
    let raw = String::from_utf8_lossy(&out).to_string();
    // 清理多余空行
    let lines: Vec<&str> = raw.lines().collect();
    let mut cleaned = Vec::new();
    let mut prev_empty = false;
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !prev_empty {
                cleaned.push("");
            }
            prev_empty = true;
        } else {
            cleaned.push(trimmed);
            prev_empty = false;
        }
    }
    cleaned.join("\n")
}

/// 从 RTF 字节导入为剧本文档
pub fn import_rtf(rtf_bytes: &[u8], title: Option<String>) -> Result<ParseOutcome, anyhow::Error> {
    let rtf = String::from_utf8_lossy(rtf_bytes).to_string();
    let text = extract_rtf_text(&rtf);
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

    #[test]
    fn t01_extract_simple_rtf() {
        let rtf = r#"{\rtf1\ansi\deff0
{\fonttbl{\f0 Times New Roman;}}
\f0\fs24 外 酒肆 夜\par
剑客：你来了。\par
}"#;
        let text = extract_rtf_text(rtf);
        assert!(text.contains("外 酒肆 夜"));
        assert!(text.contains("剑客：你来了。"));
    }

    #[test]
    fn t02_rtf_paragraph_breaks() {
        let rtf = r#"{\rtf1 hello\par world\par}"#;
        let text = extract_rtf_text(rtf);
        assert!(text.contains("hello"));
        assert!(text.contains("world"));
    }

    #[test]
    fn t03_rtf_hex_escape() {
        let rtf = r#"{\rtf1 caf\'e9}"#;
        let text = extract_rtf_text(rtf);
        assert!(text.contains("caf"));
        assert!(text.contains('é'));
    }
}
