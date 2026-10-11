//! ScreenplayCommandService —— 一切修改的唯一入口。
//!
//! 说明书第 82 条：GUI、Agent、CLI、MCP 必须共用本服务，禁止四套独立逻辑。
//! 说明书第 57 条：任何外部修改必须提供 `expectedRevision`，
//! 不匹配时返回 `REVISION_CONFLICT`，绝不能覆盖人的最新内容。

use crate::db::{Origin, Store};
use screenplay_core::ast::*;
use screenplay_core::{diff, pagination, serializer};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct CommandResult {
    #[ts(type = "number")]
    pub revision: u64,
    pub document: ScreenplayDocument,
    /// 本次命令产生的版本（如果创建了）
    pub version_id: Option<String>,
    /// 受影响的 block id
    pub affected: Vec<String>,
    /// 自由文本（右侧）—— 由 AST 重新序列化
    pub source_text: String,
    /// 分页结果 —— 与 PDF 导出共用
    pub pagination: pagination::Pagination,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "error", rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub enum CommandError {
    #[serde(rename = "REVISION_CONFLICT")]
    RevisionConflict {
        #[ts(type = "number")]
        expected: u64,
        #[ts(type = "number")]
        actual: u64,
        message: String,
    },
    #[serde(rename = "NOT_FOUND")]
    NotFound { what: String, id: String },
    #[serde(rename = "INVALID_ARGUMENT")]
    InvalidArgument { message: String },
    #[serde(rename = "LOCKED")]
    BlockLocked { id: String },
    #[serde(rename = "INTERNAL")]
    Internal { message: String },
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandError::RevisionConflict {
                expected,
                actual,
                message,
            } => write!(
                f,
                "REVISION_CONFLICT: {message} (expected rev {expected}, actual {actual})"
            ),
            CommandError::NotFound { what, id } => write!(f, "NOT_FOUND: {what} {id}"),
            CommandError::InvalidArgument { message } => write!(f, "INVALID_ARGUMENT: {message}"),
            CommandError::BlockLocked { id } => write!(f, "LOCKED: block {id} 已锁定"),
            CommandError::Internal { message } => write!(f, "INTERNAL: {message}"),
        }
    }
}

impl std::error::Error for CommandError {}

/// 撤销栈状态（供 UI 显示）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct HistoryStatus {
    pub can_undo: bool,
    pub can_redo: bool,
    pub undo_label: Option<String>,
    pub redo_label: Option<String>,
    #[ts(type = "number")]
    pub undo_depth: usize,
    #[ts(type = "number")]
    pub redo_depth: usize,
}

/// 写事务：批量修改要么全部成功要么全部回滚（说明书第 61 条）
#[derive(Clone)]
pub struct CommandService {
    pub store: std::sync::Arc<Store>,
}

impl CommandService {
    pub fn new(store: std::sync::Arc<Store>) -> Self {
        Self { store }
    }

    pub fn check_revision(&self, expected: Option<u64>) -> Result<(), CommandError> {
        if let Some(e) = expected {
            let actual = self.store.revision();
            if e != actual {
                return Err(CommandError::RevisionConflict {
                    expected: e,
                    actual,
                    message: "文档已被其他来源修改，拒绝覆盖。请重新读取后再试。".into(),
                });
            }
        }
        Ok(())
    }

    /// 构建返回值
    fn result(&self, affected: Vec<String>, version_id: Option<String>) -> CommandResult {
        let doc = self.store.snapshot();
        let source_text = serializer::to_compact_source(&doc);
        let pagination = pagination::paginate(&doc);
        CommandResult {
            revision: doc.revision,
            document: doc,
            version_id,
            affected,
            source_text,
            pagination,
        }
    }

    /// bump revision + 重建派生数据 + 重算 source range + 写入 store
    ///
    /// `recompute_ranges`：是否重算 source range。
    /// - 结构性修改（insert/delete/set_type/全文替换）需要重算 → true
    /// - 纯文本修改（replace_block）只改一个 block 的文本，跳过重算以保证
    ///   "改一句不全量重解析"（QG7）；source range 由下次 replace_source_text 兜底
    fn commit(&self, mut doc: ScreenplayDocument, recompute_ranges: bool) {
        doc.revision += 1;
        doc.updated_at = now_iso();
        let repaired=screenplay_core::typography::normalize_parentheticals(&mut doc);
        doc.rebuild_derived();
        if recompute_ranges || repaired {
            let text = serializer::to_compact_source(&doc);
            parser_fix_ranges(&mut doc, &text);
        }
        *self.store.doc.lock().unwrap() = doc;
    }

    /// 带历史记录的提交：把修改前的状态压入撤销栈
    ///
    /// `merge`：是否与上一个撤销单元合并。
    /// 右侧文本编辑（打字）应合并为一次撤销；结构性操作（插入/删除/类型切换）
    /// 不合并，每次都是独立的撤销单元。
    fn commit_with_history(
        &self,
        before: ScreenplayDocument,
        doc: ScreenplayDocument,
        origin: Origin,
        command: &str,
        target: Option<&str>,
        merge: bool,
        recompute_ranges: bool,
    ) {
        self.store
            .history
            .lock()
            .unwrap()
            .push(before, origin.as_str(), command, target, merge);
        self.commit(doc, recompute_ranges);
    }

    // -----------------------------------------------------------------
    // 撤销 / 重做
    // -----------------------------------------------------------------

    /// 是否可撤销
    pub fn can_undo(&self) -> bool {
        self.store.history.lock().unwrap().can_undo()
    }

    /// 是否可重做
    pub fn can_redo(&self) -> bool {
        self.store.history.lock().unwrap().can_redo()
    }

    /// 撤销一步。返回 None 表示无可撤销。
    ///
    /// 撤销本身也是一次修改：revision 递增、写入审计日志，
    /// 因此撤销可以被追溯（说明书要求）。
    pub fn undo(&self, origin: Origin) -> Result<CommandResult, CommandError> {
        let current = self.store.snapshot();
        let memory=current.settings.project_memory.clone();
        let popped = {
            let mut h = self.store.history.lock().unwrap();
            h.pop_undo(current)
        };
        let (entry, mut doc) = match popped {
            Some(v) => v,
            None => {
                return Err(CommandError::InvalidArgument {
                    message: "没有可撤销的操作".into(),
                })
            }
        };
        doc.settings.project_memory=memory;
        doc.revision = self.store.revision();
        self.store.log_operation(
            origin,
            "undo",
            &entry.command,
            "",
            &entry.command,
            None,
        );
        self.commit(doc, true);
        Ok(self.result(Vec::new(), None))
    }

    /// 重做一步
    pub fn redo(&self, origin: Origin) -> Result<CommandResult, CommandError> {
        let current = self.store.snapshot();
        let memory=current.settings.project_memory.clone();
        let popped = {
            let mut h = self.store.history.lock().unwrap();
            h.pop_redo(current)
        };
        let (entry, mut doc) = match popped {
            Some(v) => v,
            None => {
                return Err(CommandError::InvalidArgument {
                    message: "没有可重做的操作".into(),
                })
            }
        };
        doc.settings.project_memory=memory;
        doc.revision = self.store.revision();
        self.store.log_operation(
            origin,
            "redo",
            &entry.command,
            "",
            &entry.command,
            None,
        );
        self.commit(doc, true);
        Ok(self.result(Vec::new(), None))
    }

    /// 清空撤销栈（打开新项目时）
    pub fn clear_history(&self) {
        self.store.history.lock().unwrap().clear();
    }

    // -----------------------------------------------------------------
    // 分支
    // -----------------------------------------------------------------

    /// 当前分支名
    pub fn current_branch(&self) -> String {
        self.store.branch.lock().unwrap().clone()
    }

    /// 列出全部分支
    pub fn list_branches(&self) -> Vec<crate::db::Branch> {
        self.store.list_branches()
    }

    /// 切换到已有分支，或创建并切换到新分支
    pub fn switch_branch(&self, name: &str) -> Result<(), CommandError> {
        self.store
            .switch_branch(name)
            .map_err(|m| CommandError::InvalidArgument { message: m })
    }

    /// 撤销栈状态（供 UI 显示按钮与标签）
    pub fn history_status(&self) -> HistoryStatus {
        let h = self.store.history.lock().unwrap();
        HistoryStatus {
            can_undo: h.can_undo(),
            can_redo: h.can_redo(),
            undo_label: h.undo_label(),
            redo_label: h.redo_label(),
            undo_depth: h.undo_depth(),
            redo_depth: h.redo_depth(),
        }
    }

    // -----------------------------------------------------------------
    // 读取
    // -----------------------------------------------------------------

    pub fn get_document(&self) -> CommandResult {
        self.result(Vec::new(), self.store.current_version.lock().unwrap().clone())
    }

    pub fn get_block(&self, id: &str) -> Result<ScreenplayBlock, CommandError> {
        self.store
            .snapshot()
            .block(id)
            .cloned()
            .ok_or_else(|| CommandError::NotFound {
                what: "block".into(),
                id: id.to_string(),
            })
    }

