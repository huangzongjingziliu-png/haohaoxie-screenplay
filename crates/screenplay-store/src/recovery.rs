//! 崩溃恢复 —— 说明书 B-09：「故意强制退出后验证恢复」。
//!
//! 设计要点：
//! 1. **自动保存到独立位置**：每次修改后（可节流）把当前文档写入
//!    `~/Library/Application Support/幕本/recovery/<session>.json`，
//!    不影响用户显式保存的 `.screenplay` 容器。
//! 2. **原子写入**：先写 `.tmp` 再 `rename`，避免崩溃时留下半截文件。
//! 3. **会话标识**：启动时生成 session id，退出时清除；
//!    如果启动时发现有上次遗留的 session，说明上次是异常退出。
//! 4. **恢复内容包含撤销栈**：这样恢复后还能撤销到崩溃前的状态。
//!
//! 数据安全底线：**任何情况下都不能让用户丢失已输入的文字**。

use screenplay_core::ast::ScreenplayDocument;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// 恢复文件内容
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryRecord {
    /// 会话标识
    pub session_id: String,
    /// 项目容器路径（None = 未保存的新项目）
    pub container: Option<String>,
    pub document: ScreenplayDocument,
    /// 撤销栈 JSON（可选，恢复后可继续撤销）
    #[serde(default)]
    pub history: Option<String>,
    /// 原始文件 sidecar 的随机标识；通过会话文件名和此 ID 构造安全路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    /// 运行时加载的源文件字节，不内嵌到恢复 JSON。
    #[serde(skip, default)]
    pub original_source: Option<Vec<u8>>,
    /// 崩溃时的 revision
    pub revision: u64,
    pub saved_at: String,
}

/// 恢复管理器
pub struct RecoveryManager {
    dir: PathBuf,
    session_id: String,
    source_id: Option<String>,
    /// 上次异常退出的会话（启动时检测到）
    pending: Option<RecoveryRecord>,
}

impl RecoveryManager {
    /// 创建（或复用现有）恢复管理器
    pub fn new() -> Self {
        Self::in_directory(default_recovery_dir())
    }

