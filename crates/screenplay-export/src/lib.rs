//! 剧本导出管道：PDF / DOCX / FDX / Fountain / TXT / Markdown / HTML
//!
//! 文本类格式（Fountain/FDX/TXT/Markdown/HTML）直接复用
//! `screenplay_core` 的序列化器；二进制格式（PDF/DOCX）在本 crate 内生成。
//!
//! # 统一分页（说明书 D-08）
//!
//! 所有出口共享 `screenplay_core::pagination::paginate` 的结果：
//! - PDF：直接把 pagination 的行坐标交给 CoreText 渲染
//! - GUI 纸面：同一份 Pagination DTO
//! - 页数统计：`DocumentStats` 用同一份
//!
//! 不存在「纸面 10 页 / PDF 12 页」的可能。

pub mod docx;
pub mod pdf;

use screenplay_core::ast::ScreenplayDocument;
use screenplay_core::serializer;

/// 导出为 Fountain
pub fn export_fountain(doc: &ScreenplayDocument) -> String {
    serializer::to_fountain(doc)
}

/// 导出为 Final Draft (.fdx)
pub fn export_fdx(doc: &ScreenplayDocument) -> String {
    serializer::to_fdx(doc)
}

/// 导出为纯文本
pub fn export_plain_text(doc: &ScreenplayDocument) -> String {
    serializer::to_plain_text(doc)
}

/// 导出为 Markdown
///
/// Markdown 没有官方的剧本方言，这里采用**语义化 + 可读**的方案：
/// - 场景标题 → `# 场景标题`（带编号）
/// - 动作 → 普通段落
/// - 人物 → `**人物名**`
/// - 括注 → `*（括注）*`
/// - 对白 → 缩进引用块 `> `
/// - 转场 → `**转场**` 右对齐语义（用 → 前缀）
/// - 元信息 → YAML front matter（便于工具读取）
///
/// 生成的 Markdown 可以被本项目的 Markdown 导入器读回（round-trip）。
pub fn export_markdown(doc: &ScreenplayDocument) -> String {
    use screenplay_core::ast::BlockType as BT;
    let mut out = String::new();

    // front matter
    out.push_str("---\n");
    out.push_str(&format!("title: {}\n", doc.metadata.title));
    if !doc.metadata.author.is_empty() {
        out.push_str(&format!("author: {}\n", doc.metadata.author));
    }
    out.push_str(&format!("language: {}\n", doc.metadata.language));
    out.push_str(&format!("revision: {}\n", doc.revision));
    out.push_str(&format!("block_count: {}\n", doc.blocks.len()));
    out.push_str("---\n\n");

    let mut scene_no = 0usize;
    for b in &doc.blocks {
        let text = b.text.trim();
        if text.is_empty() {
            continue;
        }
        match b.block_type {
            BT::SceneHeading => {
                scene_no += 1;
                out.push_str(&format!("## {}. {}\n\n", scene_no, text));
            }
            BT::Action | BT::General | BT::Note => {
                out.push_str(text);
                out.push_str("\n\n");
            }
            BT::Character => {
                out.push_str(&format!("**{}**\n\n", text));
            }
            BT::Parenthetical => {
                out.push_str(&format!("*{}*\n\n", text));
            }
            BT::Dialogue | BT::DualDialogue => {
                // 多行对白各自引用，避免 Markdown 合并成一段
                for line in text.lines() {
                    out.push_str("> ");
                    out.push_str(line.trim());
                    out.push('\n');
                }
                out.push('\n');
            }
            BT::Transition => {
                out.push_str(&format!("→ {}\n\n", text));
            }
            BT::Shot => {
                out.push_str(&format!("*[镜头] {}*\n\n", text));
            }
            BT::Section => {
                out.push_str(&format!("# {}\n\n", text));
            }
            BT::Synopsis => {
                out.push_str(&format!("> **概要**：{}\n\n", text));
            }
            _ => {
                out.push_str(text);
                out.push_str("\n\n");
            }
        }
    }

    // 折叠多余空行
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    out
}

/// 导出为 HTML
pub fn export_html(doc: &ScreenplayDocument) -> String {
    serializer::to_html(doc)
}

/// 导出格式枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Pdf,
    Docx,
    Fountain,
    Fdx,
    Txt,
    Md,
    Html,
}