    pub fn search(&self, query: &str, case_sensitive: bool) -> Vec<SearchHit> {
        let doc = self.store.snapshot();
        let q = if case_sensitive {
            query.to_string()
        } else {
            query.to_lowercase()
        };
        let mut hits = Vec::new();
        for (i, b) in doc.blocks.iter().enumerate() {
            let hay = if case_sensitive {
                b.text.clone()
            } else {
                b.text.to_lowercase()
            };
            if hay.contains(&q) {
                hits.push(SearchHit {
                    block_id: b.id.clone(),
                    index: i,
                    block_type: b.block_type,
                    text: b.text.clone(),
                    scene_id: b.scene_id.clone(),
                });
            }
        }
        hits
    }

    // -----------------------------------------------------------------
    // 写入命令
    // -----------------------------------------------------------------

    /// 替换 block 文本
    pub fn replace_block(
        &self,
        origin: Origin,
        block_id: &str,
        new_text: &str,
        expected_revision: Option<u64>,
    ) -> Result<CommandResult, CommandError> {
        self.check_revision(expected_revision)?;
        let before = self.store.snapshot();
        let mut doc = before.clone();
        let new_rev = doc.revision + 1;
        let b = doc
            .block_mut(block_id)
            .ok_or_else(|| CommandError::NotFound {
                what: "block".into(),
                id: block_id.to_string(),
            })?;
        if b.metadata.locked && origin == Origin::Agent {
            return Err(CommandError::BlockLocked {
                id: block_id.to_string(),
            });
        }
        let old = b.text.clone();
        if old == new_text {
            return Ok(self.result(vec![block_id.to_string()], None));
        }
        b.text = new_text.to_string();
        b.confidence = 1.0;
        b.revision = new_rev;
        self.store
            .log_operation(origin, "replace_block", block_id, &old, new_text, None);
        self.commit_with_history(before, doc, origin, "replace_block", Some(block_id), true, false);
        Ok(self.result(vec![block_id.to_string()], None))
    }

    /// 插入 block
    pub fn insert_block(
        &self,
        origin: Origin,
        after_id: Option<&str>,
        block_type: BlockType,
        text: &str,
        expected_revision: Option<u64>,
    ) -> Result<CommandResult, CommandError> {
        self.check_revision(expected_revision)?;
        let before = self.store.snapshot();
        let mut doc = before.clone();
        let mut b = ScreenplayBlock::new(block_type, text);
        b.revision = doc.revision + 1;
        let new_id = b.id.clone();
        if let Some(sid) = after_id {
            let idx = doc.block_index(sid).ok_or_else(|| CommandError::NotFound {
                what: "block".into(),
                id: sid.to_string(),
            })?;
            doc.blocks.insert(idx + 1, b);
        } else {
            doc.blocks.push(b);
        }
        self.store.log_operation(
            origin,
            "insert_block",
            &new_id,
            "",
            text,
            None,
        );
        self.commit_with_history(before, doc, origin, "insert_block", None, false, true);
        Ok(self.result(vec![new_id], None))
    }

    /// 删除 block
    pub fn delete_block(
        &self,
        origin: Origin,
        block_id: &str,
        expected_revision: Option<u64>,
    ) -> Result<CommandResult, CommandError> {
        self.check_revision(expected_revision)?;
        let before = self.store.snapshot();
        let mut doc = before.clone();
        let idx = doc.block_index(block_id).ok_or_else(|| CommandError::NotFound {
            what: "block".into(),
            id: block_id.to_string(),
        })?;
        let removed = doc.blocks.remove(idx);
        self.store
            .log_operation(origin, "delete_block", block_id, &removed.text, "", None);
        self.commit_with_history(before, doc, origin, "delete_block", Some(block_id), false, true);
        Ok(self.result(vec![block_id.to_string()], None))
    }

    /// Reorder complete scenes atomically, retaining the preamble, block IDs and contents.
    pub fn reorder_scenes(&self, origin: Origin, scene_ids: &[String], expected_revision: Option<u64>) -> Result<CommandResult, CommandError> {
        self.reorder_scenes_with_board(origin,scene_ids,expected_revision,None)
    }
    pub fn reorder_scenes_with_board(&self,origin:Origin,scene_ids:&[String],expected_revision:Option<u64>,board:Option<screenplay_core::ast::SceneBoard>)->Result<CommandResult,CommandError>{
        self.check_revision(expected_revision)?;
        let before = self.store.snapshot();
        let starts: Vec<usize> = before.blocks.iter().enumerate().filter(|(_,b)| b.block_type == BlockType::SceneHeading).map(|(i,_)|i).collect();
        let old: Vec<String> = starts.iter().map(|&i| before.blocks[i].id.clone()).collect();
        let wanted: std::collections::BTreeSet<_> = scene_ids.iter().collect();
        if scene_ids.len() != old.len() || wanted.len() != old.len() || old.iter().any(|id| !wanted.contains(id)) {
            return Err(CommandError::InvalidArgument { message: "场景顺序必须包含所有场景且不得重复".into() });
        }
        if old == scene_ids && board.as_ref().is_none_or(|b|b==&before.settings.scene_board) { return Ok(self.result(vec![], None)); }
        let mut doc = before.clone();
        let mut chunks = std::collections::BTreeMap::new();
        for (n,&start) in starts.iter().enumerate() { let end = starts.get(n+1).copied().unwrap_or(before.blocks.len()); chunks.insert(old[n].clone(), before.blocks[start..end].to_vec()); }
        doc.blocks = before.blocks[..starts.first().copied().unwrap_or(before.blocks.len())].to_vec();
        for id in scene_ids { doc.blocks.extend(chunks.remove(id).unwrap()); }
        if let Some(board)=board {doc.settings.scene_board=board;}
        // A container always occupies one uninterrupted range in screenplay order.
        let mut used=std::collections::BTreeSet::new();
        for group in &mut doc.settings.scene_board.groups {
            let candidates:std::collections::BTreeSet<_>=group.scene_ids.iter().cloned().collect();
            let mut started=false;let mut ended=false;
            group.scene_ids=scene_ids.iter().filter(|id|{
                let belongs=candidates.contains(*id)&&!used.contains(*id);
                if started&&!belongs{ended=true;}
                if belongs&&!ended{started=true;used.insert((*id).clone());true}else{false}
            }).cloned().collect();
        }
        doc.settings.scene_board.groups.retain(|g|!g.scene_ids.is_empty());
        self.store.log_operation(origin, "reorder_scenes", "document", &old.join(","), &scene_ids.join(","), None);
        let affected = doc.blocks.iter().map(|b|b.id.clone()).collect();
        self.commit_with_history(before, doc, origin, "reorder_scenes", None, false, true);
        Ok(self.result(affected, None))
    }

    /// Replace a contiguous scope in one undoable transaction; never touch other blocks.
    pub fn replace_selected_blocks(&self,origin:Origin,ids:&[String],text:&str,expected_revision:Option<u64>)->Result<CommandResult,CommandError>{
        self.check_revision(expected_revision)?;
        let before=self.store.snapshot();
        let start=ids.first().and_then(|id|before.block_index(id)).ok_or_else(||CommandError::InvalidArgument{message:"未选择段落".into()})?;
        if ids.is_empty() || start+ids.len()>before.blocks.len() || before.blocks[start..start+ids.len()].iter().map(|b|&b.id).ne(ids.iter()) {return Err(CommandError::InvalidArgument{message:"选中范围已变化，请重新选择".into()});}
        if origin==Origin::Agent {if let Some(block)=before.blocks[start..start+ids.len()].iter().find(|b|b.metadata.locked){return Err(CommandError::BlockLocked{id:block.id.clone()});}}
        let mut replacement=screenplay_core::parse(text,&screenplay_core::ParseOptions::default()).document.blocks;
        if replacement.is_empty(){return Err(CommandError::InvalidArgument{message:"修改结果为空，未应用".into()});}
        if before.blocks[start].block_type==BlockType::SceneHeading && replacement[0].block_type==BlockType::SceneHeading {replacement[0].id=ids[0].clone();}
        let mut doc=before.clone();doc.blocks.splice(start..start+ids.len(),replacement);
        let affected=ids.to_vec();
        self.commit_with_history(before,doc,origin,"replace_selected_blocks",ids.first().map(String::as_str),false,true);
        Ok(self.result(affected,None))
    }

    /// 修改 block 类型
    pub fn set_block_type(
        &self,
        origin: Origin,
        block_id: &str,
        block_type: BlockType,
        expected_revision: Option<u64>,
    ) -> Result<CommandResult, CommandError> {
        self.check_revision(expected_revision)?;
        let before = self.store.snapshot();
        let mut doc = before.clone();
        let new_rev = doc.revision + 1;
        let old = {
            let b = doc.block_mut(block_id).ok_or_else(|| CommandError::NotFound {
                what: "block".into(),
                id: block_id.to_string(),
            })?;
            let o = b.block_type.as_str().to_string();
            b.block_type = block_type;
            b.confidence = 1.0;
            b.revision = new_rev;
            o
        };
        self.store.log_operation(
            origin,
            "set_block_type",
            block_id,
            &old,
            block_type.as_str(),
            None,
        );
        // 结构性操作**不合并**：每次类型切换都是独立的撤销单元
        self.commit_with_history(before, doc, origin, "set_block_type", Some(block_id), false, true);
        Ok(self.result(vec![block_id.to_string()], None))
    }

