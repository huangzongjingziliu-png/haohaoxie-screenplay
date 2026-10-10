//! 撤销/重做栈 —— 说明书要求「左右共享的 Undo/Redo，覆盖结构修改和 Agent 修改」。
//!
//! 设计要点：
//! 1. **在 Rust 侧**：因为 GUI、Agent、CLI、MCP 都必须能撤销，
//!    撤销栈不能只存在于前端。
//! 2. **基于快照**：每次提交前的文档状态入栈，简单可靠。
//!    剧本文档通常在几百 KB 量级，快照成本可接受。
//! 3. **合并策略**：连续同类型的微小修改（如打字）合并为一个 undo 单元，
//!    避免撤销一次只退一个字。
//! 4. **与版本引擎配合**：撤销本身也是一次修改（revision 递增、写入审计日志），
//!    因此撤销可以被审计、可以被版本捕获。

use screenplay_core::ast::ScreenplayDocument;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// 撤销栈中的一项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// 撤销到这一项后，文档处于该状态
    pub snapshot: ScreenplayDocument,
    /// 该修改的来源（用于 UI 显示与合并判断）
    pub origin: String,
    /// 该修改的命令名
    pub command: String,
    /// 被修改的对象（block_id 或 "document"）—— 合并判据
    #[serde(default)]
    pub target: Option<String>,
    /// 时间戳
    pub at: String,
}

/// 撤销/重做栈
pub struct HistoryStack {
    /// undo 栈：栈顶是最近一次修改前的状态
    undo: VecDeque<HistoryEntry>,
    redo: VecDeque<HistoryEntry>,
    /// 最大长度（防止内存无限增长）
    capacity: usize,
}

impl Default for HistoryStack {
    fn default() -> Self {
        Self {
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            capacity: 100,
        }
    }
}

impl HistoryStack {
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一次修改（传入修改**前**的状态）
    ///
    /// `merge_into_previous`：是否与栈顶合并为一个撤销单元。
    /// 由调用方（CommandService）根据语义决定，而不是靠时间窗口猜测——
    /// 时间窗口在真实使用中不可靠（用户打字停顿会意外断开撤销链）。
    pub fn push(
        &mut self,
        before: ScreenplayDocument,
        origin: &str,
        command: &str,
        target: Option<&str>,
        merge_into_previous: bool,
    ) {
        // 新修改会清空 redo 栈（标准编辑器行为）
        self.redo.clear();

        // 合并判断：调用方要求合并，且栈顶是同一「目标」的编辑。
        // 只比较命令名是不够的——用户连续编辑块 A 再编辑块 B，
        // 必须产生两个撤销单元，否则第二次 ⌘Z 会把块 A 的改动也一起撤掉。
        if merge_into_previous {
            if let Some(top) = self.undo.back() {
                if top.command == command
                    && top.origin == origin
                    && top.target.as_deref() == target
                {
                    // 保留更早的快照（我们要回到的是最初状态）
                    return;
                }
            }
        }

        self.undo.push_back(HistoryEntry {
            snapshot: before,
            origin: origin.to_string(),
            command: command.to_string(),
            target: target.map(|t| t.to_string()),
            at: chrono::Utc::now().to_rfc3339(),
        });

        while self.undo.len() > self.capacity {
            self.undo.pop_front();
        }
    }

    /// 弹出撤销目标（并把当前状态压入 redo）
    pub fn pop_undo(&mut self, current: ScreenplayDocument) -> Option<(HistoryEntry, ScreenplayDocument)> {
        let entry = self.undo.pop_back()?;
        self.redo.push_back(HistoryEntry {
            snapshot: current.clone(),
            origin: entry.origin.clone(),
            command: entry.command.clone(),
            target: entry.target.clone(),
            at: entry.at.clone(),
        });
        while self.redo.len() > self.capacity {
            self.redo.pop_front();
        }
        let snapshot = entry.snapshot.clone();
        Some((entry, snapshot))
    }

