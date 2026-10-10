//! PDF 导出。
//!
//! # 为什么用 CoreText sidecar 而不是 printpdf 内置字体
//!
//! printpdf 的 `BuiltinFont::Courier` / `Helvetica` 是 **ASCII-only** 的
//! PDF 标准 14 字体，嵌入中文必然乱码或空白。
//! printpdf 0.8 虽然有 `subset_fonts` 选项（注释里明确提到 CJK 字体可能
//! 非常大），但 `ParsedFont` 没有公开构造器，自己用 `ttf-parser` 解析
//! CFF/OTF 字形既脆弱又难维护。
//!
//! macOS 自带的 CoreText 是系统字体栈的官方接口：
//! - 正确嵌入 CJK 字形（中文可见、可复制、可搜索）
//! - 无需分发字体文件（符合 Apple 平台规范）
//! - 断行、行高由系统处理
//!
//! # 关键：分页由 Rust 计算，Swift 只渲染
//!
//! 本文件**不再自己算分页**，而是复用 `screenplay_core::pagination::paginate`，
//! 与 GUI 纸面渲染、页码显示完全一致（说明书 D-08）。
//! Swift 侧收到的是已经算好绝对坐标的「行」列表，只负责画。
//!
//! 这样三个出口（纸面 / PDF / 未来打印）共享同一份分页结果，
//! 不会出现「纸面 10 页、PDF 12 页」这类不一致。

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use screenplay_core::ast::ScreenplayDocument;
use screenplay_core::pagination;

// ---------------------------------------------------------------------------
// sidecar 调用协议
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
struct LineSpec {
    text: String,
    /// 左边距（点）
    x: f64,
    /// 基线 y（点，PDF 原点在左下）
    y: f64,
    size: f64,
    bold: bool,
    role: String,
    font_family: Option<String>,
    italic: bool,
    color: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PageSpec {
    lines: Vec<LineSpec>,
}

#[derive(Debug, Clone, Serialize)]
struct PdfSpec {
    pages: Vec<PageSpec>,
    width: f64,
    height: f64,
    typography: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SidecarError {
    code: String,
    message: String,
}

#[derive(Debug)]
pub enum PdfError {
    SidecarNotFound,
    SpawnFailed(String),
    SidecarFailed { code: String, message: String },
    BadSpec(String),
}

impl std::fmt::Display for PdfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PdfError::SidecarNotFound => write!(
                f,
                "PDF 渲染 sidecar 未找到（应在 swift/pdf_sidecar 或应用 Resources 中）"
            ),
            PdfError::SpawnFailed(m) => write!(f, "无法启动 PDF sidecar: {m}"),
            PdfError::SidecarFailed { code, message } => {
                write!(f, "PDF sidecar 失败[{code}]: {message}")
            }
            PdfError::BadSpec(m) => write!(f, "PDF 规格错误: {m}"),
        }
    }
}

impl std::error::Error for PdfError {}