    /// 把 `second` 块合并进 `first` 块（块首退格的专业编辑器行为）。
    ///
    /// 合并后：
    /// - 第一块保留稳定 ID，第二块被删除
    /// - 文本拼接为 first.text + second.text
    /// - 光标在界面侧由调用方恢复
    pub fn merge_blocks(
        &self,
        origin: Origin,
        first_id: &str,
        second_id: &str,
        expected_revision: Option<u64>,
    ) -> Result<CommandResult, CommandError> {
        self.check_revision(expected_revision)?;
        if first_id == second_id {
            return Err(CommandError::InvalidArgument {
                message: "不能与自身合并".into(),
            });
        }
        let before = self.store.snapshot();
        let mut doc = before.clone();
        let first_idx = doc.block_index(first_id).ok_or_else(|| CommandError::NotFound {
            what: "block".into(),
            id: first_id.to_string(),
        })?;
        let second_idx =
            doc.block_index(second_id).ok_or_else(|| CommandError::NotFound {
                what: "block".into(),
                id: second_id.to_string(),
            })?;
        if second_idx != first_idx + 1 {
            return Err(CommandError::InvalidArgument {
                message: format!(
                    "只能合并相邻块（{} 与 {} 之间不连续）",
                    first_id, second_id
                ),
            });
        }
        let merged_text = {
            let a = &doc.blocks[first_idx].text;
            let b = &doc.blocks[second_idx].text;
            format!("{a}{b}")
        };
        doc.blocks[first_idx].text = merged_text.clone();
        doc.blocks[first_idx].revision = doc.revision + 1;
        doc.blocks.remove(second_idx);
        self.store.log_operation(
            origin,
            "merge_blocks",
            first_id,
            second_id,
            &merged_text,
            None,
        );
        self.commit_with_history(
            before,
            doc,
            origin,
            "merge_blocks",
            Some(first_id),
            false,
            true,
        );
        Ok(self.result(vec![first_id.to_string()], None))
    }