    /// 弹出重做目标
    pub fn pop_redo(&mut self, current: ScreenplayDocument) -> Option<(HistoryEntry, ScreenplayDocument)> {
        let entry = self.redo.pop_back()?;
        self.undo.push_back(HistoryEntry {
            snapshot: current.clone(),
            origin: entry.origin.clone(),
            command: entry.command.clone(),
            target: entry.target.clone(),
            at: entry.at.clone(),
        });
        let snapshot = entry.snapshot.clone();
        Some((entry, snapshot))
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_label(&self) -> Option<String> {
        self.undo.back().map(|e| describe(&e.command, &e.origin))
    }

    pub fn redo_label(&self) -> Option<String> {
        self.redo.back().map(|e| describe(&e.command, &e.origin))
    }

    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    /// 清空（打开新项目时）
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// 导出为可序列化状态（用于崩溃恢复）
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&HistoryDump {
            undo: self.undo.iter().cloned().collect(),
            redo: self.redo.iter().cloned().collect(),
        })
    }

    /// 从序列化状态恢复
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        let dump: HistoryDump = serde_json::from_str(s)?;
        Ok(Self {
            undo: dump.undo.into_iter().collect(),
            redo: dump.redo.into_iter().collect(),
            capacity: 100,
        })
    }
}

#[derive(Serialize, Deserialize)]
struct HistoryDump {
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
}

fn describe(command: &str, origin: &str) -> String {
    let what = match command {
        "reorder_scenes" => "调整场景顺序",
        "replace_block" => "修改内容",
        "insert_block" => "插入元素",
        "delete_block" => "删除元素",
        "replace_source_text" => "编辑文本",
        "load_document" => "导入",
        "mutate_document" => "修改设置",
        "restore_version" => "恢复版本",
        "agent_apply" => "AI 修改",  // 已含 AI 标识
        _ => command,
    };
    // agent_apply 的名称里已含「AI」，其他 agent 来源加后缀避免重复
    let from = match (origin, command) {
        ("agent", "agent_apply") => "",
        ("agent", _) => "（AI）",
        _ => "",
    };
    format!("{what}{from}")
}