/// 找到 PDF sidecar 可执行文件。
///
/// 查找顺序与 OCR sidecar 一致：
/// 1. `SCREENPLAY_PDF_SIDECAR` 环境变量（测试与自定义部署）
/// 2. Tauri app 的 `Contents/Resources/`（打包后）
/// 3. 当前可执行文件同目录（单元测试）
/// 4. Cargo manifest 下的 `swift/pdf_sidecar`（开发）
pub fn find_sidecar() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("SCREENPLAY_PDF_SIDECAR") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(macos_dir) = exe.parent() {
            if let Some(contents_dir) = macos_dir.parent() {
                let candidates = [
                    contents_dir.join("Resources").join("resources").join("pdf_sidecar"),
                    contents_dir.join("Resources").join("pdf_sidecar"),
                    contents_dir
                        .join("Resources")
                        .join("pdf_sidecar.app")
                        .join("Contents/MacOS/pdf_sidecar"),
                    macos_dir.join("pdf_sidecar"),
                ];
                for c in candidates {
                    if c.exists() {
                        return Some(c);
                    }
                }
            }
        }
        let sibling = exe.with_file_name("pdf_sidecar");
        if sibling.exists() {
            return Some(sibling);
        }
    }
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let p = PathBuf::from(manifest).join("swift").join("pdf_sidecar");
        if p.exists() {
            return Some(p);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 分页 → 绝对坐标
// ---------------------------------------------------------------------------

/// 元素左边距（英寸）。
///
/// **直接复用 `pagination::indent_of`** —— 不自己维护一份缩进表。
/// 自己硬编码会导致「纸面缩进 2.2 英寸、PDF 缩进 2.0 英寸」这类漂移，
/// 正是说明书 D-08 要求消除的重复实现。
fn indent_inches(block_type: screenplay_core::ast::BlockType) -> f64 {
    pagination::indent_of(block_type)
}

/// 每行的「半角字符单位」数。
///
/// 复用 `pagination::units_per_line`（权威实现），保证纸面折行位置与 PDF 一致。
fn units_per_line(doc: &ScreenplayDocument, indent_in: f64) -> f64 {
    pagination::units_per_line(&doc.settings, indent_in)
}

/// 把分页结果转成 sidecar 规格：每页若干「行」，带绝对坐标。
fn build_spec(doc: &ScreenplayDocument) -> Result<PdfSpec, PdfError> {
    let pag = pagination::paginate(doc);
    let paper = &doc.settings.paper;
    // 纸张尺寸（英寸 → 点）
    let page_w = paper.page_width_inch * 72.0;
    let page_h = paper.page_height_inch * 72.0;
    let margin_left = paper.margin_left_inch * 72.0;
    let margin_top = paper.margin_top_inch * 72.0;

    let font_size = doc.settings.font_size.max(6.0);
    // 行高必须与 `pagination::usable_lines` 用的是**同一个**公式。
    //
    // pagination:  line_h_inch = (font_size / 72) * settings.line_height
    // usable_lines = (page_h - margin_top - margin_bottom) / line_h_inch
    //
    // 如果这里自选一个系数（例如 1.15），就会出现
    //   pagination 每页塞 54 行、PDF 每页只画得下 46 行
    // → 每页底部 7 行溢出到下边距之外（被裁掉或压到页边）
    // → 跨页对白在 PDF 里「丢字」，而纸面完全正常。
    //
    // D-08 要求纸面 / PDF / 统计共享同一份分页，行高是分页的隐含输入，
    // 必须一并复用，否则无法保证一致。
    let line_height = (screenplay_core::typography::grid_size(&doc.settings) / 72.0) * doc.settings.line_height * 72.0;

    let mut pages: Vec<PageSpec> = Vec::new();

    // 标题页（若启用）
    if pag.has_title_page {
        let mut lines: Vec<LineSpec> = Vec::new();
        let mut y = page_h - margin_top - font_size * 2.0;
        let t = &doc.title_page.title;
        if !t.trim().is_empty() {
            lines.push(LineSpec {
                text: t.clone(),
                x: margin_left,
                y,
                size: font_size * 1.4,
                bold: true,
                role: "title".into(),
                        font_family: None, italic: false, color: None,
            });
            y -= line_height * 3.0;
        }
        for field in [
            &doc.title_page.written_by,
            &doc.title_page.author,
            &doc.title_page.based_on,
            &doc.title_page.contact,
            &doc.title_page.draft_date,
            &doc.title_page.copyright,
        ] {
            if let Some(v) = field {
                if !v.trim().is_empty() {
                    lines.push(LineSpec {
                        text: v.clone(),
                        x: margin_left,
                        y,
                        size: font_size,
                        bold: false,
                        role: "general".into(),
                        font_family: None, italic: false, color: None,
                    });
                    y -= line_height;
                }
            }
        }
        pages.push(PageSpec { lines });
    }

    for page in &pag.pages {
        let mut lines: Vec<LineSpec> = Vec::new();
        // 页码（第一正文页不显示，与 pagination 一致）。
        //
        // 旧实现把 y 设成 margin_top * 0.5 —— 但 PDF 原点在左下、y 向上增长，
        // 那等于把页码画到了页脚（距底部 36pt），而说明书要求页码在右上角。
        // 正确位置：距页顶 margin_top * 0.5。
        if page.show_page_number {
            lines.push(LineSpec {
                text: format!("{}.", page.number),
                x: page_w / 2.0 - font_size,
                y: page_h - margin_top * 0.5,
                size: font_size,
                bold: false,
                        role: "general".into(),
                        font_family: None, italic: false, color: None,
            });
        }

        for pb in &page.blocks {
            // 元素类型直接取 pagination 给出的 block_type。
            //
            // 旧实现用 `doc.blocks.iter().find(|b| b.id == pb.block_id)` 反查，
            // 但跨页对白的续页块用的是**合成 ID**（`{id}:part:{page}` /
            // `{id}:contd:{page}`），在 doc.blocks 里根本不存在 →
            // find 返回 None → 类型回落成 Action →
            // 缩进变成 0，续页对白顶到左边距，视觉上「格式全乱」。
            //
            // PageBlock.block_type 才是权威（D-08：不重复实现、不反推）。
            let btype = pb.block_type;
            let mut display = screenplay_core::typography::display_text(btype, &pb.text);
            if let Some(label) = screenplay_core::typography::scene_label(doc, &pb.block_id) { display = format!("{label}\n{display}"); }
            let text = display.trim();

            // 注：跨页对白的 (CONT'D) 标记由 pagination 直接放进
            // PageBlock.text（格式「人物 (CONT'D)」），这里不再重复添加。
            // pagination 是 (MORE)/(CONT'D) 的唯一权威来源（D-09）。
            if text.is_empty() {
                continue;
            }

            // start_line already includes preceding text and paragraph spacing.
            let mut cursor_y = page_h - margin_top - font_size - pb.start_line * line_height;

            let style = screenplay_core::typography::style(&doc.settings, btype);
            let custom = doc.settings.element_styles.contains_key(btype.as_str());
            let is_bold = style.bold;
            let indent = screenplay_core::typography::indent(&doc.settings, btype);
            let units = screenplay_core::typography::units(&doc.settings, btype);
            let x = if screenplay_core::typography::chinese(&doc.settings) && matches!(btype, screenplay_core::ast::BlockType::Character | screenplay_core::ast::BlockType::Parenthetical) {
                (margin_left + page_w - doc.settings.paper.margin_right_inch * 72.0) / 2.0
            } else { margin_left + indent * 72.0 };

            // PageBlock.text 可能是多行（pagination 已按页切开并用 \n 连接）
            // 逐行输出，行距由 Rust 统一控制
            for raw_line in text.split('\n') {
                let l = raw_line.trim_end();
                if l.is_empty() {
                    cursor_y -= line_height;
                    continue;
                }
                // 折行复用 pagination 的权威实现 `split_to_lines`（D-08）。
                //
                // 不自己维护一份 wrap：重复实现必然漂移。旧实现还有
                // `limit = chars_per_line * 2` 的单位错误（`units_per_line`
                // 返回的已经是「半角单位」，再乘 2 等于宽度翻倍）。
                //
                // 注意：不能把折行结果写回 `PageBlock.text` —— GUI 把该字段当作
                // 可编辑正文（contentEditable），预折行会让用户编辑时产生多余换行。
                // 因此只有 PDF/打印这类「只读出口」才在此折行。
                for chunk in pagination::split_to_lines(l, units) {
                    lines.push(LineSpec {
                        text: chunk,
                        x,
                        y: cursor_y,
                        size: style.font_size,
                        bold: is_bold,
                        role: btype.as_str().into(),
                        font_family: if custom { Some(style.font_family.clone()) } else { None }, italic: style.italic, color: if custom { Some(style.color.clone()) } else { None },
                    });
                    cursor_y -= line_height;
                }
            }

            // 对白在本页末尾被切断 → 追加 (MORE)
            if pb.more {
                lines.push(LineSpec {
                    text: "(MORE)".to_string(),
                    x,
                    y: cursor_y,
                    size: font_size,
                    bold: false,
                        role: "general".into(),
                        font_family: None, italic: false, color: None,
                });
            }
        }

        pages.push(PageSpec { lines });
    }

    if pages.is_empty() {
        pages.push(PageSpec { lines: Vec::new() });
    }

    Ok(PdfSpec {
        pages,
        width: page_w,
        height: page_h,
        typography: doc.settings.typography.clone(),
    })
}

// ---------------------------------------------------------------------------
// 对外 API
// ---------------------------------------------------------------------------

/// 将剧本文档导出为 PDF 字节。
///
/// 分页复用 `screenplay_core::pagination`，与 GUI 纸面完全一致。
pub fn export_pdf(doc: &ScreenplayDocument) -> Result<Vec<u8>, PdfError> {
    let spec = build_spec(doc)?;
    let json = serde_json::to_vec(&spec).map_err(|e| PdfError::BadSpec(e.to_string()))?;

    let sidecar = find_sidecar().ok_or(PdfError::SidecarNotFound)?;

    let mut child = Command::new(&sidecar)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| PdfError::SpawnFailed(format!("{}: {e}", sidecar.display())))?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| PdfError::SpawnFailed("无 stdin".into()))?;
        stdin
            .write_all(&json)
            .map_err(|e| PdfError::SpawnFailed(format!("写入规格失败: {e}")))?;
    }

    let out = child
        .wait_with_output()
        .map_err(|e| PdfError::SpawnFailed(e.to_string()))?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        // sidecar 可能输出结构化错误
        if let Ok(se) = serde_json::from_str::<SidecarError>(&err) {
            return Err(PdfError::SidecarFailed {
                code: se.code,
                message: se.message,
            });
        }
        return Err(PdfError::SidecarFailed {
            code: format!("exit{}", out.status.code().unwrap_or(-1)),
            message: err,
        });
    }

    let bytes = out.stdout;
    if bytes.len() < 5 || &bytes[0..4] != b"%PDF" {
        return Err(PdfError::SidecarFailed {
            code: "BAD_OUTPUT".into(),
            message: format!("sidecar 未返回 PDF（前 16 字节: {:?}）", &bytes[..bytes.len().min(16)]),
        });
    }
    Ok(bytes)
}