    /// Explicit directory for isolated hosts and tests; never alter process-global env.
    pub fn in_directory(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        let session_id = new_session_id();

        let mut mgr = Self {
            dir,
            session_id,
            source_id: None,
            pending: None,
        };
        let current = mgr.session_id.clone();
        // 检测上次异常退出
        mgr.pending = mgr.find_stale_session(&current);
        mgr
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// 上次异常退出的记录（如果有任何遗留 session 文件）
    pub fn pending(&self) -> Option<&RecoveryRecord> {
        self.pending.as_ref()
    }

    /// 列出所有遗留的恢复文件
    pub fn stale_sessions(&self) -> Vec<RecoveryRecord> {
        self.find_stale_session("")
            .into_iter()
            .collect::<Vec<_>>()
            .into_iter()
            .chain(std::iter::empty())
            .collect::<Vec<_>>()
    }

    /// 找出所有 session != current 的恢复文件，按时间倒序
    fn find_stale_session(&self, current: &str) -> Option<RecoveryRecord> {
        let entries = std::fs::read_dir(&self.dir).ok()?;
        let mut candidates: Vec<RecoveryRecord> = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let p = e.path();
                if p.extension().and_then(|s| s.to_str()) != Some("json") {
                    return None;
                }
                let s = std::fs::read_to_string(&p).ok()?;
                let mut rec: RecoveryRecord = serde_json::from_str(&s).ok()?;
                if rec.session_id == current {
                    return None;
                }
                if let Some(source_id) = rec.source_id.as_deref().filter(|id| safe_source_id(id)) {
                    // 由已扫描到的 JSON 文件名推导 sidecar，绝不使用记录中的路径。
                    let stem = p.file_stem()?.to_str()?;
                    let source_path = self.source_path(stem, source_id);
                    if let Ok(meta) = std::fs::metadata(&source_path) {
                        if meta.len() <= crate::project::MAX_ORIGINAL_SOURCE_BYTES {
                            rec.original_source = std::fs::read(source_path)
                                .ok()
                                .filter(|bytes| bytes.len() as u64 <= crate::project::MAX_ORIGINAL_SOURCE_BYTES);
                        }
                    }
                }
                Some(rec)
            })
            .collect();
        // 最新的优先
        candidates.sort_by(|a, b| b.saved_at.cmp(&a.saved_at));
        candidates.into_iter().next()
    }

    /// 写入当前状态（原子写入）
    pub fn save(
        &mut self,
        container: Option<&Path>,
        document: &ScreenplayDocument,
        history_json: Option<String>,
    ) -> std::io::Result<PathBuf> {
        let rec = RecoveryRecord {
            session_id: self.session_id.clone(),
            container: container.map(|p| p.to_string_lossy().to_string()),
            document: document.clone(),
            history: history_json,
            source_id: self.source_id.clone(),
            original_source: None,
            revision: document.revision,
            saved_at: chrono::Utc::now().to_rfc3339(),
        };
        let final_path = self.dir.join(format!("{}.json", self.session_id));
        let tmp_path = self.dir.join(format!("{}.json.tmp", self.session_id));
        let json = serde_json::to_string(&rec).map_err(std::io::Error::other)?;
        std::fs::write(&tmp_path, json)?;
        std::fs::rename(&tmp_path, &final_path)?;
        let keep = self.source_id.as_deref().map(|id| self.source_path(&self.session_id, id));
        self.remove_other_source_files(&self.session_id, keep.as_deref());
        Ok(final_path)
    }

    /// 原子更新当前会话的原始文件附件。普通文本编辑不会重复写入大文件。
    pub fn update_original_source(&mut self, original_source: Option<&[u8]>) -> std::io::Result<()> {
        let Some(bytes) = original_source else {
            self.source_id = None;
            return Ok(());
        };
        if bytes.len() as u64 > crate::project::MAX_ORIGINAL_SOURCE_BYTES {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "原始导入文件超过 512 MiB"));
        }
        let source_id = uuid::Uuid::new_v4().simple().to_string();
        let source_path = self.source_path(&self.session_id, &source_id);
        let source_tmp = source_path.with_extension("source.tmp");
        std::fs::write(&source_tmp, bytes)?;
        std::fs::rename(&source_tmp, &source_path)?;
        self.source_id = Some(source_id);
        Ok(())
    }

    fn source_path(&self, session_stem: &str, source_id: &str) -> PathBuf {
        self.dir.join(format!("{session_stem}-{source_id}.source"))
    }

    fn remove_other_source_files(&self, session_stem: &str, keep: Option<&Path>) {
        let prefix = format!("{session_stem}-");
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for entry in entries.filter_map(|entry| entry.ok()) {
                let path = entry.path();
                let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("");
                if name.starts_with(&prefix)
                    && (name.ends_with(".source") || name.ends_with(".source.tmp"))
                    && keep != Some(path.as_path())
                {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
    }

    /// 正常退出：清除当前会话的恢复文件
    pub fn clear_current(&mut self) {
        let p = self.dir.join(format!("{}.json", self.session_id));
        let _ = std::fs::remove_file(p);
        self.remove_other_source_files(&self.session_id, None);
        self.source_id = None;
    }

    /// 用户选择丢弃恢复内容
    pub fn discard_pending(&mut self) {
        let files: Vec<PathBuf> = std::fs::read_dir(&self.dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter_map(|e| {
                        let p = e.path();
                        if p.extension().and_then(|s| s.to_str()) != Some("json") {
                            return None;
                        }
                        Some(p)
                    })
                    .collect()
            })
            .unwrap_or_default();
        for path in files {
            if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
                let _ = std::fs::remove_file(&path);
                self.remove_other_source_files(stem, None);
            }
        }
        self.pending = None;
    }

    /// 采纳成功后仅消费被采纳的旧会话，不影响其他恢复记录。
    pub fn consume_pending(&mut self) {
        if let Some(record) = self.pending.take() {
            if safe_session_id(&record.session_id) {
                let path = self.dir.join(format!("{}.json", record.session_id));
                let _ = std::fs::remove_file(path);
                self.remove_other_source_files(&record.session_id, None);
            }
        }
    }

    /// 恢复成功：清除当前会话（不再需要）
    pub fn mark_recovered(&mut self) {
        self.clear_current();
        self.pending = None;
    }

    /// 清理超过 N 天的旧恢复文件
    pub fn prune(&mut self, keep_days: i64) {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(keep_days);
        if let Ok(rd) = std::fs::read_dir(&self.dir) {
            for e in rd.filter_map(|e| e.ok()) {
                let p = e.path();
                if p.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                if let Ok(s) = std::fs::read_to_string(&p) {
                    if let Ok(rec) = serde_json::from_str::<RecoveryRecord>(&s) {
                        if let Ok(t) = chrono::DateTime::parse_from_rfc3339(&rec.saved_at) {
                            if t.with_timezone(&chrono::Utc) < cutoff {
                                let _ = std::fs::remove_file(&p);
                                if let Some(stem) = p.file_stem().and_then(|stem| stem.to_str()) {
                                    self.remove_other_source_files(stem, None);
                                }
                            }
                        }
                    }
                }
            }
        }

        // 清理崩溃或只读项目打开后遗留的、没有任何恢复 JSON 引用的 sidecar。
        let referenced: HashSet<String> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    return None;
                }
                let stem = path.file_stem()?.to_str()?;
                let content = std::fs::read_to_string(&path).ok()?;
                let record: RecoveryRecord = serde_json::from_str(&content).ok()?;
                let source_id = record.source_id.filter(|id| safe_source_id(id))?;
                Some(format!("{stem}-{source_id}.source"))
            })
            .collect();
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for entry in entries.filter_map(|entry| entry.ok()) {
                let path = entry.path();
                let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("");
                if (name.ends_with(".source") || name.ends_with(".source.tmp"))
                    && !referenced.contains(name)
                {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
    }
}