    /// 移动场景
    pub fn move_scene(
        &self,
        origin: Origin,
        scene_id: &str,
        to_index: usize,
        expected_revision: Option<u64>,
    ) -> Result<CommandResult, CommandError> {
        self.check_revision(expected_revision)?;
        let before = self.store.snapshot();
        let mut doc = before.clone();
        // 找到该场景的 block 范围
        let start = doc
            .block_index(scene_id)
            .ok_or_else(|| CommandError::NotFound {
                what: "scene".into(),
                id: scene_id.to_string(),
            })?;
        let mut end = start;
        for (i, b) in doc.blocks.iter().enumerate().skip(start + 1) {
            if b.block_type == BlockType::SceneHeading {
                break;
            }
            end = i;
        }
        let scene_blocks: Vec<ScreenplayBlock> = doc.blocks[start..=end].to_vec();
        for b in &scene_blocks {
            doc.blocks.retain(|x| x.id != b.id);
        }
        // 目标位置：按场景序号重新计算
        let target_scene_starts: Vec<usize> = doc
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| b.block_type == BlockType::SceneHeading)
            .map(|(i, _)| i)
            .collect();
        let insert_at = if to_index == 0 || target_scene_starts.is_empty() {
            if target_scene_starts.is_empty() {
                0
            } else {
                target_scene_starts[0]
            }
        } else if to_index - 1 >= target_scene_starts.len() {
            doc.blocks.len()
        } else {
            target_scene_starts[to_index - 1]
        };
        let insert_at = insert_at.min(doc.blocks.len());
        for (k, b) in scene_blocks.into_iter().enumerate() {
            doc.blocks.insert(insert_at + k, b);
        }
        self.store.log_operation(
            origin,
            "move_scene",
            scene_id,
            &format!("index {start}"),
            &format!("index {to_index}"),
            None,
        );
        self.commit_with_history(before, doc, origin, "delete_scene", Some(&scene_id), false, true);
        Ok(self.result(vec![scene_id.to_string()], None))
    }

    /// 用新的自由文本整体替换内容（右侧编辑器 → Parser）
    pub fn replace_source_text(
        &self,
        origin: Origin,
        text: &str,
        expected_revision: Option<u64>,
    ) -> Result<CommandResult, CommandError> {
        self.check_revision(expected_revision)?;
        let before = self.store.snapshot();
        let old_doc = before.clone();
        let outcome = screenplay_core::parse(
            text,
            &screenplay_core::ParseOptions {
                title: Some(old_doc.metadata.title.clone()),
                denoise: false,
                known_characters: old_doc
                    .characters
                    .iter()
                    .map(|c| c.name.clone())
                    .collect(),
                layout_hints: Vec::new(),
            },
        );
        let mut doc = outcome.document;
        doc.id = old_doc.id.clone();
        // Keep board references and unchanged paragraphs stable when editing free text.
        let mut available:std::collections::BTreeMap<(String,String),std::collections::VecDeque<_>>=std::collections::BTreeMap::new();
        let key=|b:&ScreenplayBlock| (b.block_type.as_str().to_string(),if b.block_type==BlockType::SceneHeading{b.text.chars().filter(|c|c.is_alphanumeric()).flat_map(char::to_uppercase).collect()}else{b.text.clone()});
        for b in &old_doc.blocks {available.entry(key(b)).or_default().push_back(b);}
        for b in &mut doc.blocks {if let Some(old)=available.get_mut(&key(b)).and_then(|q|q.pop_front()){b.id=old.id.clone();b.metadata=old.metadata.clone();if b.block_type==BlockType::SceneHeading{b.text=old.text.clone();}}}
        if origin==Origin::Agent {for old in &old_doc.blocks {if old.metadata.locked && !doc.blocks.iter().any(|b|b.id==old.id&&b.text==old.text&&b.block_type==old.block_type){return Err(CommandError::BlockLocked{id:old.id.clone()});}}}
        if doc.metadata.author.is_empty() { doc.metadata.author = old_doc.metadata.author.clone(); }
        doc.settings = old_doc.settings.clone();
        if doc.title_page.title.is_empty() { doc.title_page = old_doc.title_page.clone(); }
        let parsed_bible = doc.story_bible.clone();
        doc.story_bible = old_doc.story_bible.clone();
        doc.story_bible.character_bios.extend(parsed_bible.character_bios);
        if !parsed_bible.era.is_empty() { doc.story_bible.era = parsed_bible.era; }
        if !parsed_bible.world.is_empty() { doc.story_bible.world = parsed_bible.world; }
        doc.project_rules = old_doc.project_rules.clone();
        doc.created_at = old_doc.created_at.clone();
        doc.revision = old_doc.revision;
        self.store.log_operation(
            origin,
            "replace_source_text",
            "document",
            &format!("{} blocks", old_doc.blocks.len()),
            &format!("{} blocks", doc.blocks.len()),
            None,
        );
        // affected 必须在 commit 之前算好：commit 会把 doc 移入 store
        let affected: Vec<String> = doc.blocks.iter().map(|b| b.id.clone()).collect();
        self.commit_with_history(before, doc, origin, "replace_source_text", Some("document"), true, true);
        Ok(self.result(affected, None))
    }

    /// 导入整份文档（PDF/DOCX/...）
    /// 就地修改文档（设置、标题页等非 block 级修改）
    ///
    /// 与 block 级命令一样：走 check_revision → commit → 审计日志。
    /// Auxiliary project memory has its own history and must not consume writing undo.
    pub fn update_project_memory(&self, project_id:&str, expected:Option<u64>, memory:screenplay_core::ast::ProjectMemory)->Result<CommandResult,CommandError> {
        {
            let mut doc=self.store.doc.lock().unwrap();
            if doc.id!=project_id{return Err(CommandError::InvalidArgument{message:"项目已切换".into()});}
            if let Some(expected)=expected {if doc.revision!=expected{return Err(CommandError::RevisionConflict{expected,actual:doc.revision,message:"剧本已变化，记忆未应用".into()});}}
            doc.settings.project_memory=memory;
            doc.revision+=1;doc.updated_at=now_iso();
        }
        Ok(self.result(Vec::new(),None))
    }

    pub fn mutate_document<F>(
        &self,
        origin: Origin,
        f: F,
    ) -> Result<CommandResult, CommandError>
    where
        F: FnOnce(&mut ScreenplayDocument),
    {
        self.check_revision(None)?;
        let before = self.store.snapshot();
        let mut doc = before.clone();
        f(&mut doc);
        self.store
            .log_operation(origin, "mutate_document", "document", "", &doc.metadata.title, None);
        // affected 必须在 commit 之前算好：commit 会把 doc 移入 store
        let affected: Vec<String> = doc.blocks.iter().map(|b| b.id.clone()).collect();
        self.commit_with_history(before, doc, origin, "mutate_document", None, false, true);
        Ok(self.result(affected, None))
    }

    pub fn load_document(
        &self,
        origin: Origin,
        mut doc: ScreenplayDocument,
    ) -> Result<CommandResult, CommandError> {
        self.check_revision(None)?;
        doc.revision = self.store.revision();
        self.store
            .log_operation(origin, "load_document", "document", "", &doc.metadata.title, None);
        // 同上：affected 先算，再 commit
        let affected: Vec<String> = doc.blocks.iter().map(|b| b.id.clone()).collect();
        self.commit(doc, true);
        Ok(self.result(affected, None))
    }

    /// 更新设置
    pub fn update_settings(
        &self,
        origin: Origin,
        settings: ScreenplaySettings,
        expected_revision: Option<u64>,
    ) -> Result<CommandResult, CommandError> {
        self.check_revision(expected_revision)?;
        let before = self.store.snapshot();
        let mut doc = before.clone();
        let old_tpl = doc.settings.paper.template.clone();
        doc.settings = settings;
        self.store.log_operation(
            origin,
            "update_settings",
            "settings",
            &old_tpl,
            &doc.settings.paper.template,
            None,
        );
        self.commit_with_history(before, doc, origin, "update_settings", None, false, true);
        Ok(self.result(Vec::new(), None))
    }

    /// 更新标题页
    pub fn update_title_page(
        &self,
        origin: Origin,
        title_page: TitlePage,
        expected_revision: Option<u64>,
    ) -> Result<CommandResult, CommandError> {
        self.check_revision(expected_revision)?;
        let before = self.store.snapshot();
        let mut doc = before.clone();
        let old = doc.title_page.title.clone();
        doc.metadata.title = title_page.title.clone();
        doc.title_page = title_page;
        self.store.log_operation(
            origin,
            "update_title_page",
            "title_page",
            &old,
            &doc.title_page.title,
            None,
        );
        self.commit_with_history(before, doc, origin, "update_title_page", None, false, true);
        Ok(self.result(Vec::new(), None))
    }

    /// 创建命名版本
    pub fn create_version(
        &self,
        origin: Origin,
        name: &str,
        note: &str,
    ) -> Result<CommandResult, CommandError> {
        let v = self
            .store
            .create_version(name, note, origin, "create_version", false)
            .map_err(|e| CommandError::Internal { message: e })?;
        self.store.log_operation(
            origin,
            "create_version",
            &v.id,
            "",
            name,
            Some(&v.id),
        );
        Ok(self.result(Vec::new(), Some(v.id)))
    }

    /// 自动检查点
    pub fn auto_checkpoint(&self, trigger: &str) -> Option<String> {
        self.store
            .create_version("自动检查点", trigger, Origin::System, trigger, true)
            .ok()
            .map(|v| v.id)
    }

    /// 比较两个版本
    pub fn compare_versions(
        &self,
        a: &str,
        b: &str,
    ) -> Result<diff::SemanticDiff, CommandError> {
        let da = self
            .store
            .version_document(a)
            .ok_or_else(|| CommandError::NotFound {
                what: "version".into(),
                id: a.to_string(),
            })?;
        let db = self
            .store
            .version_document(b)
            .ok_or_else(|| CommandError::NotFound {
                what: "version".into(),
                id: b.to_string(),
            })?;
        Ok(diff::diff_documents(&da, &db, Some(a.into()), Some(b.into())))
    }

    /// 恢复旧版本 —— 恢复本身可撤销（说明书第 62 条）
    pub fn restore_version(
        &self,
        origin: Origin,
        version_id: &str,
    ) -> Result<CommandResult, CommandError> {
        let target = self
            .store
            .version_document(version_id)
            .ok_or_else(|| CommandError::NotFound {
                what: "version".into(),
                id: version_id.to_string(),
            })?;

        // 1. 先把当前状态存为自动版本
        let checkpoint = self
            .store
            .create_version(
                "恢复前自动检查点",
                &format!("恢复到 {} 之前的状态", version_id),
                origin,
                "restore_version",
                true,
            )
            .map_err(|e| CommandError::Internal { message: e })?;

        // 2. 把旧内容作为新的当前版本
        //    记录撤销点：恢复本身也必须可撤销（说明书 B-11）
        let before = self.store.snapshot();
        let mut doc = target;
        doc.revision = self.store.revision();
        self.store.log_operation(
            origin,
            "restore_version",
            version_id,
            "current",
            &doc.metadata.title,
            Some(&checkpoint.id),
        );
        self.commit_with_history(before, doc, origin, "restore_version", None, false, true);
        Ok(self.result(Vec::new(), Some(checkpoint.id)))
    }

    /// 创建分支
    pub fn create_branch(
        &self,
        origin: Origin,
        name: &str,
        from_version: Option<&str>,
    ) -> Result<CommandResult, CommandError> {
        self.store
            .create_branch(name, from_version)
            .map_err(|e| CommandError::InvalidArgument { message: e })?;
        self.store
            .log_operation(origin, "create_branch", name, "", from_version.unwrap_or(""), None);
        Ok(self.result(Vec::new(), None))
    }

    pub fn list_versions(&self) -> Vec<crate::db::VersionRecord> {
        self.store.list_versions()
    }

    pub fn audit_log(&self, limit: usize) -> Vec<crate::db::AuditEntry> {
        self.store.audit_log(limit)
    }

    /// 批量事务：Agent 用。全部成功才提交，失败整体回滚。
    pub fn transaction<F>(
        &self,
        origin: Origin,
        label: &str,
        ops: Vec<PendingOp>,
        expected_revision: Option<u64>,
        f: F,
    ) -> Result<CommandResult, CommandError>
    where
        F: FnOnce(&mut ScreenplayDocument) -> Result<(), String>,
    {
        self.check_revision(expected_revision)?;
        let backup = self.store.snapshot();

        // 在副本上执行
        let mut work = backup.clone();
        match f(&mut work) {
            Ok(()) => {}
            Err(e) => {
                // 回滚：什么都不做，store 未被修改
                self.store
                    .log_operation(origin, "transaction_rollback", label, "", &e, None);
                return Err(CommandError::InvalidArgument { message: e });
            }
        }

        // Agent transactions must obey the same lock policy as individual edits.
        if origin == Origin::Agent {
            for op in &ops {
                let targets: Vec<&str> = match op {
                    PendingOp::ReplaceBlock { block_id, .. }
                    | PendingOp::DeleteBlock { block_id }
                    | PendingOp::SetBlockType { block_id, .. } => vec![block_id],
                    PendingOp::MergeBlocks { first_id, second_id } => vec![first_id, second_id],
                    PendingOp::InsertBlock { .. } => Vec::new(),
                };
                for id in targets {
                    if backup.block(id).is_some_and(|block| block.metadata.locked) {
                        return Err(CommandError::BlockLocked { id: id.into() });
                    }
                }
            }
            // A custom transaction callback must not mutate or remove locked blocks either.
            for block in backup.blocks.iter().filter(|block| block.metadata.locked) {
                if work.block(&block.id) != Some(block) {
                    return Err(CommandError::BlockLocked { id: block.id.clone() });
                }
            }
        }

        // 应用 PendingOp
        for op in &ops {
            if let Err(e) = apply_pending(&mut work, op) {
                let msg = e.to_string();
                self.store
                    .log_operation(origin, "transaction_rollback", label, "", &msg, None);
                return Err(e);
            }
        }

        work.revision = backup.revision;
        self.store
            .log_operation(origin, "transaction", label, "", &format!("{} ops", ops.len()), None);
        let affected: Vec<String> = ops
            .iter()
            .filter_map(|o| match o {
                PendingOp::ReplaceBlock { block_id, .. } => Some(block_id.clone()),
                PendingOp::DeleteBlock { block_id } => Some(block_id.clone()),
                PendingOp::SetBlockType { block_id, .. } => Some(block_id.clone()),
                PendingOp::InsertBlock { block_id, .. } => Some(block_id.clone()),
                PendingOp::MergeBlocks { first_id, .. } => Some(first_id.clone()),
            })
            .collect();
        let checkpoint = if origin == Origin::Agent {
            Some(self.store.create_version("Agent 修改前", label, origin, "agent_checkpoint", true)
                .map_err(|message| CommandError::Internal { message })?.id)
        } else {
            None
        };
        self.commit_with_history(backup, work, origin, label, None, false, true);
        Ok(self.result(affected, checkpoint))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PendingOp {
    ReplaceBlock {
        block_id: String,
        text: String,
    },
    InsertBlock {
        block_id: String,
        after_id: Option<String>,
        block_type: BlockType,
        text: String,
    },
    DeleteBlock {
        block_id: String,
    },
    SetBlockType {
        block_id: String,
        block_type: BlockType,
    },
    MergeBlocks {
        first_id: String,
        second_id: String,
    },
}

fn apply_pending(doc: &mut ScreenplayDocument, op: &PendingOp) -> Result<(), CommandError> {
    match op {
        PendingOp::ReplaceBlock { block_id, text } => {
            let b = doc.block_mut(block_id).ok_or_else(|| CommandError::NotFound {
                what: "block".into(),
                id: block_id.clone(),
            })?;
            b.text = text.clone();
            b.confidence = 1.0;
        }
        PendingOp::DeleteBlock { block_id } => {
            doc.blocks.retain(|b| &b.id != block_id);
        }
        PendingOp::SetBlockType { block_id, block_type } => {
            let b = doc.block_mut(block_id).ok_or_else(|| CommandError::NotFound {
                what: "block".into(),
                id: block_id.clone(),
            })?;
            b.block_type = *block_type;
            b.confidence = 1.0;
        }
        PendingOp::InsertBlock {
            block_id,
            after_id,
            block_type,
            text,
        } => {
            let mut b = ScreenplayBlock::new(*block_type, text.clone());
            b.id = block_id.clone();
            match after_id {
                Some(a) => {
                    let idx = doc.block_index(a).ok_or_else(|| CommandError::NotFound {
                        what: "block".into(),
                        id: a.clone(),
                    })?;
                    doc.blocks.insert(idx + 1, b);
                }
                None => doc.blocks.push(b),
            }
        }
        PendingOp::MergeBlocks { first_id, second_id } => {
            if first_id == second_id {
                return Err(CommandError::InvalidArgument {
                    message: "不能与自身合并".into(),
                });
            }
            let first_idx = doc.block_index(first_id).ok_or_else(|| CommandError::NotFound {
                what: "block".into(),
                id: first_id.clone(),
            })?;
            let second_idx = doc.block_index(second_id).ok_or_else(|| CommandError::NotFound {
                what: "block".into(),
                id: second_id.clone(),
            })?;
            if second_idx != first_idx + 1 {
                return Err(CommandError::InvalidArgument {
                    message: format!("只能合并相邻块（{first_id} 与 {second_id} 不相邻）"),
                });
            }
            let merged = format!("{}{}", doc.blocks[first_idx].text, doc.blocks[second_idx].text);
            doc.blocks[first_idx].text = merged;
            doc.blocks.remove(second_idx);
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub block_id: String,
    pub index: usize,
    pub block_type: BlockType,
    pub text: String,
    pub scene_id: Option<String>,
}

/// 修正 source range（复用 core 的实现）
pub(crate) fn parser_fix_ranges(doc: &mut ScreenplayDocument, text: &str) {
    screenplay_core::parser::recompute_source_ranges(doc, text);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_db, Store};
    use screenplay_core::ast::{BlockType, ScreenplayDocument};
    use std::sync::Arc;

    fn make_service() -> CommandService {
        let conn = open_db(std::path::Path::new(":memory:")).unwrap();
        let doc = ScreenplayDocument::blank("test");
        let store = Arc::new(Store::new(conn, doc));
        CommandService::new(store)
    }

    #[test]
    fn markdown_metadata_and_bios_survive_subsequent_plain_source_edits() {
        let svc = make_service();
        svc.replace_source_text(Origin::GuiRight, include_str!("../../screenplay-core/fixtures/synthetic-markdown.md"), None).unwrap();
        let snapshot = svc.get_document();
        assert_eq!(snapshot.document.metadata.author, "Test Author");
        assert!(snapshot.document.story_bible.character_bios["林"].contains("三十余岁"));
        svc.replace_source_text(Origin::GuiRight, &snapshot.source_text, Some(snapshot.revision)).unwrap();
        let doc = svc.get_document().document;
        assert_eq!(doc.metadata.author, "Test Author");
        assert!(doc.story_bible.character_bios["林"].contains("三十余岁"));
        assert_eq!(doc.scenes.len(), 11);
    }

    fn make_service_with_blocks() -> CommandService {
        let svc = make_service();
        svc.insert_block(
            Origin::Cli,
            None,
            BlockType::SceneHeading,
            "EXT. BAR - NIGHT",
            None,
        )
        .unwrap();
        svc.insert_block(
            Origin::Cli,
            None,
            BlockType::Action,
            "The wind blows.",
            None,
        )
        .unwrap();
        svc
    }

    #[test]
    fn scoped_replace_preserves_other_blocks_checks_revision_and_undo() {
        let svc=make_service_with_blocks();let before=svc.get_document().document;
        let ids=vec![before.blocks[1].id.clone()];
        let result=svc.replace_selected_blocks(Origin::Agent,&ids,"新动作",Some(before.revision)).unwrap();
        assert_eq!(result.document.blocks[0],before.blocks[0]);
        assert_eq!(result.document.blocks[1].text,"新动作");
        assert!(svc.replace_selected_blocks(Origin::Agent,&ids,"过期",Some(before.revision)).is_err());
        svc.undo(Origin::GuiLeft).unwrap();assert_eq!(svc.get_document().document.blocks,before.blocks);
        svc.mutate_document(Origin::GuiLeft,|d|d.blocks[1].metadata.locked=true).unwrap();
        assert!(matches!(svc.replace_selected_blocks(Origin::Agent,&ids,"禁止",None),Err(CommandError::BlockLocked{..})));
    }
    #[test]
    fn groups_order_and_notes_are_one_undo_unit_and_free_text_keeps_ids(){
        let svc=make_service_with_blocks();
        svc.insert_block(Origin::GuiLeft,None,BlockType::SceneHeading,"EXT. TWO - NIGHT",None).unwrap();
        svc.insert_block(Origin::GuiLeft,None,BlockType::Action,"第二场动作",None).unwrap();
        let before=svc.get_document().document;let ids:Vec<_>=before.blocks.iter().filter(|b|b.block_type==BlockType::SceneHeading).map(|b|b.id.clone()).collect();
        let mut board=before.settings.scene_board.clone();board.groups.push(screenplay_core::ast::SceneGroup{id:"g".into(),name:"第一幕".into(),scene_ids:ids.clone()});
        let reversed:Vec<_>=ids.iter().rev().cloned().collect();
        let result=svc.reorder_scenes_with_board(Origin::GuiLeft,&reversed,Some(before.revision),Some(board)).unwrap();assert_eq!(result.document.settings.scene_board.groups[0].scene_ids,reversed);
        svc.undo(Origin::GuiLeft).unwrap();assert_eq!(svc.get_document().document.blocks,before.blocks);assert!(svc.get_document().document.settings.scene_board.groups.is_empty());
        let current=svc.get_document().document;let source=serializer::to_compact_source(&current);
        let result=svc.replace_source_text(Origin::GuiRight,&source,None).unwrap();
        assert_eq!(result.document.blocks.iter().map(|b|&b.id).collect::<Vec<_>>(),current.blocks.iter().map(|b|&b.id).collect::<Vec<_>>());
    }

    #[test]
    fn t01_insert_and_replace_block() {
        let svc = make_service();
        let r = svc
            .insert_block(Origin::Cli, None, BlockType::Action, "Hello", None)
            .unwrap();
        let bid = &r.affected[0];
        assert_eq!(r.revision, 1);

        let r2 = svc
            .replace_block(Origin::Cli, bid, "World", None)
            .unwrap();
        assert_eq!(r2.revision, 2);
        assert_eq!(r2.document.blocks[0].text, "World");
    }

    #[test]
    fn t02_revision_conflict() {
        let svc = make_service_with_blocks();
        let rev = svc.store.revision();
        // 传入错误的 expected revision
        let err = svc
            .replace_block(
                Origin::Cli,
                &svc.get_document().document.blocks[0].id,
                "x",
                Some(rev + 999),
            )
            .unwrap_err();
        match err {
            CommandError::RevisionConflict { .. } => {}
            other => panic!("期望 REVISION_CONFLICT，得到 {other:?}"),
        }
    }

    #[test]
    fn t03_delete_block_not_found() {
        let svc = make_service();
        let err = svc.delete_block(Origin::Cli, "nonexistent", None).unwrap_err();
        match err {
            CommandError::NotFound { .. } => {}
            other => panic!("期望 NOT_FOUND，得到 {other:?}"),
        }
    }

    #[test]
    fn t04_version_create_and_restore() {
        let svc = make_service_with_blocks();
        let first = svc.get_document().document.blocks[0].text.clone();

        let v = svc.create_version(Origin::Cli, "v1", "first").unwrap();
        let vid = v.version_id.unwrap();

        // 修改后恢复
        svc.replace_block(
            Origin::Cli,
            &svc.get_document().document.blocks[0].id,
            "MODIFIED",
            None,
        )
        .unwrap();

        let restored = svc.restore_version(Origin::Cli, &vid).unwrap();
        assert_eq!(restored.document.blocks[0].text, first);
    }

    #[test]
    fn t05_transaction_rollback() {
        let svc = make_service_with_blocks();
        let bid = svc.get_document().document.blocks[0].id.clone();
        let before = svc.store.revision();

        let ops = vec![PendingOp::ReplaceBlock {
            block_id: bid.clone(),
            text: "should not persist".to_string(),
        }];
        let err = svc
            .transaction(
                Origin::Agent,
                "test",
                ops,
                None,
                |_doc| Err("intentional failure".to_string()),
            )
            .unwrap_err();
        match err {
            CommandError::InvalidArgument { .. } => {}
            other => panic!("期望 INVALID_ARGUMENT，得到 {other:?}"),
        }
        // revision 不变
        assert_eq!(svc.store.revision(), before);
        // 文本未变
        assert_eq!(svc.get_document().document.blocks[0].text, "EXT. BAR - NIGHT");
    }

    #[test]
    fn t06_transaction_success() {
        let svc = make_service_with_blocks();
        let bid = svc.get_document().document.blocks[0].id.clone();
        let ops = vec![PendingOp::ReplaceBlock {
            block_id: bid.clone(),
            text: "transaction ok".to_string(),
        }];
        let r = svc
            .transaction(Origin::Agent, "test", ops, None, |_doc| Ok(()))
            .unwrap();
        assert_eq!(r.document.blocks[0].text, "transaction ok");
        assert!(r.version_id.is_some(), "Agent changes must retain a checkpoint");
        let undone = svc.undo(Origin::Cli).unwrap();
        assert_eq!(undone.document.blocks[0].text, "EXT. BAR - NIGHT");
        let redone = svc.redo(Origin::Cli).unwrap();
        assert_eq!(redone.document.blocks[0].text, "transaction ok");
    }

    #[test]
    fn agent_transactions_reject_locked_targets_without_partial_changes() {
        let svc = make_service_with_blocks();
        let snapshot = svc.get_document().document;
        let first = snapshot.blocks[0].id.clone();
        let second = snapshot.blocks[1].id.clone();
        svc.store.doc.lock().unwrap().blocks[1].metadata.locked = true;
        let before = svc.get_document().document;
        for blocked in [
            PendingOp::ReplaceBlock { block_id: second.clone(), text: "changed".into() },
            PendingOp::DeleteBlock { block_id: second.clone() },
            PendingOp::SetBlockType { block_id: second.clone(), block_type: BlockType::Action },
            PendingOp::MergeBlocks { first_id: first.clone(), second_id: second.clone() },
        ] {
            let error = svc.transaction(Origin::Agent, "locked", vec![
                PendingOp::ReplaceBlock { block_id: first.clone(), text: "partial".into() }, blocked,
            ], Some(before.revision), |_| Ok(())).unwrap_err();
            assert!(matches!(error, CommandError::BlockLocked { .. }));
            assert_eq!(svc.get_document().document, before);
        }
    }

    #[test]
    fn t07_block_locked_rejects_agent() {
        let svc = make_service_with_blocks();
        let bid = svc.get_document().document.blocks[0].id.clone();
        // 手动锁定
        {
            let mut doc = svc.store.doc.lock().unwrap();
            doc.block_mut(&bid).unwrap().metadata.locked = true;
        }
        let err = svc
            .replace_block(Origin::Agent, &bid, "hacked", None)
            .unwrap_err();
        match err {
            CommandError::BlockLocked { .. } => {}
            other => panic!("期望 BLOCK_LOCKED，得到 {other:?}"),
        }
        // GUI 仍可修改
        svc.replace_block(Origin::GuiLeft, &bid, "gui ok", None)
            .unwrap();
    }

    #[test]
    fn t08_search() {
        let svc = make_service_with_blocks();
        svc.insert_block(Origin::Cli, None, BlockType::Action, "wind is cold", None)
            .unwrap();
        let hits = svc.search("wind", false);
        assert!(hits.len() >= 1);
    }

    #[test]
    fn t09_compare_versions() {
        let svc = make_service_with_blocks();
        let v1 = svc.create_version(Origin::Cli, "v1", "").unwrap().version_id.unwrap();
        svc.replace_block(
            Origin::Cli,
            &svc.get_document().document.blocks[0].id,
            "CHANGED",
            None,
        )
        .unwrap();
        let v2 = svc.create_version(Origin::Cli, "v2", "").unwrap().version_id.unwrap();
        let diff = svc.compare_versions(&v1, &v2).unwrap();
        assert!(diff.entries.len() >= 1);
    }

    #[test]
    fn t10_audit_log_records_operations() {
        let svc = make_service_with_blocks();
        svc.insert_block(Origin::Cli, None, BlockType::Action, "audited", None)
            .unwrap();
        let log = svc.audit_log(10);
        assert!(!log.is_empty());
        assert!(log.iter().any(|e| e.command == "insert_block"));
    }

    #[test]
    fn t11_no_op_replace_keeps_revision() {
        let svc = make_service_with_blocks();
        let bid = svc.get_document().document.blocks[0].id.clone();
        let text = svc.get_document().document.blocks[0].text.clone();
        let rev = svc.store.revision();
        let r = svc.replace_block(Origin::Cli, &bid, &text, None).unwrap();
        assert_eq!(r.revision, rev, "相同文本不应增加 revision");
    }

    #[test]
    fn t12_merge_blocks_concatenates_and_keeps_first_id() {
        let svc = make_service_with_blocks();
        let before = svc.get_document().document;
        let first = &before.blocks[0];
        let second = &before.blocks[1];
        let (first_id, second_id) = (first.id.clone(), second.id.clone());
        let want = format!("{}{}", first.text, second.text);

        let r = svc
            .merge_blocks(Origin::GuiLeft, &first_id, &second_id, None)
            .unwrap();

        assert_eq!(r.document.blocks.len(), before.blocks.len() - 1);
        let merged = r.document.blocks.iter().find(|b| b.id == first_id).unwrap();
        assert_eq!(merged.text, want, "合并后应拼接两段文本");
        assert!(
            !r.document.blocks.iter().any(|b| b.id == second_id),
            "被合并的块应被删除"
        );
    }

    #[test]
    fn t13_merge_blocks_rejects_non_adjacent() {
        let svc = make_service();
        let a = svc
            .insert_block(Origin::Cli, None, BlockType::Action, "A", None)
            .unwrap()
            .affected[0]
            .clone();
        let b = svc
            .insert_block(Origin::Cli, Some(&a), BlockType::Action, "B", None)
            .unwrap()
            .affected[0]
            .clone();
        let c = svc
            .insert_block(Origin::Cli, Some(&b), BlockType::Action, "C", None)
            .unwrap()
            .affected[0]
            .clone();
        assert_ne!(b, c);
        // 顺序为 A, B, C —— A 与 C 不相邻
        let err = svc.merge_blocks(Origin::GuiLeft, &a, &c, None).unwrap_err();
        match err {
            CommandError::InvalidArgument { .. } => {}
            other => panic!("期望 INVALID_ARGUMENT，得到 {other:?}"),
        }
    }

    #[test]
    fn t13b_merge_blocks_rejects_self() {
        let svc = make_service();
        let a = svc
            .insert_block(Origin::Cli, None, BlockType::Action, "A", None)
            .unwrap()
            .affected[0]
            .clone();
        let err = svc.merge_blocks(Origin::GuiLeft, &a, &a, None).unwrap_err();
        match err {
            CommandError::InvalidArgument { .. } => {}
            other => panic!("期望 INVALID_ARGUMENT，得到 {other:?}"),
        }
    }

    #[test]
    fn t14_merge_blocks_is_undoable() {
        let svc = make_service_with_blocks();
        let before = svc.get_document().document;
        let (first_id, second_id) = (before.blocks[0].id.clone(), before.blocks[1].id.clone());
        let len_before = before.blocks.len();

        svc.merge_blocks(Origin::GuiLeft, &first_id, &second_id, None)
            .unwrap();
        assert_eq!(svc.get_document().document.blocks.len(), len_before - 1);

        svc.undo(Origin::GuiLeft).unwrap();
        let after = svc.get_document().document;
        assert_eq!(after.blocks.len(), len_before, "撤销合并应恢复块数");
        assert!(
            after.blocks.iter().any(|b| b.id == second_id),
            "撤销后被合并的块应回来"
        );
    }

    #[test]
    fn scenes_move_as_whole_keep_ids_preamble_and_undo() {
        let svc = make_service();
        for (t,text) in [(BlockType::General,"前言"),(BlockType::SceneHeading,"INT. A - DAY"),(BlockType::Action,"A动作"),(BlockType::Character,"甲"),(BlockType::Dialogue,"A对白"),(BlockType::SceneHeading,"EXT. B - NIGHT"),(BlockType::Action,"B动作"),(BlockType::SceneHeading,"INT. C - DAY"),(BlockType::Action,"C动作")] { svc.insert_block(Origin::GuiLeft,None,t,text,None).unwrap(); }
        let before = svc.get_document().document;
        let ids: Vec<_> = before.blocks.iter().filter(|b|b.block_type==BlockType::SceneHeading).map(|b|b.id.clone()).collect();
        let order = vec![ids[2].clone(),ids[0].clone(),ids[1].clone()];
        let r = svc.reorder_scenes(Origin::GuiLeft,&order,Some(before.revision)).unwrap();
        assert_eq!(r.document.blocks.iter().map(|b|b.text.as_str()).collect::<Vec<_>>(),vec!["前言","INT. C - DAY","C动作","INT. A - DAY","A动作","甲","A对白","EXT. B - NIGHT","B动作"]);
        for b in &before.blocks { assert!(r.document.blocks.iter().any(|a|a.id==b.id && a.text==b.text && a.block_type==b.block_type)); }
        assert_eq!(r.document.scenes.len(),3);
        assert!(r.source_text.find("C动作").unwrap()<r.source_text.find("A动作").unwrap());
        assert!(svc.reorder_scenes(Origin::GuiLeft,&ids,Some(before.revision)).is_err());
        let revision = r.revision;
        assert!(svc.reorder_scenes(Origin::GuiLeft,&[ids[0].clone(),ids[0].clone(),ids[1].clone()],Some(revision)).is_err());
        assert_eq!(svc.get_document().revision,revision);
        svc.undo(Origin::GuiLeft).unwrap();
        assert_eq!(svc.get_document().document.blocks.iter().map(|b|&b.id).collect::<Vec<_>>(),before.blocks.iter().map(|b|&b.id).collect::<Vec<_>>());
        svc.redo(Origin::GuiLeft).unwrap();
        assert_eq!(svc.get_document().document.blocks[1].id,ids[2]);
    }

    #[test]
    fn t15_set_block_type_keeps_id_and_is_undoable() {
        let svc = make_service_with_blocks();
        let bid = svc.get_document().document.blocks[0].id.clone();
        svc.set_block_type(Origin::GuiLeft, &bid, BlockType::Shot, None)
            .unwrap();
        let doc = svc.get_document().document;
        let b = doc.blocks.iter().find(|b| b.id == bid).unwrap();
        assert_eq!(b.block_type, BlockType::Shot);
        assert_eq!(b.id, bid, "类型切换必须保留稳定 ID");

        svc.undo(Origin::GuiLeft).unwrap();
        let back = svc.get_document().document;
        let b2 = back.blocks.iter().find(|b| b.id == bid).unwrap();
        assert_eq!(b2.block_type, BlockType::SceneHeading, "撤销应恢复原类型");
    }
}


#[cfg(test)]
mod history_tests {
    use super::*;
    use crate::db::{open_db, Store};

    fn svc() -> CommandService {
        let conn = open_db(std::path::Path::new(":memory:")).unwrap();
        let store = std::sync::Arc::new(Store::new(
            conn,
            ScreenplayDocument::blank("t"),
        ));
        CommandService::new(store)
    }

    /// seed 两个块。insert_block 不参与合并，两次插入是独立撤销单元。
    fn seed(s: &CommandService) -> Vec<String> {
        let r = s
            .insert_block(Origin::GuiLeft, None, BlockType::Action, "第一段", None)
            .unwrap();
        let r2 = s
            .insert_block(Origin::GuiLeft, Some(r.affected[0].as_str()), BlockType::Action, "第二段", None)
            .unwrap();
        r2.document.blocks.iter().map(|b| b.id.clone()).collect()
    }

    #[test]
    fn h01_replace_block_then_undo_restores_text() {
        let s = svc();
        let ids = seed(&s);
        s.replace_block(Origin::GuiLeft, &ids[0], "改过的第一段", None).unwrap();
        assert_eq!(s.get_document().document.blocks[0].text, "改过的第一段");
        assert!(s.can_undo());

        s.undo(Origin::GuiLeft).unwrap();
        assert_eq!(s.get_document().document.blocks[0].text, "第一段");
    }

    #[test]
    fn h02_undo_redo_roundtrip() {
        let s = svc();
        let ids = seed(&s);
        s.replace_block(Origin::GuiLeft, &ids[0], "X", None).unwrap();
        s.undo(Origin::GuiLeft).unwrap();
        assert!(s.can_redo());
        s.redo(Origin::GuiLeft).unwrap();
        assert_eq!(s.get_document().document.blocks[0].text, "X");
    }

    #[test]
    fn h03_insert_then_undo_removes_block() {
        let s = svc();
        let ids = seed(&s);
        let before = s.get_document().document.blocks.len();
        let r = s
            .insert_block(Origin::GuiLeft, Some(ids[1].as_str()), BlockType::Dialogue, "对白", None)
            .unwrap();
        assert_eq!(s.get_document().document.blocks.len(), before + 1);

        s.undo(Origin::GuiLeft).unwrap();
        let after = s.get_document().document;
        assert_eq!(after.blocks.len(), before, "撤销后应恢复块数");
        assert!(after.block(&r.affected[0]).is_none(), "新插入的块应消失");
    }

    #[test]
    fn h04_delete_then_undo_restores_block() {
        let s = svc();
        let ids = seed(&s);
        s.delete_block(Origin::GuiLeft, &ids[1], None).unwrap();
        assert_eq!(s.get_document().document.blocks.len(), 1);
        s.undo(Origin::GuiLeft).unwrap();
        let doc = s.get_document().document;
        assert_eq!(doc.blocks.len(), 2);
        assert_eq!(doc.blocks[1].text, "第二段");
    }

    /// 连续输入应合并为一个撤销单元（打字体验：⌘Z 一次回到本次输入前）
    #[test]
    fn h05_consecutive_text_edits_merge_into_one_undo() {
        let s = svc();
        s.replace_source_text(Origin::GuiRight, "外 城门 黄昏\n\n甲。\n", None).unwrap();
        s.replace_source_text(Origin::GuiRight, "外 城门 黄昏\n\n甲乙。\n", None).unwrap();
        s.replace_source_text(Origin::GuiRight, "外 城门 黄昏\n\n甲乙丙。\n", None).unwrap();
        assert!(s.get_document().source_text.contains("丙"));

        // 一次撤销应回到最初（合并为一个单元）
        s.undo(Origin::GuiRight).unwrap();
        assert!(
            s.get_document().document.blocks.is_empty(),
            "连续输入应一次性撤销到最初状态"
        );
    }

    #[test]
    fn h06_agent_change_can_be_undone_separately() {
        let s = svc();
        let ids = seed(&s);
        s.replace_block(Origin::GuiLeft, &ids[0], "人工修改", None).unwrap();
        s.replace_block(Origin::Agent, &ids[1], "AI 修改", None).unwrap();

        s.undo(Origin::GuiLeft).unwrap();
        let doc = s.get_document().document;
        assert_eq!(doc.blocks[1].text, "第二段", "AI 修改应被撤销");
        assert_eq!(doc.blocks[0].text, "人工修改", "人工修改应保留");
    }

    #[test]
    fn h07_undo_without_history_returns_error() {
        let s = svc();
        let r = s.undo(Origin::GuiLeft);
        assert!(r.is_err(), "空栈撤销应返回错误而非 panic");
    }

    #[test]
    fn h08_redo_without_undo_returns_error() {
        let s = svc();
        let r = s.redo(Origin::GuiLeft);
        assert!(r.is_err());
    }

    #[test]
    fn h09_undo_increments_revision_and_writes_audit() {
        let s = svc();
        let ids = seed(&s);
        s.replace_block(Origin::GuiLeft, &ids[0], "改", None).unwrap();
        let rev_before = s.store.revision();

        let r = s.undo(Origin::GuiLeft).unwrap();
        assert_eq!(r.revision, rev_before + 1, "撤销本身也是一次修改");
        assert!(r.source_text.contains("第一段"), "撤销后自由文本同步回退");

        let audit = s.audit_log(50);
        assert!(
            audit.iter().any(|a| a.command == "undo"),
            "撤销必须写入审计日志"
        );
    }

    /// 连续修改同一块合并为一个撤销单元；
    /// 但**不同块**的修改不合并，可逐步撤销。
    #[test]
    fn h10_edits_to_different_blocks_undo_separately() {
        let s = svc();
        let ids = seed(&s);
        // 改块 0
        s.replace_block(Origin::GuiLeft, &ids[0], "改了第一段", None).unwrap();
        // 改块 1（不同 block_id，不应合并）
        s.replace_block(Origin::GuiLeft, &ids[1], "改了第二段", None).unwrap();

        s.undo(Origin::GuiLeft).unwrap();
        let d1 = s.get_document().document;
        assert_eq!(d1.blocks[1].text, "第二段", "第一次撤销应恢复第二段");
        assert_eq!(d1.blocks[0].text, "改了第一段", "第一段的修改应保留");

        s.undo(Origin::GuiLeft).unwrap();
        let d2 = s.get_document().document;
        assert_eq!(d2.blocks[0].text, "第一段", "第二次撤销应恢复第一段");
    }

    #[test]
    fn h11_history_status_reports_labels() {
        let s = svc();
        let ids = seed(&s);
        s.replace_block(Origin::GuiLeft, &ids[0], "改", None).unwrap();
        let st = s.history_status();
        assert!(st.can_undo);
        assert!(!st.can_redo);
        assert!(st.undo_depth >= 1);
        assert!(st.undo_label.is_some());
    }

    #[test]
    fn h12_clear_history_disables_undo() {
        let s = svc();
        let ids = seed(&s);
        s.replace_block(Origin::GuiLeft, &ids[0], "改", None).unwrap();
        assert!(s.can_undo());
        s.clear_history();
        assert!(!s.can_undo());
        assert!(s.undo(Origin::GuiLeft).is_err());
    }

    #[test]
    fn h13_new_change_after_undo_clears_redo() {
        let s = svc();
        let ids = seed(&s);
        s.replace_block(Origin::GuiLeft, &ids[0], "A", None).unwrap();
        s.undo(Origin::GuiLeft).unwrap();
        assert!(s.can_redo());
        s.replace_block(Origin::GuiLeft, &ids[0], "B", None).unwrap();
        assert!(!s.can_redo(), "撤销后新修改必须清空 redo");
    }

    #[test]
    fn h14_settings_change_undo() {
        let s = svc();
        seed(&s);
        let new_settings = ScreenplaySettings {
            paper: screenplay_core::ast::PaperSettings {
                template: "a4".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        s.update_settings(Origin::GuiLeft, new_settings, None).unwrap();
        assert_eq!(s.get_document().document.settings.paper.template, "a4");
        s.undo(Origin::GuiLeft).unwrap();
        assert_eq!(
            s.get_document().document.settings.paper.template,
            "us-letter",
            "撤销应恢复纸张设置"
        );
    }

    #[test]
    fn h15_undo_restores_derived_data() {
        let s = svc();
        // 先建立基线（一次输入）
        s.replace_source_text(Origin::GuiRight, "外 城门 黄昏\n\n甲。\n", None).unwrap();
        let scenes_before = s.get_document().document.scenes.len();
        let chars_before = s.get_document().document.characters.len();

        // 结构性操作：删除场景（不合并，可独立撤销）
        let scene_block = s
            .get_document()
            .document
            .blocks
            .iter()
            .find(|b| b.block_type == BlockType::SceneHeading)
            .unwrap()
            .id
            .clone();
        s.delete_block(Origin::GuiLeft, &scene_block, None).unwrap();
        assert!(s.get_document().document.scenes.is_empty(), "场景应被删除");

        s.undo(Origin::GuiLeft).unwrap();
        let doc = s.get_document().document;
        assert_eq!(
            doc.scenes.len(),
            scenes_before,
            "撤销必须恢复派生数据（场景列表）"
        );
        assert_eq!(doc.characters.len(), chars_before, "人物实体也应恢复");
    }
    #[test]
    fn memory_updates_do_not_consume_writing_undo_and_reject_stale_results() {
        let s=svc();seed(&s);
        let doc=s.get_document().document;
        let mut memory=doc.settings.project_memory.clone();memory.markdown="人物设定".into();
        s.update_project_memory(&doc.id,Some(doc.revision),memory.clone()).unwrap();
        assert!(s.update_project_memory(&doc.id,Some(doc.revision),memory).is_err());
        s.undo(Origin::GuiLeft).unwrap();
        assert_eq!(s.get_document().document.settings.project_memory.markdown,"人物设定");
        s.redo(Origin::GuiLeft).unwrap();
        assert_eq!(s.get_document().document.settings.project_memory.markdown,"人物设定");
    }

}

#[cfg(test)]
mod version_tests {
    use super::*;
    use crate::db::{open_db, Store};

    fn svc() -> CommandService {
        let conn = open_db(std::path::Path::new(":memory:")).unwrap();
        CommandService::new(std::sync::Arc::new(Store::new(
            conn,
            ScreenplayDocument::blank("t"),
        )))
    }

    fn put(s: &CommandService, text: &str) {
        s.replace_source_text(Origin::GuiLeft, text, None).unwrap();
    }

    /// 说明书 B-10：恢复旧版本前保留当前内容
    #[test]
    fn v01_restore_creates_checkpoint_of_current_content() {
        let s = svc();
        put(&s, "外 A 夜\n\n初稿。\n");
        let v1 = s.create_version(Origin::GuiLeft, "v1", "第一稿").unwrap();
        put(&s, "外 A 夜\n\n第三稿。\n");

        s.restore_version(Origin::GuiLeft, &v1.version_id.unwrap())
            .unwrap();

        let versions = s.list_versions();
        assert!(versions.len() >= 2, "应有原版本 + 自动检查点");
        let checkpoint = versions
            .iter()
            .find(|v| v.name.contains("恢复前自动检查点"))
            .expect("必须有恢复前检查点");
        assert!(checkpoint.auto, "检查点应标记为 auto");
    }

    /// 说明书 B-11：恢复本身可撤销
    #[test]
    fn v02_restore_is_undoable() {
        let s = svc();
        put(&s, "外 A 夜\n\n初稿。\n");
        let v1 = s.create_version(Origin::GuiLeft, "v1", "第一稿").unwrap();
        put(&s, "外 A 夜\n\n第三稿。\n");
        assert!(s.get_document().source_text.contains("第三稿"));

        // 恢复到 v1
        s.restore_version(Origin::GuiLeft, &v1.version_id.unwrap())
            .unwrap();
        assert!(
            s.get_document().source_text.contains("初稿"),
            "恢复后应是初稿内容"
        );

        // 撤销恢复 → 回到第三稿
        assert!(s.can_undo(), "恢复后应可撤销");
        s.undo(Origin::GuiLeft).unwrap();
        assert!(
            s.get_document().source_text.contains("第三稿"),
            "撤销恢复后应回到第三稿: {}",
            s.get_document().source_text
        );
    }

    #[test]
    fn v03_restore_unknown_version_returns_error() {
        let s = svc();
        let r = s.restore_version(Origin::GuiLeft, "v-nonexistent");
        assert!(r.is_err());
        assert!(matches!(r, Err(CommandError::NotFound { .. })));
    }

    #[test]
    fn v04_version_compare_reports_changes() {
        let s = svc();
        put(&s, "外 A 夜\n\n甲。\n");
        let v1 = s.create_version(Origin::GuiLeft, "v1", "").unwrap();
        put(&s, "外 A 夜\n\n乙。\n丙。\n");
        let v2 = s.create_version(Origin::GuiLeft, "v2", "").unwrap();

        let d = s
            .compare_versions(
                v1.version_id.as_ref().unwrap(),
                v2.version_id.as_ref().unwrap(),
            )
            .unwrap();
        assert!(!d.entries.is_empty(), "应检测到差异");
        assert!(d.changed_blocks > 0 || d.added_blocks > 0);
    }

    #[test]
    fn v05_create_version_with_duplicate_name_allowed() {
        let s = svc();
        put(&s, "外 A 夜\n\n甲。\n");
        s.create_version(Origin::GuiLeft, "同名", "").unwrap();
        // 允许重名（时间戳区分），不应报错
        s.create_version(Origin::GuiLeft, "同名", "").unwrap();
        assert_eq!(s.list_versions().len(), 2);
    }

    #[test]
    fn v06_audit_log_records_restore() {
        let s = svc();
        put(&s, "外 A 夜\n\n甲。\n");
        let v1 = s.create_version(Origin::GuiLeft, "v1", "").unwrap();
        s.restore_version(Origin::GuiLeft, &v1.version_id.unwrap())
            .unwrap();
        let audit = s.audit_log(100);
        assert!(
            audit.iter().any(|a| a.command == "restore_version"),
            "恢复必须写入审计日志"
        );
    }
}

#[cfg(test)]
mod perf_tests {
    use super::*;
    use crate::db::{open_db, Store};
    use screenplay_core::ast::{BlockType, ScreenplayBlock};

    /// 生成一个约 300 页的大文档（直接构造 AST，避免逐个 insert 的开销）
    fn make_large_svc() -> (CommandService, String) {
        let conn = open_db(std::path::Path::new(":memory:")).unwrap();
        // 直接构造大文档再放入 Store
        let mut doc = ScreenplayDocument::blank("perf");
        for p in 0..300 {
            doc.blocks.push(ScreenplayBlock::new(
                BlockType::SceneHeading,
                format!("外 场景{} 夜", p + 1),
            ));
            for _ in 0..20 {
                doc.blocks.push(ScreenplayBlock::new(
                    BlockType::Action,
                    "剑客缓步走入酒肆，目光扫过四周。".to_string(),
                ));
            }
        }
        let first_id = doc.blocks[0].id.clone();
        let svc = CommandService::new(std::sync::Arc::new(Store::new(conn, doc)));
        (svc, first_id)
    }

    /// QG7: 改一句不全量重解析
    /// replace_block 直接修改 AST 中一个 block，不重新 parse 全文。
    /// 在 300 页文档上应 < 50ms。
    #[test]
    fn qg7_single_block_edit_is_incremental() {
        let (s, first_id) = make_large_svc();
        let total_blocks = s.get_document().document.blocks.len();

        let start = std::time::Instant::now();
        let r = s
            .replace_block(Origin::GuiLeft, &first_id, "修改后的场景标题", None)
            .unwrap();
        let elapsed = start.elapsed();

        eprintln!(
            "[QG7] 300页文档单 block 编辑耗时: {:?} (共 {} blocks)",
            elapsed, total_blocks
        );

        // 单 block 编辑应远快于全量重解析（< 50ms）
        assert!(
            elapsed < std::time::Duration::from_millis(50),
            "单 block 编辑耗时 {:?} 超过 50ms",
            elapsed
        );
        // 只影响 1 个 block
        assert_eq!(r.affected.len(), 1);
        assert_eq!(r.affected[0], first_id);
        // 其他 block 不变
        let doc = s.get_document().document;
        assert_eq!(doc.blocks.len(), total_blocks);
        assert_eq!(doc.blocks[0].text, "修改后的场景标题");
    }

    /// QG7: 300 页文档的 get_document（含分页）应在 100ms 内
    #[test]
    fn qg7_get_document_300pages_fast() {
        let (s, _) = make_large_svc();
        let start = std::time::Instant::now();
        let r = s.get_document();
        let elapsed = start.elapsed();
        eprintln!("[QG7] 300页 get_document 耗时: {:?}", elapsed);
        assert!(
            elapsed < std::time::Duration::from_millis(100),
            "get_document 耗时 {:?} 超过 100ms",
            elapsed
        );
        assert!(!r.pagination.pages.is_empty());
    }


}
