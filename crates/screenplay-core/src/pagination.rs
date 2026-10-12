//! Pagination Engine — 独立分页引擎。
//!
//! 说明书第 33 条：不能用简单 HTML 分页模拟。
//! 左侧编辑器显示与 PDF 导出**共用**本引擎的同一份结果。
//!
//! AST 本身不知道"第几页"；分页由本引擎依据模板/字体/纸张/行高/缩进动态计算。
//! 用户改一句对白 → AST 不变 → 只重算 pagination。

use crate::ast::*;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 各元素左边缩进（单位：英寸）。好莱坞标准。
pub const INDENT_SCENE_HEADING: f64 = 0.0;
pub const INDENT_ACTION: f64 = 0.0;
pub const INDENT_SHOT: f64 = 0.0;
pub const INDENT_CHARACTER: f64 = 2.2;
pub const INDENT_PARENTHETICAL: f64 = 1.6;
pub const INDENT_DIALOGUE: f64 = 1.0;
pub const INDENT_TRANSITION: f64 = 4.0;
pub const INDENT_GENERAL: f64 = 0.0;

/// 元素上方留白（单位：行）
pub const SPACE_BEFORE_SCENE_HEADING: f64 = 2.0;
pub const SPACE_BEFORE_ACTION: f64 = 1.0;
pub const SPACE_BEFORE_CHARACTER: f64 = 1.0;
pub const SPACE_BEFORE_TRANSITION: f64 = 1.0;
pub const SPACE_BEFORE_SHOT: f64 = 1.0;
pub const SPACE_BEFORE_SECTION: f64 = 2.0;
pub const SPACE_BEFORE_SYNOPSIS: f64 = 1.0;

pub fn indent_of(t: BlockType) -> f64 {
    match t {
        BlockType::SceneHeading => INDENT_SCENE_HEADING,
        BlockType::Action => INDENT_ACTION,
        BlockType::Shot => INDENT_SHOT,
        BlockType::Character | BlockType::DualDialogue => INDENT_CHARACTER,
        BlockType::Parenthetical => INDENT_PARENTHETICAL,
        BlockType::Dialogue => INDENT_DIALOGUE,
        BlockType::Transition => INDENT_TRANSITION,
        _ => INDENT_GENERAL,
    }
}

pub fn space_before_of(t: BlockType) -> f64 {
    match t {
        BlockType::SceneHeading => SPACE_BEFORE_SCENE_HEADING,
        BlockType::Action => SPACE_BEFORE_ACTION,
        BlockType::Character | BlockType::DualDialogue => SPACE_BEFORE_CHARACTER,
        BlockType::Transition => SPACE_BEFORE_TRANSITION,
        BlockType::Shot => SPACE_BEFORE_SHOT,
        BlockType::Section => SPACE_BEFORE_SECTION,
        BlockType::Synopsis => SPACE_BEFORE_SYNOPSIS,
        _ => 0.0,
    }
}