impl Default for RecoveryManager {
    fn default() -> Self {
        Self::new()
    }
}

fn default_recovery_dir() -> PathBuf {
    let mut p = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join("Library").join("Application Support")))
        .unwrap_or_else(std::env::temp_dir);
    p.push("ScreenplayStudio");
    p.push("recovery");
    p
}

fn safe_session_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn safe_source_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn new_session_id() -> String {
    format!("s{}", uuid::Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenplay_core::ast::BlockType;

    fn mgr_in(dir: &Path) -> RecoveryManager {
        RecoveryManager {
            dir: dir.to_path_buf(),
            session_id: "test-session".into(),
            source_id: None,
            pending: None,
        }
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("screenplay-recovery-test-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn doc_with(text: &str) -> ScreenplayDocument {
        let mut d = ScreenplayDocument::blank("测试");
        d.blocks.push(screenplay_core::ast::ScreenplayBlock::new(
            BlockType::Action,
            text,
        ));
        d
    }

    #[test]
    fn consuming_recovery_preserves_current_and_unrelated_sessions() {
        let dir = tmpdir("consume");
        let mut m = mgr_in(&dir);
        for id in ["old", "other", "current"] {
            m.session_id = id.into();
            m.save(None, &doc_with(id), None).unwrap();
        }
        let record: RecoveryRecord = serde_json::from_slice(&std::fs::read(dir.join("old.json")).unwrap()).unwrap();
        m.pending = Some(record);
        m.consume_pending();
        assert!(m.pending().is_none());
        assert!(!dir.join("old.json").exists());
        assert!(dir.join("other.json").exists());
        assert!(dir.join("current.json").exists());
    }

    #[test]
    fn t01_save_creates_recovery_file() {
        let dir = tmpdir("01");
        let mut m = mgr_in(&dir);
        m.save(None, &doc_with("内容"), None).unwrap();
        let files: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
        assert_eq!(files.len(), 1, "应生成 1 个恢复文件");
    }

    #[test]
    fn t02_stale_session_detected_after_crash() {
        let dir = tmpdir("02");
        // 模拟上次会话写入
        let mut m1 = mgr_in(&dir);
        m1.session_id = "old".into();
        m1.save(None, &doc_with("崩溃前的内容"), None).unwrap();

        // 新会话启动
        let mut m2 = mgr_in(&dir);
        m2.session_id = "new".into();
        let stale = m2.find_stale_session("new");
        assert!(stale.is_some(), "应检测到遗留会话");
        assert_eq!(
            stale.unwrap().document.blocks[0].text,
            "崩溃前的内容",
            "应能恢复崩溃前的内容"
        );
    }

    #[test]
    fn original_source_survives_recovery_and_is_removed_when_consumed() {
        let dir = tmpdir("source");
        let mut old = mgr_in(&dir);
        old.session_id = "old-source".into();
        let source = b"source file bytes";
        old.update_original_source(Some(source)).unwrap();
        old.save(None, &doc_with("原稿文本"), None).unwrap();

        let mut current = mgr_in(&dir);
        current.session_id = "current".into();
        current.pending = current.find_stale_session("current");
        assert_eq!(current.pending().unwrap().original_source.as_deref(), Some(source.as_slice()));
        current.consume_pending();
        assert!(!dir.join("old-source.json").exists());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0, "消费恢复记录时同时删除附件");
    }

    #[test]
    fn editing_recovery_does_not_rewrite_the_original_attachment() {
        let dir = tmpdir("source-stable");
        let mut manager = mgr_in(&dir);
        manager.update_original_source(Some(b"large source bytes")).unwrap();
        manager.save(None, &doc_with("first edit"), None).unwrap();
        let first: RecoveryRecord = serde_json::from_slice(
            &std::fs::read(dir.join("test-session.json")).unwrap(),
        )
        .unwrap();
        manager.save(None, &doc_with("second edit"), None).unwrap();
        let second: RecoveryRecord = serde_json::from_slice(
            &std::fs::read(dir.join("test-session.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(first.source_id, second.source_id);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
    }

    #[test]
    fn prune_removes_source_sidecars_without_recovery_records() {
        let dir = tmpdir("orphan-source");
        let mut manager = mgr_in(&dir);
        manager.update_original_source(Some(b"unreferenced source")).unwrap();
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        manager.prune(30);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    }

    #[test]
    fn t03_current_session_not_treated_as_stale() {
        let dir = tmpdir("03");
        let m = mgr_in(&dir);
        assert!(m.find_stale_session(&m.session_id).is_none());
    }

    #[test]
    fn t04_clear_current_removes_file() {
        let dir = tmpdir("04");
        let mut m = mgr_in(&dir);
        m.save(None, &doc_with("x"), None).unwrap();
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        m.clear_current();
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    }

    #[test]
    fn t05_atomic_write_leaves_no_tmp() {
        let dir = tmpdir("05");
        let mut m = mgr_in(&dir);
        m.save(None, &doc_with("x"), None).unwrap();
        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert!(!names.iter().any(|n| n.ends_with(".tmp")), "不应留下 .tmp: {names:?}");
    }

    #[test]
    fn t06_history_included_and_restored() {
        let dir = tmpdir("06");
        let mut m = mgr_in(&dir);
        m.save(None, &doc_with("x"), Some("{\"undo\":[],\"redo\":[]}".into()))
            .unwrap();
        let stale = m.find_stale_session("other").unwrap();
        assert!(stale.history.is_some(), "撤销栈应被保存");
    }

    #[test]
    fn t07_container_path_recorded() {
        let dir = tmpdir("07");
        let mut m = mgr_in(&dir);
        m.save(
            Some(Path::new("/tmp/x.screenplay")),
            &doc_with("x"),
            None,
        )
        .unwrap();
        let stale = m.find_stale_session("other").unwrap();
        assert_eq!(stale.container.as_deref(), Some("/tmp/x.screenplay"));
    }

    #[test]
    fn t08_discard_pending_removes_all() {
        let dir = tmpdir("08");
        let mut m1 = mgr_in(&dir);
        m1.session_id = "old1".into();
        m1.save(None, &doc_with("a"), None).unwrap();
        let mut m2 = mgr_in(&dir);
        m2.session_id = "old2".into();
        m2.save(None, &doc_with("b"), None).unwrap();
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);

        m1.discard_pending();
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    }

    #[test]
    fn t09_corrupt_recovery_file_ignored() {
        let dir = tmpdir("09");
        std::fs::write(dir.join("bad.json"), b"{ not json").unwrap();
        let m = mgr_in(&dir);
        assert!(
            m.find_stale_session("new").is_none(),
            "损坏的恢复文件应被忽略而不是崩溃"
        );
    }

    #[test]
    fn t10_newest_session_wins() {
        let dir = tmpdir("10");
        let mut a = mgr_in(&dir);
        a.session_id = "a".into();
        a.save(None, &doc_with("较早"), None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let mut b = mgr_in(&dir);
        b.session_id = "b".into();
        b.save(None, &doc_with("较新"), None).unwrap();

        let m = mgr_in(&dir);
        let latest = m.find_stale_session("new").unwrap();
        assert_eq!(
            latest.document.blocks[0].text, "较新",
            "应恢复最新的会话"
        );
    }

    #[test]
    fn t11_overwrite_keeps_single_file() {
        let dir = tmpdir("11");
        let mut m = mgr_in(&dir);
        for i in 0..5 {
            m.save(None, &doc_with(&format!("v{i}")), None).unwrap();
        }
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            1,
            "同一会话多次保存只应有一个文件"
        );
    }

    #[test]
    fn t12_prune_removes_old() {
        let dir = tmpdir("12");
        let mut m = mgr_in(&dir);
        m.session_id = "ancient".into();
        m.save(None, &doc_with("x"), None).unwrap();
        // prune 0 天 = 全部删除
        m.prune(0);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    }
}