impl ExportFormat {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "pdf" => Some(Self::Pdf),
            "docx" => Some(Self::Docx),
            "fountain" | "spmd" => Some(Self::Fountain),
            "fdx" => Some(Self::Fdx),
            "txt" => Some(Self::Txt),
            "md" | "markdown" => Some(Self::Md),
            "html" | "htm" => Some(Self::Html),
            _ => None,
        }
    }

    /// 稳定的字符串标识（CLI / MCP / 桥接协议输出用）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Docx => "docx",
            Self::Fountain => "fountain",
            Self::Fdx => "fdx",
            Self::Txt => "txt",
            Self::Md => "md",
            Self::Html => "html",
        }
    }
}

/// 统一导出入口
pub fn export(
    doc: &ScreenplayDocument,
    format: ExportFormat,
) -> Result<Vec<u8>, anyhow::Error> {
    match format {
        ExportFormat::Pdf => pdf::export_pdf(doc).map_err(|e| anyhow::anyhow!("{e}")),
        ExportFormat::Docx => docx::export_docx(doc),
        ExportFormat::Fountain => Ok(export_fountain(doc).into_bytes()),
        ExportFormat::Fdx => Ok(export_fdx(doc).into_bytes()),
        ExportFormat::Txt => Ok(export_plain_text(doc).into_bytes()),
        ExportFormat::Md => Ok(export_markdown(doc).into_bytes()),
        ExportFormat::Html => Ok(export_html(doc).into_bytes()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenplay_core::ast::{BlockType, ScreenplayBlock};

    fn sample() -> ScreenplayDocument {
        let mut doc = ScreenplayDocument::blank("测试剧本");
        doc.blocks.push(ScreenplayBlock::new(
            BlockType::SceneHeading,
            "外 镇北堡城门 黄昏",
        ));
        doc.blocks.push(ScreenplayBlock::new(BlockType::Action, "风从荒原吹来。"));
        doc.blocks.push(ScreenplayBlock::new(BlockType::Character, "老板娘"));
        doc.blocks.push(ScreenplayBlock::new(
            BlockType::Parenthetical,
            "（冷笑）",
        ));
        doc.blocks.push(ScreenplayBlock::new(
            BlockType::Dialogue,
            "昨日，去了买卖城。",
        ));
        doc
    }

    #[test]
    fn t01_markdown_extension_is_supported() {
        // 缺口修复：说明书要求 Markdown 导出，原实现没有
        assert_eq!(ExportFormat::from_extension("md"), Some(ExportFormat::Md));
        assert_eq!(
            ExportFormat::from_extension("markdown"),
            Some(ExportFormat::Md)
        );
        assert_eq!(ExportFormat::Md.as_str(), "md");
    }

    #[test]
    fn t02_markdown_has_front_matter_and_semantic_markers() {
        let md = export_markdown(&sample());
        assert!(md.starts_with("---\n"), "应有 YAML front matter");
        assert!(md.contains("## 1. 外 镇北堡城门 黄昏"), "场景应为标题");
        assert!(md.contains("**老板娘**"), "人物应加粗");
        assert!(md.contains("*（冷笑）*"), "括注应斜体");
        assert!(md.contains("> 昨日，去了买卖城。"), "对白应引用块");
    }

    #[test]
    fn t03_markdown_preserves_all_text() {
        let doc = sample();
        let md = export_markdown(&doc);
        for b in &doc.blocks {
            if b.text.trim().is_empty() {
                continue;
            }
            let probe = b.text.trim().lines().next().unwrap_or("").trim();
            assert!(
                md.contains(probe),
                "Markdown 丢失内容: {probe}（block {:?}）",
                b.block_type
            );
        }
    }

    #[test]
    fn t04_markdown_scene_numbering_increments() {
        let mut doc = sample();
        doc.blocks.push(ScreenplayBlock::new(
            BlockType::SceneHeading,
            "内 酒肆 夜",
        ));
        let md = export_markdown(&doc);
        assert!(md.contains("## 1."), "首个场景应编号 1");
        assert!(md.contains("## 2."), "第二个场景应编号 2");
    }

    #[test]
    fn t05_all_formats_produce_output() {
        let doc = sample();
        for f in [
            ExportFormat::Fountain,
            ExportFormat::Fdx,
            ExportFormat::Txt,
            ExportFormat::Md,
            ExportFormat::Html,
            ExportFormat::Docx,
        ] {
            let bytes = export(&doc, f).unwrap_or_else(|e| panic!("{f:?} 导出失败: {e}"));
            assert!(!bytes.is_empty(), "{f:?} 输出为空");
        }
    }
}