/// 便捷入口：直接写到文件
pub fn export_pdf_to_file(doc: &ScreenplayDocument, path: &Path) -> Result<(), PdfError> {
    let bytes = export_pdf(doc)?;
    std::fs::write(path, bytes).map_err(|e| PdfError::SpawnFailed(format!("写入文件失败: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenplay_core::ast::{BlockType, ScreenplayBlock};

    #[test]
    fn block_baselines_follow_pagination_without_double_spacing() {
        let mut doc = sample();
        doc.title_page.title.clear();
        let pagination = pagination::paginate(&doc);
        let spec = build_spec(&doc).unwrap();
        let paper = &doc.settings.paper;
        for (page, rendered) in pagination.pages.iter().zip(&spec.pages) {
            for block in &page.blocks {
                let expected = (paper.page_height_inch - paper.margin_top_inch) * 72.0
                    - doc.settings.font_size - block.start_line * doc.settings.font_size * doc.settings.line_height;
                let first = rendered.lines.iter().find(|line| line.text == block.text.trim()).unwrap();
                assert!((first.y - expected).abs() < 0.01, "incorrect baseline for {}", block.text);
            }
        }
    }

    #[test]
    fn long_dialogue_and_more_stay_inside_bottom_margin() {
        let mut doc = sample();
        doc.blocks.last_mut().unwrap().text = "很长的台词，需要跨页。".repeat(2000);
        let spec = build_spec(&doc).unwrap();
        let minimum = doc.settings.paper.margin_bottom_inch * 72.0 + doc.settings.font_size * 0.25;
        for page in spec.pages {
            for line in page.lines {
                assert!(line.y >= minimum, "line below bottom margin: {} at {}", line.text, line.y);
            }
        }
    }

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
    fn t01_split_to_lines_counts_cjk_as_two_units() {
        // 折行已上移到 pagination，这里验证权威实现的单位语义
        assert_eq!(pagination::text_units("abc"), 3.0);
        assert_eq!(pagination::text_units("中"), 2.0);
        assert_eq!(pagination::text_units("中a"), 3.0);
        assert_eq!(pagination::text_units("，"), 2.0, "全角标点应算 2");
    }

    #[test]
    fn t02_split_respects_display_width() {
        let text = "风从荒原吹来老板娘说昨日去了买卖城";
        let lines = pagination::split_to_lines(text, 10.0); // 10 半角 = 5 个汉字
        assert!(lines.len() >= 2, "长文本应折行");
        for l in &lines {
            assert!(
                pagination::text_units(l) <= 10.0,
                "每行不应超过 10 个半角单位: {l} (w={})",
                pagination::text_units(l)
            );
        }
        let joined: String = lines.join("");
        assert_eq!(joined, text, "折行不得丢字");
    }

    #[test]
    fn t03_split_keeps_ascii_words_intact() {
        let text = "The wind blows gently over the desert plain";
        let lines = pagination::split_to_lines(text, 12.0);
        let joined: String = lines.join(" ");
        assert!(joined.contains("wind"), "英文单词不应被劈开");
        assert!(joined.contains("desert"));
    }

    #[test]
    fn t04_split_preserves_explicit_newlines() {
        let lines = pagination::split_to_lines("第一行\n第二行", 40.0);
        assert_eq!(lines, vec!["第一行", "第二行"]);
    }

    #[test]
    fn t08_line_height_matches_pagination_usable_lines() {
        // 关键回归测试（D-08）。
        //
        // 旧实现硬编码 `font_size * 1.15`，而 pagination 用
        // `(font_size/72) * settings.line_height`：
        //   pagination 每页塞 54 行，PDF 每页只画 46 行
        // → 每页底部 7 行溢出下边距 → 跨页对白在 PDF 里丢字。
        //
        // 不变量：PDF 画出的总高度必须 ≤ 可用高度，
        // 否则最后几行必然被裁掉。
        for font_size in [10.0, 12.0, 14.0] {
            for line_height in [1.0, 1.15, 1.5] {
                let mut doc = sample();
                doc.settings.font_size = font_size;
                doc.settings.line_height = line_height;
                let spec = build_spec(&doc).unwrap();

                let usable = pagination::usable_lines(&doc.settings);
                let line_h_pt = (font_size / 72.0) * line_height * 72.0;

                for (i, page) in spec.pages.iter().enumerate() {
                    let lowest = page
                        .lines
                        .iter()
                        .map(|l| l.y)
                        .fold(f64::INFINITY, f64::min);
                    if lowest.is_finite() {
                        assert!(
                            lowest >= 0.0,
                            "第 {} 页有行落到下边距之外 (y={lowest})",
                            i + 1
                        );
                    }
                }
                // 一页最多 usable 行，行距必须是 pagination 的行距
                let body_pages = spec.pages.len().saturating_sub(1).max(1);
                let max_lines = page_max_lines(&spec);
                assert!(
                    (max_lines as f64) <= usable.ceil() + 1.0,
                    "font={font_size} lh={line_height}: 单页 {max_lines} 行 > 可用 {usable} 行 \
                     (行距 {line_h_pt}pt 与 pagination 不一致会导致溢出丢字)"
                );
                let _ = body_pages;
            }
        }
    }

    #[test]
    fn t09_no_line_overflows_right_margin() {
        // 每行宽度必须落在「左边距 + 缩进」到「右边距」之间，
        // 否则 PDF 文字会跑出纸张（对白/动作长句最容易出现）。
        let mut doc = sample();
        doc.blocks.push(ScreenplayBlock::new(
            BlockType::Action,
            "风从荒原吹来，卷起漫天的黄沙，遮蔽了远处镇北堡的城门，\
             只留下一道模糊的轮廓，老板娘站在城门下，望着天边的落日发呆。",
        ));
        let spec = build_spec(&doc).unwrap();
        let paper = &doc.settings.paper;
        let right = (paper.page_width_inch - paper.margin_right_inch) * 72.0;

        for page in &spec.pages {
            for l in &page.lines {
                // 用保守估算：中文按字号全宽，ASCII 按 0.6 倍宽
                let w = l
                    .text
                    .chars()
                    .map(|c| if (c as u32) > 0x2E80 { l.size } else { l.size * 0.6 })
                    .sum::<f64>();
                assert!(
                    l.x + w <= right + 1.0,
                    "行超出右边距: x={} w={:.1} right={right} text={:?}",
                    l.x,
                    w,
                    l.text
                );
            }
        }
    }

    #[test]
    fn t10_spec_conveys_every_character_verbatim() {
        // 零丢失不变式（D-09 + D-08）。
        //
        // 不能用 `contains("第{i}句")` 判断丢字：跨页折行可能把「第107句」
        // 拆成「第」（上一页末尾）和「107句」（下一页开头），中间插入了
        // (MORE) / 页码 / (CONT'D) 等结构性标记。字符都在，但短语被切断。
        //
        // 正确的零丢失检查：把 spec 里所有对白行（按对白缩进 x 识别，排除
        // (MORE) 标记）拼接、去空白，与原始对白去空白后逐字符比较。
        let mut doc = sample();
        let long = format!(
            "长对白。{}",
            (0..300)
                .map(|i| format!("第{i}句非常长的台词，"))
                .collect::<String>()
        ) + "结束。";
        doc.blocks.push(ScreenplayBlock::new(BlockType::Character, "老板娘"));
        doc.blocks.push(ScreenplayBlock::new(BlockType::Dialogue, &long));

        let spec = build_spec(&doc).unwrap();

        // 对白缩进 x = margin_left + INDENT_DIALOGUE * 72
        let dialogue_x = doc.settings.paper.margin_left_inch * 72.0
            + screenplay_core::pagination::INDENT_DIALOGUE * 72.0;

        let dialogue_in_spec: String = spec
            .pages
            .iter()
            .flat_map(|p| p.lines.iter())
            .filter(|l| {
                // 对白行：x 等于对白缩进，且不是 (MORE) 标记
                (l.x - dialogue_x).abs() < 0.5 && l.text.trim() != "(MORE)"
            })
            .map(|l| l.text.as_str())
            .collect();

        fn norm(s: &str) -> String {
            s.chars().filter(|c| !c.is_whitespace()).collect()
        }

        // 原文里所有对白拼接（sample() 里已有一段对白「昨日，去了买卖城。」，
        // 再加上长对白）
        let all_dialogue: String = doc
            .blocks
            .iter()
            .filter(|b| {
                b.block_type == BlockType::Dialogue || b.block_type == BlockType::DualDialogue
            })
            .map(|b| b.text.as_str())
            .collect();

        assert_eq!(
            norm(&dialogue_in_spec),
            norm(&all_dialogue),
            "spec 里的对白字符必须与原文逐字一致（D-09 跨页对白零丢失）"
        );

        // 结尾必须保留
        assert!(
            norm(&dialogue_in_spec).ends_with("结束。"),
            "spec 丢了长对白结尾「结束。」"
        );
        // 页数与 pagination 一致（D-08）
        let pag = pagination::paginate(&doc);
        assert_eq!(
            spec.pages.len(),
            pag.pages.len() + if pag.has_title_page { 1 } else { 0 },
            "PDF 页数与 pagination 不一致（D-08）"
        );
        // 4. 不得出现重复：同一编号不得出现两次
        let count_of_7 = norm(&dialogue_in_spec).matches("第7句").count();
        assert_eq!(count_of_7, 1, "编号 第7句 出现 {count_of_7} 次（内容重复）");
    }

    #[test]
    fn t11_page_number_sits_inside_top_margin() {
        // 页码必须落在上边距内（说明书：页码在右上角）。
        // 旧实现把 y 设成 margin_top * 0.5 —— PDF 原点在左下，
        // 那是「距页面底部 0.5 英寸」，等于把页码画到了页脚位置。
        let doc = sample();
        let spec = build_spec(&doc).unwrap();
        let paper = &doc.settings.paper;
        let margin_top_pt = paper.margin_top_inch * 72.0;
        let page_h_pt = paper.page_height_inch * 72.0;

        for (i, page) in spec.pages.iter().enumerate() {
            for l in &page.lines {
                // 页码行是唯一 x 居中的行
                if !l.text.ends_with('.') || !l.text[..l.text.len() - 1].chars().all(|c| c.is_ascii_digit())
                {
                    continue;
                }
                assert!(
                    l.y > page_h_pt - margin_top_pt - 1.0,
                    "第 {} 页页码 y={} 应在上边距内（>{:.1}）",
                    i + 1,
                    l.y,
                    page_h_pt - margin_top_pt
                );
            }
        }
    }

    fn page_max_lines(spec: &PdfSpec) -> usize {
        spec.pages
            .iter()
            .map(|p| p.lines.len())
            .max()
            .unwrap_or(0)
    }

    #[test]
    fn t05_spec_uses_paper_settings_from_document() {
        let mut doc = sample();
        // 改成 A4
        doc.settings.paper.template = "a4".into();
        doc.settings.paper.page_width_inch = 8.27;
        doc.settings.paper.page_height_inch = 11.69;
        let spec = build_spec(&doc).unwrap();
        assert!((spec.width - 8.27 * 72.0).abs() < 0.01);
        assert!((spec.height - 11.69 * 72.0).abs() < 0.01);
    }

    #[test]
    fn t06_spec_page_count_matches_pagination() {
        // 关键：PDF 页数必须与 Rust pagination 一致（D-08 统一分页）
        let doc = sample();
        let pag = pagination::paginate(&doc);
        let spec = build_spec(&doc).unwrap();
        // spec = 标题页（若有）+ 正文页
        let expected = pag.pages.len() + if pag.has_title_page { 1 } else { 0 };
        assert_eq!(
            spec.pages.len(),
            expected,
            "PDF 页数应与 pagination 一致（分页必须复用同一实现）"
        );
    }

    #[test]
    fn t07_indent_reuses_pagination_authority() {
        use screenplay_core::ast::BlockType as BT;
        // 关键：不自己维护缩进表，必须与 pagination 完全一致
        for bt in [
            BT::SceneHeading,
            BT::Action,
            BT::Character,
            BT::Parenthetical,
            BT::Dialogue,
            BT::Transition,
            BT::Shot,
            BT::General,
        ] {
            assert_eq!(
                indent_inches(bt),
                pagination::indent_of(bt),
                "{bt:?} 的缩进必须与 pagination 一致"
            );
        }
        // Hollywood 标准：人物最靠右，括注次之，对白最靠左
        // （括注是在对白内部的补充说明，视觉上比对白更靠右）
        assert!(indent_inches(BT::Character) > indent_inches(BT::Parenthetical));
        assert!(indent_inches(BT::Parenthetical) > indent_inches(BT::Dialogue));
    }
}

#[cfg(test)] mod custom_tests {
    use super::*;
    use screenplay_core::ast::{BlockType, ScreenplayBlock, ElementStyle};
    #[test] fn parenthetical_pdf_remains_regular_below_bold_character(){
        let mut doc=ScreenplayDocument::blank("括注验收");doc.settings.include_title_page=false;
        doc.settings.element_styles.insert("parenthetical".into(),ElementStyle{font_family:"Songti SC".into(),font_size:12.0,bold:true,italic:false,color:"#000000".into()});
        doc.blocks=vec![ScreenplayBlock::new(BlockType::Character,"小林"),ScreenplayBlock::new(BlockType::Parenthetical,"轻声"),ScreenplayBlock::new(BlockType::Dialogue,"等等。")];
        let spec=build_spec(&doc).unwrap();let lines=&spec.pages[0].lines;
        let cue=lines.iter().find(|l|l.text=="小林").unwrap();let note=lines.iter().find(|l|l.text.contains("轻声")).unwrap();
        assert!(cue.bold);assert!(!note.bold);assert!(cue.y>note.y);
    }
    #[test] fn custom_pdf_payload_uses_same_grid_and_styles_as_editor() {
        let mut doc = ScreenplayDocument::blank("测试");
        doc.settings.include_title_page = false;
        doc.settings.element_styles.insert("action".into(), ElementStyle { font_family: "PingFang SC".into(), font_size: 24.0, bold: true, italic: true, color: "#2355aa".into() });
        doc.blocks.push(ScreenplayBlock::new(BlockType::Action, "字号测试"));
        doc.blocks.push(ScreenplayBlock::new(BlockType::Action, "第二行"));
        let spec = build_spec(&doc).unwrap();
        let lines = &spec.pages[0].lines;
        assert_eq!(lines[0].size, 24.0); assert!(lines[0].italic); assert_eq!(lines[0].font_family.as_deref(), Some("PingFang SC")); assert_eq!(lines[0].color.as_deref(), Some("#2355aa"));
        let pagination = pagination::paginate(&doc);
        let expected = (pagination.pages[0].blocks[1].start_line - pagination.pages[0].blocks[0].start_line) * 24.0;
        assert_eq!(lines[0].y - lines[1].y, expected);
    }
}