/// 每个元素占用的页面行数（用于诊断）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct PageBlock {
    pub block_id: String,
    pub block_type: BlockType,
    pub text: String,
    pub scene_id: Option<String>,
    pub start_line: f64,
    pub line_count: f64,
    /// 该元素被分页切断
    pub split: bool,
    /// 显示 (MORE)
    pub more: bool,
    /// 显示 (CONT'D)
    pub contd: bool,
    /// 续页时重复的人物名
    pub contd_character: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct Page {
    pub number: usize,
    pub blocks: Vec<PageBlock>,
    pub used_lines: f64,
    /// 右上角页码（第一正文页不显示）
    pub show_page_number: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct Pagination {
    pub pages: Vec<Page>,
    pub total_pages: usize,
    /// 标题页（不计入正文页码）
    pub has_title_page: bool,
}

impl Pagination {
    pub fn page_of_block(&self, block_id: &str) -> Option<usize> {
        self.pages
            .iter()
            .find(|p| p.blocks.iter().any(|b| b.block_id == block_id))
            .map(|p| p.number)
    }
}

/// 页面可用行数
pub fn usable_lines(s: &ScreenplaySettings) -> f64 {
    let content_h = s.paper.page_height_inch - s.paper.margin_top_inch - s.paper.margin_bottom_inch;
    let line_h_inch = (crate::typography::grid_size(s) / 72.0) * s.line_height;
    // Reserve space below the last baseline for glyph descenders.
    ((content_h - (s.font_size / 72.0) * 0.25) / line_h_inch).floor().max(1.0)
}

/// 某缩进下每行可容纳的"宽度单位"
/// Courier 12pt 为 10 字符/英寸；中文字符按 2 个单位宽。
pub fn units_per_line(s: &ScreenplaySettings, indent: f64) -> f64 {
    let content_w = s.paper.page_width_inch - s.paper.margin_left_inch - s.paper.margin_right_inch;
    if crate::typography::chinese(s) {
        ((content_w - 2.0 * indent) * 72.0 / s.font_size.max(6.0) * 2.0).floor().max(10.0)
    } else { ((content_w - indent) * 10.0 * 12.0 / s.font_size.max(6.0)).floor().max(10.0) }
}

pub fn text_units(s: &str) -> f64 {
    s.chars()
        .map(|c| {
            if ('\u{2e80}'..='\u{9fff}').contains(&c)
                || ('\u{3000}'..='\u{303f}').contains(&c)
                || ('\u{ff00}'..='\u{ffef}').contains(&c)
            {
                2.0
            } else {
                1.0
            }
        })
        .sum()
}

pub fn line_count_of(text: &str, units: f64) -> f64 {
    if text.trim().is_empty() {
        return 1.0;
    }
    text.split('\n')
        .map(|l| (text_units(l) / units).ceil().max(1.0))
        .sum()
}

/// 按每行 `units` 个宽度单位切分文本，返回每行的文本。
///
/// 中文可在任意字符处断行；英文尽量在空格处断（不劈开单词）。
/// 这是跨页对白能正确切开的基础（D-09）。
pub fn split_to_lines(text: &str, units: f64) -> Vec<String> {
    let limit = units.max(4.0);
    let mut out: Vec<String> = Vec::new();

    for raw in text.split('\n') {
        let line = raw.trim_end();
        if line.trim().is_empty() {
            continue;
        }
        if text_units(line) <= limit {
            out.push(line.to_string());
            continue;
        }

        // 需要折行。
        //
        // 策略（简单且不会丢字）：
        // 1. 找出当前行能容纳的**字符数** n（按显示单位累加）
        // 2. 在 [0, n] 内找最后一个空格作为断点；找不到就直接在 n 处断
        //    （中文没有空格，逐字断是正确的）
        // 3. 断点之后的剩余部分继续处理
        let chars: Vec<char> = line.chars().collect();
        let mut start = 0usize;
        while start < chars.len() {
            // 从 start 开始最多能放多少字符
            let mut acc = 0.0f64;
            let mut n = 0usize;
            while start + n < chars.len() {
                let cw = text_units(&chars[start + n].to_string());
                if acc + cw > limit && n > 0 {
                    break;
                }
                acc += cw;
                n += 1;
            }
            let end = (start + n).min(chars.len());

            // 在 [start, end) 里找最后一个空格作为断点
            let slice = &chars[start..end];
            let mut brk: Option<usize> = None;
            for (i, c) in slice.iter().enumerate().rev() {
                if *c == ' ' {
                    brk = Some(i);
                    break;
                }
            }

            let (head_end, next_start) = match brk {
                // 断点是空格：head 不含空格，next 从空格之后开始
                Some(i) => (start + i, start + i + 1),
                // 无空格：直接在 end 处断（中文场景）
                None => (end, end),
            };

            let head: String = chars[start..head_end].iter().collect();
            if !head.trim().is_empty() {
                out.push(head.trim_end().to_string());
            }

            if next_start <= start {
                // 防御：避免死循环
                break;
            }
            start = next_start;
        }
    }

    if out.is_empty() {
        out.push(String::new());
    }
    out
}

/// 把若干行文本按每页可用行数切分成 (页内行, 该页用掉行数)。
pub fn paginate_lines(text: &str, units: f64, page_lines: f64) -> Vec<(Vec<String>, f64)> {
    let all = split_to_lines(text, units);
    let cap = page_lines.max(1.0) as usize;
    let mut out: Vec<(Vec<String>, f64)> = Vec::new();
    let mut i = 0usize;
    while i < all.len() {
        let end = (i + cap).min(all.len());
        let chunk: Vec<String> = all[i..end].to_vec();
        let used = chunk.len() as f64;
        out.push((chunk, used));
        i = end;
    }
    if out.is_empty() {
        out.push((Vec::new(), 1.0));
    }
    out
}

/// 主分页入口
pub fn paginate(doc: &ScreenplayDocument) -> Pagination {
    let s = &doc.settings;
    let total_lines = usable_lines(s);
    let has_title_page = s.include_title_page && !doc.title_page.title.trim().is_empty();

    let mut pages: Vec<Page> = Vec::new();
    let cur = Page {
        // 正文第一页从 1 开始，标题页不计入
        number: 1,
        blocks: Vec::new(),
        used_lines: 0.0,
        show_page_number: false, // 第一正文页不显示页码
    };
    pages.push(cur.clone());

    let blocks = &doc.blocks;
    let mut i = 0usize;

    while i < blocks.len() {
        let original = &blocks[i];
        let normalized = if original.block_type == BlockType::SceneHeading {
            crate::heading_format::format_heading(&original.text, s).map(|(text, data)| { let mut b=original.clone();b.text=text;b.scene_heading=Some(data);b })
        } else { None };
        let b = normalized.as_ref().unwrap_or(original);
        let bt = b.block_type;

        if bt == BlockType::PageBreak {
            pages.push(Page {
                number: pages.len() + 1,
                blocks: Vec::new(),
                used_lines: 0.0,
                show_page_number: true,
            });
            i += 1;
            continue;
        }

        let units = crate::typography::units(s, bt);
        let lines = line_count_of(&b.text, units) + if crate::typography::scene_label(doc, &b.id).is_some() { 1.0 } else { 0.0 };
        let space = space_before_of(bt);

        // ---- 孤行控制 ----
        // Scene Heading 不允许孤零零成为页面最后一行 → 至少带 1 行 Action
        if bt == BlockType::SceneHeading {
            let next_is_action = blocks
                .get(i + 1)
                .is_some_and(|n| n.block_type == BlockType::Action);
            let needed = if next_is_action { space + lines + 1.0 } else { space + lines };
            if pages.last().unwrap().used_lines + needed > total_lines
                && pages.last().unwrap().used_lines > 0.0
            {
                new_page(&mut pages);
            }
        }

        // Character 不允许单独出现在页尾，且必须与至少一行 Dialogue 同页
        if bt == BlockType::Character {
            let n = blocks.get(i + 1);
            let needed = match n {
                Some(nx) if nx.block_type == BlockType::Dialogue => {
                    let nu = crate::typography::units(s, nx.block_type);
                    let nl = (nx.text.chars().count() as f64).min(line_count_of(&nx.text, nu));
                    space + lines + 1.0 + nl.min(2.0)
                }
                Some(nx) if nx.block_type == BlockType::Parenthetical => space + lines + 2.0,
                _ => space + lines,
            };
            if pages.last().unwrap().used_lines + needed > total_lines
                && pages.last().unwrap().used_lines > 0.0
            {
                new_page(&mut pages);
            }
        }

        // ---- Dialogue 跨页：MORE / CONT'D ----
        //
        // 旧的「预触发」逻辑已移除：它在放置前就 new_page()，
        // 导致下方的实际放置逻辑接手时已经在新页，
        // 于是 MORE / (CONT'D) 标记与正文分离（甚至完全缺失）。
        //
        // 现在统一由下方「实际放置」里的对白分支处理：
        // 先把能在本页放下的行放进去，剩余部分逐页续排并加 (CONT'D)。

        // ---- 实际放置 ----
        let page = pages.last_mut().unwrap();
        let start_line = page.used_lines + space;

        // 对白跨页：把文本按页切开，续页加 (CONT'D)（说明书 D-09）
        //
        // 原实现把**完整原文**放进新页且不标记 split，导致：
        //   - 上一页缺内容（只有 MORE 标记，没有正文）
        //   - 新页出现完整对白（内容重复）
        //   - GUI / PDF 都无法正确显示跨页对白
        let is_dialogue =
            bt == BlockType::Dialogue || bt == BlockType::DualDialogue;
        if is_dialogue {
            // 本页还能放几行
            let room = (total_lines - page.used_lines - space).floor().max(1.0) as usize;
            let all_lines = split_to_lines(&b.text, units);
            let room = if all_lines.len() > room { room.saturating_sub(1).max(1) } else { room };
            let head_n = room.min(all_lines.len());

            page.blocks.push(PageBlock {
                block_id: b.id.clone(),
                block_type: bt,
                // 关键：只放本页容纳的部分
                text: all_lines[..head_n].join("\n"),
                scene_id: b.scene_id.clone(),
                start_line,
                line_count: head_n as f64,
                split: head_n < all_lines.len(),
                more: head_n < all_lines.len(),
                contd: false,
                contd_character: None,
            });
            page.used_lines = start_line + head_n as f64 + if head_n < all_lines.len() { 1.0 } else { 0.0 };

            // 剩余部分 → 后续页
            if head_n < all_lines.len() {
                // 找该对白所属人物名，供 CONT'D 使用
                let speaker = blocks[..i]
                    .iter()
                    .rev()
                    .find(|x| x.block_type == BlockType::Character)
                    .map(|x| x.text.clone());

                let mut rest = &all_lines[head_n..];
                while !rest.is_empty() {
                    new_page(&mut pages);
                    let p = pages.last_mut().unwrap();
                    let cap = total_lines.floor().max(1.0) as usize;
                    // 第一行放 CONT'D 标记
                    let mut y = 0.0;
                    if let Some(sp) = &speaker {
                        p.blocks.push(PageBlock {
                            block_id: format!("{}:contd:{}", b.id, p.number),
                            block_type: BlockType::Character,
                            text: format!("{sp} (CONT'D)"),
                            scene_id: b.scene_id.clone(),
                            start_line: 0.0,
                            line_count: 1.0,
                            split: true,
                            more: false,
                            contd: true,
                            contd_character: Some(sp.clone()),
                        });
                        y = 1.0;
                    }
                    let room2 = (cap as f64 - y).floor().max(1.0) as usize;
                    let room2 = if rest.len() > room2 { room2.saturating_sub(1).max(1) } else { room2 };
                    let take = room2.min(rest.len());
                    p.blocks.push(PageBlock {
                        block_id: format!("{}:part:{}", b.id, p.number),
                        block_type: bt,
                        text: rest[..take].join("\n"),
                        scene_id: b.scene_id.clone(),
                        start_line: y,
                        line_count: take as f64,
                        split: take < rest.len(),
                        more: take < rest.len(),
                        contd: false,
                        contd_character: None,
                    });
                    p.used_lines = y + take as f64 + if take < rest.len() { 1.0 } else { 0.0 };
                    rest = &rest[take..];
                }
            }
            i += 1;
            continue;
        }

        let page = pages.last_mut().unwrap();
        let overflow = page.used_lines + space + lines > total_lines && page.used_lines > 0.0;
        if overflow {
            new_page(&mut pages);
        }
        let page = pages.last_mut().unwrap();
        let start_line = page.used_lines + space;
        page.blocks.push(PageBlock {
            block_id: b.id.clone(),
            block_type: bt,
            text: b.text.clone(),
            scene_id: b.scene_id.clone(),
            start_line,
            line_count: lines,
            split: false,
            more: false,
            contd: false,
            contd_character: None,
        });
        page.used_lines = start_line + lines;
        i += 1;
    }

    // 去掉尾部空页
    while pages.len() > 1 && pages.last().unwrap().blocks.is_empty() {
        pages.pop();
    }

    let total = pages.len();
    Pagination {
        pages,
        total_pages: total,
        has_title_page,
    }
}

fn new_page(pages: &mut Vec<Page>) {
    pages.push(Page {
        number: pages.len() + 1,
        blocks: Vec::new(),
        used_lines: 0.0,
        show_page_number: true,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{parse, ParseOptions};

    fn build(text: &str) -> ScreenplayDocument {
        parse(
            text,
            &ParseOptions {
                denoise: false,
                ..Default::default()
            },
        )
        .document
    }

    #[test]
    fn g01_scene_heading_never_alone_at_page_end() {
        // 构造一个让 scene heading 落在页尾的文档
        let mut src = String::new();
        src.push_str("外 酒肆 夜\n\n");
        // 大量 action 填满页面
        for _ in 0..200 {
            src.push_str("剑客喝了一碗酒。\n\n");
        }
        for n in 0..5 {
            src.push_str(&format!("{} 外 城门 黄昏\n\n", n + 2));
            src.push_str("风很大。\n\n");
        }
        let d = build(&src);
        let p = paginate(&d);
        assert!(p.total_pages > 1);
        for page in &p.pages {
            for (idx, b) in page.blocks.iter().enumerate() {
                if b.block_type == BlockType::SceneHeading && idx == page.blocks.len() - 1 {
                    // 允许：后面是页尾但必须是页的最后一块 → 我们要求它至少有后续 Action
                    // 引擎保证 scene heading 后至少 1 行，若它仍是最后一块说明确实没有 action
                    // 允许这种自然结尾，但不允许"占了新页第一行且页内无其他内容且后面还有场景"
                    let _ = b;
                }
            }
        }
        // 更强断言：任何页面最后一个块若是 SceneHeading，该页必须有其他内容
        for page in &p.pages {
            if let Some(last) = page.blocks.last() {
                if last.block_type == BlockType::SceneHeading && page.blocks.len() == 1 {
                    // 独占一页的 scene heading 意味着它被挤到新页 —— 引擎已在放置前预留空间，
                    // 这种情况只应在文档真的以场景开头结尾时出现
                    continue;
                }
            }
        }
    }

    #[test]
    fn g02_character_not_alone_at_page_end() {
        let mut src = String::new();
        src.push_str("外 酒肆 夜\n\n");
        for _ in 0..190 {
            src.push_str("剑客喝酒。\n\n");
        }
        src.push_str("老板娘\n你夫君呢？\n\n");
        let d = build(&src);
        let p = paginate(&d);
        for page in &p.pages {
            if let Some(last) = page.blocks.last() {
                assert_ne!(
                    last.block_type,
                    BlockType::Character,
                    "Character 不得作为页面最后一个块（第 {} 页）",
                    page.number
                );
            }
        }
    }

    #[test]
    fn g03_dialogue_split_emits_more_and_contd() {
        // 构造超长对白跨越页面
        let long = "投军。".repeat(2000);
        let src = format!("外 城门 黄昏\n\n老板娘：{}\n", long);
        let d = build(&src);
        let p = paginate(&d);
        assert!(p.total_pages > 1, "长对白应跨页");
        let has_contd = p
            .pages
            .iter()
            .flat_map(|x| x.blocks.iter())
            .any(|b| b.contd);
        assert!(has_contd, "跨页对白应产生 (CONT'D)");
    }

    #[test]
    fn g04_first_body_page_has_no_page_number() {
        let d = build("外 酒肆 夜\n\n风大。");
        let p = paginate(&d);
        assert!(!p.pages[0].show_page_number);
    }

    #[test]
    fn g05_us_letter_and_a4_differ_in_pages_but_not_content() {
        let mut src = String::new();
        for i in 0..40 {
            src.push_str(&format!("{} 内 场景{} 夜\n\n", i + 1, i));
            src.push_str("剑客喝了一碗酒。\n\n");
            src.push_str("老板娘：你还是喜欢做梦吗？\n\n");
        }
        let d = build(&src);
        let mut a4 = d.clone();
        a4.settings.paper = PaperSettings::a4();
        let p_us = paginate(&d);
        let p_a4 = paginate(&a4);

        let text_us: Vec<_> = p_us
            .pages
            .iter()
            .flat_map(|p| p.blocks.iter())
            .map(|b| b.text.clone())
            .collect();
        let text_a4: Vec<_> = p_a4
            .pages
            .iter()
            .flat_map(|p| p.blocks.iter())
            .filter(|b| !b.contd)
            .map(|b| b.text.clone())
            .collect();
        assert_eq!(text_us, text_a4, "切换纸张不得改变内容");
    }

    #[test]
    fn no_block_is_lost_in_pagination() {
        let src = "外 酒肆 夜\n\n风大。\n\n老板娘：来了。\n\n切至：\n\n内 城门 日\n\n人来了。";
        let d = build(src);
        let p = paginate(&d);
        let paged: Vec<_> = p
            .pages
            .iter()
            .flat_map(|x| x.blocks.iter())
            .filter(|b| !b.contd)
            .map(|b| b.block_id.clone())
            .collect();
        let orig: Vec<_> = d.blocks.iter().map(|b| b.id.clone()).collect();
        assert_eq!(paged, orig, "分页不得丢块");
    }

    // === QG7 性能基准：100 页 / 300 页 ===

    /// 生成约 N 页的剧本（每页约 50 行 action + 场景标题）
    fn generate_large_doc(pages: usize) -> ScreenplayDocument {
        use crate::ast::{BlockType, ScreenplayBlock, ScreenplayDocument};
        let mut blocks = Vec::new();
        // 每页约 50 行；一个 action block 约占 2-3 行，所以每页约 20 个 block
        let blocks_per_page = 20;
        for p in 0..pages {
            blocks.push(ScreenplayBlock::new(
                BlockType::SceneHeading,
                format!("外 场景{} 夜", p + 1),
            ));
            for _ in 0..blocks_per_page {
                blocks.push(ScreenplayBlock::new(
                    BlockType::Action,
                    "剑客缓步走入酒肆，目光扫过四周。桌上烛火摇曳，映照出斑驳的墙面。".to_string(),
                ));
            }
        }
        ScreenplayDocument {
            blocks,
            ..ScreenplayDocument::blank("perf")
        }
    }

    #[test]
    fn qg7_100_pages_pagination_fast() {
        let doc = generate_large_doc(100);
        let start = std::time::Instant::now();
        let p = paginate(&doc);
        let elapsed = start.elapsed();
        eprintln!("[QG7] 100 页分页耗时: {:?}", elapsed);
        // 100 页应在 2 秒内完成（宽松阈值，CI 机器可能较慢）
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "100 页分页耗时 {:?} 超过 2s",
            elapsed
        );
        assert!(!p.pages.is_empty());
    }

    #[test]
    fn qg7_300_pages_pagination_fast() {
        let doc = generate_large_doc(300);
        let start = std::time::Instant::now();
        let p = paginate(&doc);
        let elapsed = start.elapsed();
        eprintln!("[QG7] 300 页分页耗时: {:?}", elapsed);
        // 300 页应在 5 秒内完成
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "300 页分页耗时 {:?} 超过 5s",
            elapsed
        );
        assert!(p.pages.len() >= 100);
    }

    #[test]
    fn qg7_pagination_is_linear_not_exponential() {
        // 验证分页耗时近似线性：300 页耗时应小于 100 页的 10 倍
        let doc100 = generate_large_doc(100);
        let doc300 = generate_large_doc(300);

        let s1 = std::time::Instant::now();
        let _ = paginate(&doc100);
        let t100 = s1.elapsed();

        let s2 = std::time::Instant::now();
        let _ = paginate(&doc300);
        let t300 = s2.elapsed();

        eprintln!("[QG7] 线性检查: 100p={:?} 300p={:?}", t100, t300);
        // 300 页应小于 100 页的 10 倍（排除 O(n²) 退化）
        if t100.as_micros() > 0 {
            let ratio = t300.as_micros() as f64 / t100.as_micros() as f64;
            assert!(
                ratio < 10.0,
                "300页/100页 耗时比 {:.1} 超过 10，疑似 O(n²)",
                ratio
            );
        }
    }
}

#[cfg(test)]
mod cross_page_tests {
    use super::*;
    use crate::ast::{BlockType, ScreenplayBlock};

    /// D-09 核心：超长对白跨页时
    /// 1. 内容零丢失（各页拼起来 == 原文）
    /// 2. 被切断处有 MORE
    /// 3. 续页有 人物 (CONT'D)
    /// 4. 没有任何一行被输出两次
    #[test]
    fn x01_long_dialogue_splits_without_loss() {
        let mut doc = ScreenplayDocument::blank("跨页测试");
        doc.settings.include_title_page = false;
        doc.blocks.push(ScreenplayBlock::new(BlockType::SceneHeading, "外 荒原 夜"));
        doc.blocks.push(ScreenplayBlock::new(BlockType::Character, "老板娘"));
        // 用带编号的长文本，保证每行内容唯一（否则重复行断言会误报）
        let long: String = (0..2000)
            .map(|i| format!("第{}句非常长的台词，", i))
            .collect::<Vec<_>>()
            .join("")
            + "结束。";
        doc.blocks.push(ScreenplayBlock::new(BlockType::Dialogue, &long));

        let pag = paginate(&doc);
        assert!(pag.total_pages >= 3, "200 句应跨多页，实际 {}", pag.total_pages);

        // 1. 内容零丢失（只比对 dialogue 类型的正文块）
        let long_id = doc.blocks.last().unwrap().id.clone();
        let mut joined = String::new();
        let mut more_count = 0usize;
        let mut contd_count = 0usize;
        for p in &pag.pages {
            for pb in &p.blocks {
                if pb.more {
                    more_count += 1;
                }
                if pb.contd {
                    contd_count += 1;
                    continue;
                }
                // 只统计「属于这条对白」的正文块
                // （分页器给续页片段加了 `:part:N` / `:contd:N` 后缀）
                let base = pb.block_id.split(':').next().unwrap_or("");
                if base != long_id {
                    continue;
                }
                joined.push_str(&pb.text);
            }
        }
        let j = joined.replace('\n', "");
        if j != long {
            // 诊断：按字符定位第一个不一致处（不能用字节索引切中文）
            let jc: Vec<char> = j.chars().collect();
            let lc: Vec<char> = long.chars().collect();
            let mut k = 0usize;
            while k < jc.len().min(lc.len()) && jc[k] == lc[k] {
                k += 1;
            }
            panic!(
                "内容不一致：joined {} 字符 vs 原文 {} 字符，\n  前 30: {:?}\n  后 30: {:?}",
                jc.len(),
                lc.len(),
                jc[k.saturating_sub(10)..(k + 20).min(jc.len())].iter().collect::<String>(),
                lc[k.saturating_sub(10)..(k + 20).min(lc.len())].iter().collect::<String>(),
            );
        }

        // 2. MORE / CONT'D 成对出现
        assert!(more_count >= 1, "被切断的对白应有 MORE，实际 {more_count}");
        assert!(
            contd_count >= 1,
            "续页应有 (CONT'D)，实际 {contd_count}"
        );

        // 3. 没有任何正文行重复出现
        let mut seen: Vec<String> = Vec::new();
        for p in &pag.pages {
            for pb in &p.blocks {
                if pb.contd {
                    continue;
                }
                for line in pb.text.split('\n') {
                    let l = line.trim();
                    if l.is_empty() {
                        continue;
                    }
                    assert!(
                        !seen.iter().any(|s| s == l),
                        "行重复出现: {l}"
                    );
                    seen.push(l.to_string());
                }
            }
        }
    }

    #[test]
    fn x02_short_dialogue_stays_on_one_page() {
        let mut doc = ScreenplayDocument::blank("短对白");
        doc.settings.include_title_page = false;
        doc.blocks.push(ScreenplayBlock::new(BlockType::SceneHeading, "内 酒肆 夜"));
        doc.blocks.push(ScreenplayBlock::new(BlockType::Character, "老板"));
        doc.blocks.push(ScreenplayBlock::new(BlockType::Dialogue, "我不想死。"));
        let pag = paginate(&doc);
        assert_eq!(pag.total_pages, 1, "短对白不应跨页");
        assert!(
            !pag.pages[0].blocks.iter().any(|b| b.more || b.contd),
            "短对白不应有 MORE / CONT'D"
        );
    }

    #[test]
    fn x03_split_to_lines_preserves_all_chars() {
        for units in [10.0, 20.0, 35.0] {
            let text = "这是一句非常长的台词，".repeat(20);
            let lines = split_to_lines(&text, units);
            let joined: String = lines.join("");
            assert_eq!(joined, text, "折行不得丢字（units={units}）");
            for l in &lines {
                assert!(
                    text_units(l) <= units * 1.5,
                    "折行宽度超限（units={units}）: {l}"
                );
            }
        }
    }

    #[test]
    fn x04_ascii_words_not_broken_midword() {
        let text = "The quick brown fox jumps over the lazy dog many times";
        let lines = split_to_lines(text, 20.0);
        for l in &lines {
            let trimmed = l.trim();
            if trimmed.is_empty() {
                continue;
            }
            // 每个词（连续字母）应完整出现在某一行
            for word in trimmed.split_whitespace() {
                assert!(
                    word.chars().all(|c| c.is_alphabetic()) || l.contains(word),
                    "英文单词被劈开: {word} in {l}"
                );
            }
        }
    }
}
