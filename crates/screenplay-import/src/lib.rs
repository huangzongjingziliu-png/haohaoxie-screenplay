//! 剧本导入管道：txt / md / docx / doc / rtf / fountain / fdx / pdf / OCR
//!
//! 每种格式负责将原始字节或文本转换为 `screenplay_core` 的中间文本，
//! 再交由核心解析器生成 AST。布局敏感格式（PDF）会先提取坐标，
//! 生成 `layout_hints` 辅助解析。

pub mod control;
pub use control::ImportControl;
pub mod pdf;
pub mod docx;
pub mod rtf;
pub mod ocr;

use screenplay_core::parser::{self, ParseOptions, ParseOutcome};
use screenplay_core::serializer;
use std::path::Path;

/// 从纯文本导入
pub fn import_text(text: &str, title: Option<String>) -> ParseOutcome {
    let opts = ParseOptions {
        title,
        denoise: true,
        known_characters: Vec::new(),
        layout_hints: Vec::new(),
    };
    parser::parse(text, &opts)
}

/// 从 Fountain 导入（直接复用 core 的解析器）
pub fn import_fountain(text: &str, title: Option<String>) -> ParseOutcome {
    import_text(text, title)
}

/// 从 Final Draft (.fdx) XML 导入
pub fn import_fdx(xml: &str, title: Option<String>) -> Result<ParseOutcome, String> {
    let doc = serializer::from_fdx(xml, title)?;
    Ok(ParseOutcome {
        low_confidence: 0,
        removed_noise: Vec::new(),
        document: doc,
    })
}

/// 统一导入入口：根据格式分发到对应解析器
pub fn import(
    bytes: &[u8],
    format: ImportFormat,
    title: Option<String>,
) -> Result<ParseOutcome, anyhow::Error> {
    import_controlled(bytes, format, title, &ImportControl::default())
}

pub fn import_controlled(bytes: &[u8], format: ImportFormat, title: Option<String>, control: &ImportControl) -> anyhow::Result<ParseOutcome> {
    control.check()?;
    control.report("parsing", 0, 0);
    let outcome = match format {

        ImportFormat::Txt | ImportFormat::Markdown | ImportFormat::Fountain => {
            let text = String::from_utf8_lossy(bytes).to_string();
            Ok(import_text(&text, title))
        }
        ImportFormat::Fdx => {
            let xml = String::from_utf8_lossy(bytes).to_string();
            import_fdx(&xml, title).map_err(|e| anyhow::anyhow!(e))
        }
        ImportFormat::Docx => docx::import_docx(bytes, title),
        ImportFormat::Rtf => rtf::import_rtf(bytes, title),
        ImportFormat::Pdf => {
            // 文本可提取的 PDF 走解析器；扫描型 PDF（无文本层）回退到 OCR
            match pdf::import_pdf_controlled(bytes, title.clone(), control) {
                Ok(outcome) if has_meaningful_text(&outcome) => Ok(outcome),
                Ok(_) | Err(_) => {
                    // 文本层为空或提取失败：写到临时文件后调 sidecar OCR
                    { control.check()?; ocr_fallback_for_pdf(bytes, title, control) }
                }
            }
        }
        ImportFormat::Image => {
            // 直接 OCR 单张图片
            let tmp = tempfile::Builder::new()
                .suffix(".img")
                .tempfile()
                .map_err(|e| anyhow::anyhow!("创建临时文件失败: {e}"))?;
            std::fs::write(tmp.path(), bytes)
                .map_err(|e| anyhow::anyhow!("写入临时文件失败: {e}"))?;
            ocr::import_ocr_controlled(tmp.path(), title, None, control)
                .map_err(|e| anyhow::anyhow!(e.to_string()))
        }
        ImportFormat::Doc => Err(anyhow::anyhow!(
            "旧版 .doc 二进制格式暂不支持，请转换为 .docx 后导入"
        )),
    }?;
    control.check()?;
    control.report("ready", 0, 0);
    Ok(outcome)
}

/// 判断 ParseOutcome 是否包含有意义的文本（非纯噪声）
fn has_meaningful_text(outcome: &ParseOutcome) -> bool {
    outcome
        .document
        .blocks
        .iter()
        .any(|b| b.text.chars().filter(|c| c.is_alphabetic()).count() >= 2)
}

/// 扫描型 PDF 回退：将原始 PDF 字节写到临时文件，调用 OCR sidecar
fn ocr_fallback_for_pdf(pdf_bytes: &[u8], title: Option<String>, control: &ImportControl) -> Result<ParseOutcome, anyhow::Error> {
    let tmp = tempfile::Builder::new()
        .suffix(".pdf")
        .tempfile()
        .map_err(|e| anyhow::anyhow!("创建临时文件失败: {e}"))?;
    std::fs::write(tmp.path(), pdf_bytes)
        .map_err(|e| anyhow::anyhow!("写入临时文件失败: {e}"))?;
    ocr::import_ocr_controlled(tmp.path(), title, None, control)
        .map_err(|e| anyhow::anyhow!(e.to_string()))
}

