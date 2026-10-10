//! Semantic Diff — 场景/块级语义比较，而非纯字符串比较。
//!
//! 说明书第 64 条：显示形式应为
//! ```text
//! Scene 3
//! 老板娘 Dialogue：
//! - ...
//! + ...
//! 新增 Action 1 段
//! Scene 8 移动到 Scene 9 后
//! ```

use crate::ast::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DiffEntry {
    SceneAdded {
        scene: String,
        number: String,
    },
    SceneRemoved {
        scene: String,
        number: String,
    },
    SceneMoved {
        scene: String,
        from: usize,
        to: usize,
    },
    BlockAdded {
        scene: String,
        block_type: String,
        text: String,
    },
    BlockRemoved {
        scene: String,
        block_type: String,
        text: String,
    },
    BlockChanged {
        scene: String,
        block_type: String,
        old_text: String,
        new_text: String,
        /// 字符级相似度 0.0-1.0
        similarity: f32,
    },
    BlockRetyped {
        scene: String,
        old_type: String,
        new_type: String,
        text: String,
    },
    TitlePageChanged {
        field: String,
        old_value: String,
        new_value: String,
    },
    SettingsChanged {
        field: String,
        old_value: String,
        new_value: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticDiff {
    pub from_version: Option<String>,
    pub to_version: Option<String>,
    pub entries: Vec<DiffEntry>,
    pub added_blocks: usize,
    pub removed_blocks: usize,
    pub changed_blocks: usize,
    /// 相似度高于此值视为"修改"而非"删除+新增"
    pub similarity_threshold: f32,
}

impl SemanticDiff {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn summary(&self) -> String {
        format!(
            "+{} / -{} / ~{}",
            self.added_blocks, self.removed_blocks, self.changed_blocks
        )
    }
}

fn similarity(a: &str, b: &str) -> f32 {
    if a == b {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    use similar::TextDiff;
    // 多行文本用行级比较，单行文本用字符级比较（否则单行不同会得到 0.0）
    let sim = if a.contains('\n') || b.contains('\n') {
        TextDiff::from_lines(a, b).ratio()
    } else {
        TextDiff::from_chars(a, b).ratio()
    };
    (sim as f32).clamp(0.0, 1.0)
}

/// 生成语义差异
pub fn diff_documents(
    old: &ScreenplayDocument,
    new: &ScreenplayDocument,
    from_version: Option<String>,
    to_version: Option<String>,
) -> SemanticDiff {
    let mut entries: Vec<DiffEntry> = Vec::new();
    let mut added = 0usize;
    let mut removed = 0usize;
    let mut changed = 0usize;

    // 1. 场景序列对比（检测新增/删除/移动）
    let old_scenes: Vec<&Scene> = old.scenes.iter().collect();
    let new_scenes: Vec<&Scene> = new.scenes.iter().collect();
    let old_nums: Vec<&str> = old_scenes.iter().filter_map(|s| s.number.as_deref()).collect();
    let new_nums: Vec<&str> = new_scenes.iter().filter_map(|s| s.number.as_deref()).collect();

    for s in &new_scenes {
        if !old_nums.contains(&s.number.as_deref().unwrap_or("")) {
            entries.push(DiffEntry::SceneAdded {
                scene: s.location.clone(),
                number: s.number.clone().unwrap_or_default(),
            });
        }
    }
    for s in &old_scenes {
        if !new_nums.contains(&s.number.as_deref().unwrap_or("")) {
            entries.push(DiffEntry::SceneRemoved {
                scene: s.location.clone(),
                number: s.number.clone().unwrap_or_default(),
            });
        }
    }
    // 移动：编号顺序不一致
    if old_nums.len() == new_nums.len() && old_nums != new_nums {
        let common: Vec<&str> = new_nums.iter().filter(|n| old_nums.contains(n)).copied().collect();
        for (new_idx, n) in new_nums.iter().enumerate() {
            if common.contains(n) {
                if let Some(old_idx) = old_nums.iter().position(|x| x == n) {
                    if old_idx != new_idx {
                        if let Some(s) = new_scenes.get(new_idx) {
                            entries.push(DiffEntry::SceneMoved {
                                scene: s.location.clone(),
                                from: old_idx + 1,
                                to: new_idx + 1,
                            });
                        }
                    }
                }
            }
        }
    }

    // 2. 块级对比：按 scene 分组，用块内容做键
    let old_groups = group_by_scene(old);
    let new_groups = group_by_scene(new);

    let mut scene_keys: Vec<String> = old_groups.keys().cloned().collect();
    for k in new_groups.keys() {
        if !scene_keys.contains(k) {
            scene_keys.push(k.clone());
        }
    }

    for key in scene_keys {
        let ob = old_groups.get(&key).cloned().unwrap_or_default();
        let nb = new_groups.get(&key).cloned().unwrap_or_default();
        diff_group(&key, &ob, &nb, &mut entries, &mut added, &mut removed, &mut changed);
    }

    // 3. 标题页
    let tp = &old.title_page;
    let np = &new.title_page;
    macro_rules! cmp_field {
        ($f:ident) => {
            if tp.$f != np.$f {
                entries.push(DiffEntry::TitlePageChanged {
                    field: stringify!($f).to_string(),
                    old_value: format!("{:?}", tp.$f),
                    new_value: format!("{:?}", np.$f),
                });
            }
        };
    }
    cmp_field!(title);
    cmp_field!(written_by);
    cmp_field!(author);
    cmp_field!(based_on);
    cmp_field!(contact);
    cmp_field!(draft_date);
    cmp_field!(copyright);

    // 4. 设置
    if old.settings.include_title_page != new.settings.include_title_page {
        entries.push(DiffEntry::SettingsChanged {
            field: "include_title_page".into(),
            old_value: old.settings.include_title_page.to_string(),
            new_value: new.settings.include_title_page.to_string(),
        });
    }
    if old.settings.show_scene_numbers != new.settings.show_scene_numbers {
        entries.push(DiffEntry::SettingsChanged {
            field: "show_scene_numbers".into(),
            old_value: old.settings.show_scene_numbers.to_string(),
            new_value: new.settings.show_scene_numbers.to_string(),
        });
    }
    if (old.settings.paper.page_width_inch - new.settings.paper.page_width_inch).abs() > f64::EPSILON {
        entries.push(DiffEntry::SettingsChanged {
            field: "paper".into(),
            old_value: old.settings.paper.template.clone(),
            new_value: new.settings.paper.template.clone(),
        });
    }

    SemanticDiff {
        from_version,
        to_version,
        entries,
        added_blocks: added,
        removed_blocks: removed,
        changed_blocks: changed,
        similarity_threshold: 0.45,
    }
}

type Group = Vec<(BlockType, String, String)>; // (type, id, text)

fn group_by_scene(doc: &ScreenplayDocument) -> std::collections::BTreeMap<String, Group> {
    let mut map: std::collections::BTreeMap<String, Group> = std::collections::BTreeMap::new();
    // 用「编号·地点」作为场景键，避免 scene_id（UUID）每次解析不同导致无法配对
    let scene_key = |sid: &str| -> String {
        doc.scene(sid)
            .map(|s| format!("{}·{}", s.number.clone().unwrap_or_default(), s.location))
            .unwrap_or_else(|| "_nosplit".into())
    };
    for b in &doc.blocks {
        if b.text.trim().is_empty() {
            continue;
        }
        let key = b
            .scene_id
            .as_deref()
            .map(scene_key)
            .unwrap_or_else(|| "_nosplit".into());
        map.entry(key)
            .or_default()
            .push((b.block_type, b.id.clone(), b.text.clone()));
    }
    map
}

fn scene_label(_doc: &ScreenplayDocument, key: &str) -> String {
    if key == "_nosplit" {
        return "（场景外）".into();
    }
    // key 格式为 "编号·地点"
    if let Some((num, loc)) = key.split_once('·') {
        format!("Scene {} · {}", num, loc)
    } else {
        format!("Scene {}", key)
    }
}

fn diff_group(
    key: &str,
    old: &Group,
    new: &Group,
    entries: &mut Vec<DiffEntry>,
    added: &mut usize,
    removed: &mut usize,
    changed: &mut usize,
) {
    // 贪心匹配：先用相同文本配对，再做相似匹配
    let mut old_used = vec![false; old.len()];
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    let mut new_unmatched: Vec<usize> = Vec::new();

    for (ni, nb) in new.iter().enumerate() {
        // 先用 (类型, 文本) 精确配对（同一文档不同解析的 block id 不同，不能用 id 配对）
        if let Some(oi) = old
            .iter()
            .enumerate()
            .position(|(oi, ob)| !old_used[oi] && ob.0 == nb.0 && ob.2 == nb.2)
        {
            old_used[oi] = true;
            pairs.push((oi, ni));
        } else {
            new_unmatched.push(ni);
        }
    }

    // 相似匹配（处理修改）
    let mut still_new: Vec<usize> = Vec::new();
    for ni in new_unmatched {
        let mut best: Option<(usize, f32)> = None;
        for (oi, ob) in old.iter().enumerate() {
            if old_used[oi] {
                continue;
            }
            let s = similarity(&ob.2, &new[ni].2);
            if s >= 0.45 && best.is_none_or(|(_, bs)| s > bs) {
                best = Some((oi, s));
            }
        }
        match best {
            Some((oi, s)) => {
                old_used[oi] = true;
                pairs.push((oi, ni));
                // 记录相似度到临时
                entries.push(DiffEntry::BlockChanged {
                    scene: key.to_string(),
                    block_type: new[ni].0.as_str().to_string(),
                    old_text: old[oi].2.clone(),
                    new_text: new[ni].2.clone(),
                    similarity: s,
                });
                *changed += 1;
            }
            None => still_new.push(ni),
        }
    }

    // 新增
    for ni in still_new {
        entries.push(DiffEntry::BlockAdded {
            scene: key.to_string(),
            block_type: new[ni].0.as_str().to_string(),
            text: new[ni].2.clone(),
        });
        *added += 1;
    }

    // 删除
    for (oi, ob) in old.iter().enumerate() {
        if !old_used[oi] {
            entries.push(DiffEntry::BlockRemoved {
                scene: key.to_string(),
                block_type: ob.0.as_str().to_string(),
                text: ob.2.clone(),
            });
            *removed += 1;
        }
    }

    // 类型变化检查
    for (oi, ni) in pairs {
        if old[oi].0 != new[ni].0 {
            // 已在 BlockChanged 中记录过则升级为 Retyped
            let already = entries.iter().any(
                |e| matches!(e, DiffEntry::BlockChanged { old_text, new_text, .. }
                    if *old_text == old[oi].2 && *new_text == new[ni].2),
            );
            if already {
                if let Some(pos) = entries.iter().position(
                    |e| matches!(e, DiffEntry::BlockChanged { old_text, new_text, .. }
                        if *old_text == old[oi].2 && *new_text == new[ni].2),
                ) {
                    entries.remove(pos);
                    *changed -= 1;
                }
                entries.push(DiffEntry::BlockRetyped {
                    scene: key.to_string(),
                    old_type: old[oi].0.as_str().to_string(),
                    new_type: new[ni].0.as_str().to_string(),
                    text: new[ni].2.clone(),
                });
                *changed += 1;
            } else {
                entries.push(DiffEntry::BlockRetyped {
                    scene: key.to_string(),
                    old_type: old[oi].0.as_str().to_string(),
                    new_type: new[ni].0.as_str().to_string(),
                    text: new[ni].2.clone(),
                });
                *changed += 1;
            }
        }
    }
}

/// 为 CLI / MCP 生成人类可读摘要
///
/// `old` / `new` 保留在签名里：调用方（GUI 版本比较面板、CLI `diff`、
/// MCP `compare_versions`）都以同一对文档调用本函数，参数是语义契约的一部分。
/// 当前实现只依赖 `d`（SemanticDiff 已含全部前后值），故标注为可复用上下文。
#[allow(unused_variables)]
pub fn render_diff(old: &ScreenplayDocument, new: &ScreenplayDocument, d: &SemanticDiff) -> String {
    let mut out = String::new();
    if let (Some(a), Some(b)) = (&d.from_version, &d.to_version) {
        out.push_str(&format!("版本 {} → {}\n", a, b));
    }
    out.push_str(&format!("摘要: {}\n\n", d.summary()));
    let mut last_scene = String::new();
    for e in &d.entries {
        let (scene_key, body) = match e {
            DiffEntry::SceneAdded { scene, number } => {
                out.push_str(&format!("+ 新增场景 {} {}\n", number, scene));
                continue;
            }
            DiffEntry::SceneRemoved { scene, number } => {
                out.push_str(&format!("- 删除场景 {} {}\n", number, scene));
                continue;
            }
            DiffEntry::SceneMoved { scene, from, to } => {
                out.push_str(&format!("~ 场景「{}」从第 {} 位移动到第 {} 位\n", scene, from, to));
                continue;
            }
            DiffEntry::TitlePageChanged {
                field,
                old_value,
                new_value,
            } => {
                out.push_str(&format!("~ 标题页 {}: {} → {}\n", field, old_value, new_value));
                continue;
            }
            DiffEntry::SettingsChanged {
                field,
                old_value,
                new_value,
            } => {
                out.push_str(&format!("~ 设置 {}: {} → {}\n", field, old_value, new_value));
                continue;
            }
            DiffEntry::BlockAdded {
                scene,
                block_type,
                text,
            } => (scene.clone(), format!("+ [{}] {}\n", block_type, text)),
            DiffEntry::BlockRemoved {
                scene,
                block_type,
                text,
            } => (scene.clone(), format!("- [{}] {}\n", block_type, text)),
            DiffEntry::BlockChanged {
                scene,
                block_type,
                old_text,
                new_text,
                ..
            } => (
                scene.clone(),
                format!("~ [{}]\n    - {}\n    + {}\n", block_type, old_text, new_text),
            ),
            DiffEntry::BlockRetyped {
                scene,
                old_type,
                new_type,
                text,
            } => (
                scene.clone(),
                format!("~ [{} → {}] {}\n", old_type, new_type, text),
            ),
        };
        if scene_key != last_scene {
            let label = scene_label(new, &scene_key);
            out.push_str(&format!("{}\n", label));
            last_scene = scene_key;
        }
        out.push_str(&body);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{parse, ParseOptions};

    fn d(text: &str) -> ScreenplayDocument {
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
    fn detects_dialogue_text_change() {
        let a = d("外 酒肆 夜\n\n老板娘：昨日，去了买卖城。");
        let b = d("外 酒肆 夜\n\n老板娘：昨日去了买卖城。");
        let diff = diff_documents(&a, &b, None, None);
        assert_eq!(diff.changed_blocks, 1, "{:?}", diff.entries);
        assert!(matches!(
            diff.entries.first(),
            Some(DiffEntry::BlockChanged { .. })
        ));
    }

    #[test]
    fn detects_added_block() {
        let a = d("外 酒肆 夜\n\n风大。");
        let b = d("外 酒肆 夜\n\n风大。\n\n剑客走进门。");
        let diff = diff_documents(&a, &b, None, None);
        assert_eq!(diff.added_blocks, 1);
    }

    #[test]
    fn detects_removed_block() {
        let a = d("外 酒肆 夜\n\n风大。\n\n剑客走进门。");
        let b = d("外 酒肆 夜\n\n风大。");
        let diff = diff_documents(&a, &b, None, None);
        assert_eq!(diff.removed_blocks, 1);
    }

    #[test]
    fn detects_type_change() {
        let a = d("外 酒肆 夜\n\n风大。");
        let mut b = a.clone();
        b.blocks[1].block_type = BlockType::Note;
        b.rebuild_derived();
        let diff = diff_documents(&a, &b, None, None);
        assert!(
            diff.entries
                .iter()
                .any(|e| matches!(e, DiffEntry::BlockRetyped { .. })),
            "{:?}",
            diff.entries
        );
    }

    #[test]
    fn identical_documents_produce_empty_diff() {
        let a = d("外 酒肆 夜\n\n风大。\n\n老板娘：来了。");
        let b = a.clone();
        let diff = diff_documents(&a, &b, None, None);
        assert!(diff.is_empty(), "{:?}", diff.entries);
    }

    #[test]
    fn scene_add_and_move_detected() {
        let a = d("1 外 甲 夜\n\n风大。\n\n2 外 乙 夜\n\n风小。");
        let b = d("2 外 乙 夜\n\n风小。\n\n1 外 甲 夜\n\n风大。");
        let diff = diff_documents(&a, &b, None, None);
        assert!(
            diff.entries
                .iter()
                .any(|e| matches!(e, DiffEntry::SceneMoved { .. })),
            "{:?}",
            diff.entries
        );
    }
}