pub fn current_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(text: &str) -> ScreenplayDocument {
        let mut d = ScreenplayDocument::blank("t");
        d.blocks.push(screenplay_core::ast::ScreenplayBlock::new(
            screenplay_core::ast::BlockType::Action,
            text,
        ));
        d
    }

    #[test]
    fn t01_push_then_undo_restores_previous() {
        let mut h = HistoryStack::new();
        let d0 = doc("原始");
        h.push(d0.clone(), "gui-left", "replace_block", None, false);
        assert!(h.can_undo());
        assert!(!h.can_redo());

        let (_entry, restored) = h.pop_undo(doc("修改后")).unwrap();
        assert_eq!(restored.blocks[0].text, "原始");
        assert!(h.can_redo());
        assert!(!h.can_undo());
    }

    #[test]
    fn t02_redo_reapplies_change() {
        let mut h = HistoryStack::new();
        h.push(doc("原始"), "gui-left", "replace_block", None, false);
        let (_, restored) = h.pop_undo(doc("修改后")).unwrap();
        assert_eq!(restored.blocks[0].text, "原始");

        let (_e2, forward) = h.pop_redo(doc("原始")).unwrap();
        assert_eq!(forward.blocks[0].text, "修改后");
        assert!(h.can_undo());
    }

    #[test]
    fn t03_new_change_clears_redo() {
        let mut h = HistoryStack::new();
        h.push(doc("A"), "gui-left", "replace_block", None, false);
        h.pop_undo(doc("B"));
        assert!(h.can_redo());

        h.push(doc("B"), "gui-left", "replace_block", None, false);
        assert!(!h.can_redo(), "新修改必须清空 redo 栈");
    }

    #[test]
    fn t04_undo_redo_cycles_multiple_times() {
        let mut h = HistoryStack::new();
        // 语义：push 传入的是「本次修改前」的状态
        //   修改1: v0 → v1   push(v0)
        //   修改2: v1 → v2   push(v1)
        //   修改3: v2 → v3   push(v2)
        h.push(doc("v0"), "gui-left", "replace_block", None, false);
        h.push(doc("v1"), "gui-left", "replace_block", None, false);
        h.push(doc("v2"), "gui-left", "replace_block", None, false);
        assert_eq!(h.undo_depth(), 3);

        // 当前文档是 v3
        let (_, r1) = h.pop_undo(doc("v3")).unwrap();
        assert_eq!(r1.blocks[0].text, "v2");
        let (_, r2) = h.pop_undo(r1.clone()).unwrap();
        assert_eq!(r2.blocks[0].text, "v1");
        let (_, r3) = h.pop_undo(r2.clone()).unwrap();
        assert_eq!(r3.blocks[0].text, "v0");
        assert!(!h.can_undo());
        assert_eq!(h.redo_depth(), 3);

        // 重做应按相反顺序恢复
        let (_, f1) = h.pop_redo(r3.clone()).unwrap();
        assert_eq!(f1.blocks[0].text, "v1");
        let (_, f2) = h.pop_redo(f1.clone()).unwrap();
        assert_eq!(f2.blocks[0].text, "v2");
        let (_, f3) = h.pop_redo(f2.clone()).unwrap();
        assert_eq!(f3.blocks[0].text, "v3");
        assert!(!h.can_redo());
    }

    #[test]
    fn t05_rapid_same_command_merges_into_one_undo_unit() {
        let mut h = HistoryStack::new();
        // 模拟打字：3 次快速修改
        h.push(doc("a"), "gui-right", "replace_block", None, true);
        h.push(doc("ab"), "gui-right", "replace_block", None, true);
        h.push(doc("abc"), "gui-right", "replace_block", None, true);
        // 都在 merge_window_ms(600) 内 → 应该只保留最早的那个快照
        assert_eq!(h.undo_depth(), 1, "连续打字应合并为一个撤销单元");

        let (_, restored) = h.pop_undo(doc("abcd")).unwrap();
        assert_eq!(restored.blocks[0].text, "a", "应回到最初状态");
    }

    #[test]
    fn t06_slow_edits_do_not_merge() {
        let mut h = HistoryStack::new();
        h.push(doc("a"), "gui-right", "replace_block", None, false);
        // 间隔超过 merge window
        h.push(doc("ab"), "gui-right", "replace_block", None, false);
        assert_eq!(h.undo_depth(), 2, "慢速编辑不应合并");
    }

    #[test]
    fn t07_different_commands_do_not_merge() {
        let mut h = HistoryStack::new();
        h.push(doc("a"), "gui-right", "replace_block", None, false);
        h.push(doc("b"), "gui-right", "insert_block", None, false);
        assert_eq!(h.undo_depth(), 2, "不同命令不应合并");
    }

    #[test]
    fn t08_different_origins_do_not_merge() {
        let mut h = HistoryStack::new();
        h.push(doc("a"), "gui-left", "replace_block", None, false);
        h.push(doc("b"), "agent", "replace_block", None, false);
        assert_eq!(h.undo_depth(), 2, "不同来源不应合并（AI 修改需独立撤销）");
    }

    #[test]
    fn t09_capacity_limit_drops_oldest() {
        let mut h = HistoryStack::new();
        h.capacity = 3;
        for i in 0..5 {
            h.push(doc(&format!("v{i}")), "gui-left", "replace_block", None, false);
        }
        assert_eq!(h.undo_depth(), 3, "应保持容量上限");
    }

    #[test]
    fn t10_json_roundtrip_preserves_stack() {
        let mut h = HistoryStack::new();
        h.push(doc("A"), "gui-left", "replace_block", None, false);
        h.push(doc("B"), "gui-left", "replace_block", None, false);
        h.pop_undo(doc("B"));

        let json = h.to_json().unwrap();
        let restored = HistoryStack::from_json(&json).unwrap();
        assert_eq!(restored.undo_depth(), h.undo_depth());
        assert_eq!(restored.redo_depth(), h.redo_depth());
    }

    #[test]
    fn t11_clear_resets_both_stacks() {
        let mut h = HistoryStack::new();
        h.push(doc("A"), "gui-left", "replace_block", None, false);
        h.pop_undo(doc("B"));
        h.clear();
        assert!(!h.can_undo());
        assert!(!h.can_redo());
    }

    #[test]
    fn t12_labels_describe_action_and_origin() {
        let mut h = HistoryStack::new();
        h.push(doc("A"), "agent", "agent_apply", None, false);
        assert_eq!(h.undo_label().unwrap(), "AI 修改");
    }
}