/// 从文件路径（带扩展名）识别格式并直接导入
pub fn import_path(
    path: &Path,
    title: Option<String>,
) -> Result<ParseOutcome, anyhow::Error> {
    Ok(import_path_with_source_controlled(path, title, &ImportControl::default())?.0)
}

/// Maximum size of a source attachment retained with a project.
pub const MAX_SOURCE_BYTES: u64 = 512 * 1024 * 1024;

/// Read and parse once, retaining the exact input bytes for project archiving.
pub fn import_path_with_source_controlled(
    path: &Path,
    title: Option<String>,
    control: &ImportControl,
) -> anyhow::Result<(ParseOutcome, Vec<u8>)> {
    use std::io::Read;
    control.check()?;
    control.report("reading", 0, 0);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let format = ImportFormat::from_extension(&ext)
        .ok_or_else(|| anyhow::anyhow!("不支持的文件格式: .{ext}"))?;
    let mut file = std::fs::File::open(path)?;
    anyhow::ensure!(file.metadata()?.len() <= MAX_SOURCE_BYTES, "原始文件超过 512 MiB，暂不支持导入归档");
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        control.check()?;
        let count = file.read(&mut chunk)?;
        if count == 0 { break; }
        anyhow::ensure!(bytes.len() as u64 + count as u64 <= MAX_SOURCE_BYTES, "原始文件超过 512 MiB，暂不支持导入归档");
        bytes.extend_from_slice(&chunk[..count]);
    }
    let mut outcome = import_controlled(&bytes, format, title, control)?;
    anyhow::ensure!(has_meaningful_text(&outcome), "未识别到可编辑文字，当前剧本未改变。请检查文件或扫描清晰度");
    if let Some(info) = outcome.document.import_info.as_mut() {
        use screenplay_core::ast::SourceType;
        info.source_name = path.file_name().map(|name| name.to_string_lossy().into_owned());
        info.source_type = match format {
            ImportFormat::Pdf => SourceType::Pdf,
            ImportFormat::Docx => SourceType::Docx,
            ImportFormat::Doc => SourceType::Doc,
            ImportFormat::Rtf => SourceType::Rtf,
            ImportFormat::Fdx => SourceType::Fdx,
            ImportFormat::Fountain => SourceType::Fountain,
            ImportFormat::Txt => SourceType::Txt,
            ImportFormat::Markdown => SourceType::Markdown,
            ImportFormat::Image => SourceType::Image,
        };
    }
    Ok((outcome, bytes))
}

/// 文件格式枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportFormat {
    Txt,
    Markdown,
    Fountain,
    Fdx,
    Docx,
    Doc,
    Rtf,
    Pdf,
    Image,
}

impl ImportFormat {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "txt" => Some(Self::Txt),
            "md" | "markdown" => Some(Self::Markdown),
            "fountain" | "spmd" => Some(Self::Fountain),
            "fdx" => Some(Self::Fdx),
            "docx" => Some(Self::Docx),
            "doc" => Some(Self::Doc),
            "rtf" => Some(Self::Rtf),
            "pdf" => Some(Self::Pdf),
            "png" | "jpg" | "jpeg" | "heic" | "tiff" | "tif" | "bmp" | "gif" => Some(Self::Image),
            _ => None,
        }
    }
}

#[cfg(test)]
mod source_file_tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn source_bytes_and_filename_are_retained() {
        let mut file = tempfile::Builder::new().suffix(".txt").tempfile().unwrap();
        let bytes = "内景 客厅 日\r\n\r\n人物走进房间。\r\n".as_bytes();
        file.write_all(bytes).unwrap();
        let (outcome, source) = import_path_with_source_controlled(file.path(), None, &ImportControl::default()).unwrap();
        assert_eq!(source, bytes);
        assert_eq!(outcome.document.import_info.unwrap().source_name.as_deref(), file.path().file_name().unwrap().to_str());
    }

    #[test]
    fn empty_source_is_rejected_before_document_replacement() {
        let file = tempfile::Builder::new().suffix(".txt").tempfile().unwrap();
        assert!(import_path(file.path(), None).unwrap_err().to_string().contains("未识别到"));
    }

    #[test]
    fn oversized_source_is_rejected_without_reading_it() {
        let file = tempfile::Builder::new().suffix(".txt").tempfile().unwrap();
        file.as_file().set_len(MAX_SOURCE_BYTES + 1).unwrap();
        assert!(import_path(file.path(), None).unwrap_err().to_string().contains("512 MiB"));
    }
}
