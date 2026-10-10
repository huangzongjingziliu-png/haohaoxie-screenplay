//! ProjectService —— 唯一权威项目状态。
//!
//! 说明书第 82 条：GUI、Agent、CLI、MCP 必须共用同一个 CommandService，
//! 禁止任何一方维护自己的文档副本。
//!
//! 本模块把「打开的 Workspace」从散落的 `Mutex<Option<Arc<Workspace>>>`
//! 收敛为单一服务对象，并提供：
//! - 权威文档读取（前端不得持有可写的文档副本）
//! - 统一的保存语义（保存用后端权威状态，不用前端快照）
//! - dirty 追踪（基于 revision 比较，而非前端布尔标志）
//! - 变更广播（供后续 C 阶段的外部写入实时同步使用）

use screenplay_core::ast::ScreenplayDocument;
use screenplay_store::command::{CommandError, CommandResult};
use screenplay_store::db::Origin;
use screenplay_store::workspace::Workspace;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use screenplay_store::recovery::RecoveryManager;
use ts_rs::TS;

/// 项目身份：新建 / 已打开 / 未保存
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, TS)]
#[serde(tag = "state", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/types/")]
pub enum ProjectState {
    /// 尚未打开任何项目（欢迎页）
    Empty,
    /// 新建但尚未保存到磁盘
    Unsaved { title: String },
    /// 已绑定到磁盘文件
    Open { path: PathBuf, title: String },
}

impl ProjectState {
    pub fn path(&self) -> Option<&Path> {
        match self {
            ProjectState::Open { path, .. } => Some(path.as_path()),
            _ => None,
        }
    }

    pub fn title(&self) -> &str {
        match self {
            ProjectState::Unsaved { title } => title,
            ProjectState::Open { title, .. } => title,
            ProjectState::Empty => "",
        }
    }

    pub fn is_open(&self) -> bool {
        !matches!(self, ProjectState::Empty)
    }
}

/// 保存结果：明确告知前端「保存的是哪个 revision」
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/types/")]
pub struct SaveOutcome {
    pub path: String,
    /// 保存时的文档 revision（前端据此判断保存期间是否又发生修改）
    pub saved_revision: u64,
    pub saved_at: String,
}

/// 待恢复的崩溃前状态（供 UI 提示）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/types/")]
pub struct PendingRecovery {
    pub container: Option<String>,
    pub title: String,
    #[ts(type = "number")]
    pub revision: u64,
    #[ts(type = "number")]
    pub block_count: usize,
    pub saved_at: String,
    pub preview: String,
}

/// 自动保存配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/types/")]
pub struct AutosaveConfig {
    pub enabled: bool,
    #[ts(type = "number")]
    pub save_interval_secs: u64,
    #[ts(type = "number")]
    pub version_interval_secs: u64,
}

/// 自动保存 tick 的结果
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/types/")]
pub enum AutosaveAction {
    /// 已自动保存到磁盘
    Saved,
    /// 已自动创建版本检查点
    VersionCreated,
}

/// 项目服务：持有唯一 Workspace
pub struct ProjectService {
    workspace: Mutex<Option<Arc<Workspace>>>,
    /// 最近一次成功保存时的 revision
    saved_revision: AtomicU64,
    /// 是否有未保存修改
    dirty: AtomicBool,
    /// 正在进行的保存操作计数（用于 in-flight dirty 判定）
    saving: Mutex<u64>,
    /// 崩溃恢复管理器
    recovery: Mutex<RecoveryManager>,
    /// 自动保存状态
    autosave: AutosaveState,
}

/// 自动保存配置与计时
struct AutosaveState {
    /// 是否启用自动保存
    enabled: AtomicBool,
    /// 自动保存间隔（秒）
    interval_secs: AtomicU64,
    /// 自动版本间隔（秒），0 = 关闭
    version_interval_secs: AtomicU64,
    /// 上次自动保存时的 revision
    last_saved_rev: AtomicU64,
    /// 上次自动版本时的 revision
    last_version_rev: AtomicU64,
    clocks: Mutex<(Instant, Instant)>,
}

impl Default for ProjectService {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectService {
    pub fn new() -> Self {
        #[cfg(not(test))]
        let recovery = RecoveryManager::new();
        #[cfg(test)]
        let recovery = RecoveryManager::in_directory(std::env::temp_dir().join(screenplay_core::ast::new_id("muben-service-test")));
        Self::from_recovery(recovery)
    }

    pub fn with_recovery_directory(directory: PathBuf) -> Self {
        Self::from_recovery(RecoveryManager::in_directory(directory))
    }

    fn from_recovery(mut recovery: RecoveryManager) -> Self {
        // 清理 30 天前的旧恢复文件
        recovery.prune(30);
        Self {
            workspace: Mutex::new(None),
            saved_revision: AtomicU64::new(0),
            dirty: AtomicBool::new(false),
            saving: Mutex::new(0),
            recovery: Mutex::new(recovery),
            autosave: AutosaveState {
                enabled: AtomicBool::new(true),
                // 默认 30 秒自动保存
                interval_secs: AtomicU64::new(30),
                // 默认 5 分钟自动版本
                version_interval_secs: AtomicU64::new(300),
                last_saved_rev: AtomicU64::new(0),
                last_version_rev: AtomicU64::new(0),
                clocks: Mutex::new((Instant::now(), Instant::now())),
            },
        }
    }

    // -----------------------------------------------------------------
    // 自动保存 / 自动版本
    // -----------------------------------------------------------------

    /// 配置自动保存（间隔秒数，0 = 关闭）
    pub fn configure_autosave(
        &self,
        enabled: bool,
        save_interval_secs: u64,
        version_interval_secs: u64,
    ) {
        let a = &self.autosave;
        a.enabled.store(enabled, Ordering::SeqCst);
        a.interval_secs.store(save_interval_secs, Ordering::SeqCst);
        a.version_interval_secs
            .store(version_interval_secs, Ordering::SeqCst);
        // 重置计时基准，避免改配置后立刻触发
        let rev = self.revision();
        a.last_saved_rev.store(rev, Ordering::SeqCst);
        a.last_version_rev.store(rev, Ordering::SeqCst);
        *a.clocks.lock().unwrap() = (Instant::now(), Instant::now());
    }

    /// 自动保存配置
    pub fn autosave_config(&self) -> AutosaveConfig {
        let a = &self.autosave;
        AutosaveConfig {
            enabled: a.enabled.load(Ordering::SeqCst),
            save_interval_secs: a.interval_secs.load(Ordering::SeqCst),
            version_interval_secs: a.version_interval_secs.load(Ordering::SeqCst),
        }
    }

    /// 自动保存 tick。由前端定时调用（或 Rust 定时器）。
    ///
    /// 返回实际执行的动作，供 UI 显示「已自动保存」提示。
    pub fn autosave_tick(&self) -> Result<Option<AutosaveAction>, String> {
        self.autosave_tick_at(Instant::now())
    }

    fn autosave_tick_at(&self, now: Instant) -> Result<Option<AutosaveAction>, String> {
        if !self.is_open() || !self.autosave.enabled.load(Ordering::SeqCst) { return Ok(None); }
        let a = &self.autosave;
        let mut clocks = a.clocks.lock().unwrap();
        let rev = self.revision();
        let mut action = None;
        let version_interval = a.version_interval_secs.load(Ordering::SeqCst);
        if version_interval > 0 && rev > a.last_version_rev.load(Ordering::SeqCst)
            && now.saturating_duration_since(clocks.1) >= Duration::from_secs(version_interval) {
            let ws = self.require().map_err(|e| e.to_string())?;
            ws.service.auto_checkpoint("autosave").ok_or_else(|| "自动版本创建失败".to_string())?;
            a.last_version_rev.store(rev, Ordering::SeqCst);
            clocks.1 = now;
            action = Some(AutosaveAction::VersionCreated);
        }
        let interval = a.interval_secs.load(Ordering::SeqCst);
        if interval > 0 && self.is_dirty()
            && now.saturating_duration_since(clocks.0) >= Duration::from_secs(interval) {
            if self.require().map_err(|e| e.to_string())?.container_path().is_some() {
                self.save(None).map_err(|e| e.to_string())?;
                a.last_saved_rev.store(self.saved_revision.load(Ordering::SeqCst), Ordering::SeqCst);
                clocks.0 = now;
                action = Some(AutosaveAction::Saved);
            }
        }
        Ok(action)
    }

    fn reset_autosave_for_project(&self) {
        if let Ok(ws) = self.require() {
            let settings = ws.document().settings;
            self.configure_autosave(settings.auto_save, settings.auto_save_interval_secs, settings.auto_version_interval_secs);
        }
    }

    // -----------------------------------------------------------------
    // 生命周期
    // -----------------------------------------------------------------

    /// 新建空白项目。若已有项目，**无条件替换**（不得继续写入旧项目）。
    pub fn create_new(&self, title: &str) -> Result<(), String> {
        let ws = Workspace::blank(title)?;
        self.sync_recovery_source_from(&ws);
        *self.workspace.lock().unwrap() = Some(Arc::new(ws));
        self.saved_revision.store(0, Ordering::SeqCst);
        self.dirty.store(true, Ordering::SeqCst);
        self.reset_autosave_for_project();
        Ok(())
    }

    /// 打开已有项目
    pub fn open(&self, path: &Path) -> Result<(), String> {
        let ws = Workspace::open(path)?;
        self.sync_recovery_source_from(&ws);
        *self.workspace.lock().unwrap() = Some(Arc::new(ws));
        // 刚打开的项目是已保存状态
        let rev = self.revision();
        self.saved_revision.store(rev, Ordering::SeqCst);
        self.dirty.store(false, Ordering::SeqCst);
        self.reset_autosave_for_project();
        Ok(())
    }

    /// 关闭当前项目（回到欢迎页）
    pub fn close(&self) -> Result<(), String> {
        // 关闭前先写恢复点（用户可能刚编辑完就关闭）
        self.write_recovery();
        *self.workspace.lock().unwrap() = None;
        self.saved_revision.store(0, Ordering::SeqCst);
        self.dirty.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Explicit discard only affects this session; never deletes the saved project or older recoveries.
    pub fn close_without_saving(&self) -> Result<(), String> {
        let _saving = self.saving.lock().map_err(|e| e.to_string())?;
        *self.workspace.lock().map_err(|e| e.to_string())? = None;
        self.saved_revision.store(0, Ordering::SeqCst);
        self.dirty.store(false, Ordering::SeqCst);
        self.recovery.lock().map_err(|e| e.to_string())?.clear_current();
        Ok(())
    }

    pub fn state(&self) -> ProjectState {
        let guard = self.workspace.lock().unwrap();
        match guard.as_ref() {
            None => ProjectState::Empty,
            Some(ws) => {
                let title = ws.document().metadata.title.clone();
                match ws.container_path() {
                    Some(p) => ProjectState::Open { path: p, title },
                    None => ProjectState::Unsaved { title },
                }
            }
        }
    }

    pub fn is_open(&self) -> bool {
        self.workspace.lock().unwrap().is_some()
    }

    // -----------------------------------------------------------------
    // 权威状态读取
    // -----------------------------------------------------------------

    /// 取当前 Workspace（未打开则报错）
    pub fn require(&self) -> Result<Arc<Workspace>, String> {
        self.workspace
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "尚未打开项目".to_string())
    }

    /// 取 CommandService 的 Arc 副本（供 Agent 等需要 Arc<CommandService> 的模块）。
    /// CommandService 内部用 Arc<Store>，clone 仅复制 Arc，共享同一份存储。
    pub fn command_service_arc(&self) -> Result<Arc<screenplay_store::command::CommandService>, String> {
        let ws = self.require()?;
        Ok(Arc::new(ws.service.clone()))
    }

    pub fn revision(&self) -> u64 {
        self.workspace
            .lock()
            .unwrap()
            .as_ref()
            .map(|ws| ws.revision())
            .unwrap_or(0)
    }

    /// 权威文档快照（只读用途）
    pub fn document(&self) -> Result<ScreenplayDocument, String> {
        Ok(self.require()?.document())
    }

    /// dirty 状态：基于 revision 与 saved_revision 比较，**不依赖前端标志**
    ///
    /// 三种情况：
    /// 1. 未打开项目 → 不 dirty
    /// 2. 已打开但 revision 高于上次保存 → dirty（有未保存修改）
    /// 3. 已打开且 revision 等于上次保存 → clean
    ///
    /// 注意「新建未保存」项目：即使 revision == saved_revision == 0，
    /// 也必须算 dirty，因为它还没落盘。
    pub fn is_dirty(&self) -> bool {
        let guard = self.workspace.lock().unwrap();
        let ws = match guard.as_ref() {
            None => return false,
            Some(ws) => ws,
        };
        // 未绑定磁盘路径的新项目，一定是 dirty
        if ws.container_path().is_none() {
            return true;
        }
        let current = ws.revision();
        let saved = self.saved_revision.load(Ordering::SeqCst);
        current > saved
    }

    // -----------------------------------------------------------------
    // 保存
    // -----------------------------------------------------------------

    /// 保存到指定路径（另存为），或保存到当前绑定路径
    ///
    /// 关键：**保存的是后端权威状态**，不接受前端传入的文档 JSON。
    /// 这样前端无法用过期快照覆盖后端的新内容。
    pub fn save(&self, target: Option<&Path>) -> Result<SaveOutcome, String> {
        let ws = self.require()?;
        let path: PathBuf = match target {
            Some(p) => p.to_path_buf(),
            None => ws
                .container_path()
                .ok_or("尚未指定保存路径，请使用「另存为」")?,
        };

        // Serialize saves; edits can continue, so use the exact persisted snapshot revision.
        let _saving = self.saving.lock().unwrap();
        if !self.workspace.lock().unwrap().as_ref().is_some_and(|active| Arc::ptr_eq(active, &ws)) {
            return Err("项目已关闭，取消保存".into());
        }
        let (saved_path, rev) = ws.save_with_revision(Some(&path))?;
        ws.bind_container(&saved_path);
        let current = self.workspace.lock().unwrap();
        if current.as_ref().is_some_and(|active| Arc::ptr_eq(active, &ws)) {
            self.saved_revision.store(rev, Ordering::SeqCst);
        }
        drop(current);
        Ok(SaveOutcome {
            path: saved_path.to_string_lossy().to_string(),
            saved_revision: rev,
            saved_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// 放弃未保存修改（回到 saved_revision 对应状态）
    /// 需要重新从磁盘读取
    pub fn revert(&self) -> Result<(), String> {
        let guard = self.workspace.lock().unwrap();
        let ws = guard.as_ref().ok_or("尚未打开项目")?;
        let path = ws
            .container_path()
            .ok_or("未保存的新项目无法放弃修改（没有磁盘版本可回退）")?;
        drop(guard);
        let fresh = Workspace::open(&path)?;
        self.sync_recovery_source_from(&fresh);
        let rev = fresh.revision();
        *self.workspace.lock().unwrap() = Some(Arc::new(fresh));
        self.saved_revision.store(rev, Ordering::SeqCst);
        self.dirty.store(false, Ordering::SeqCst);
        Ok(())
    }

    // -----------------------------------------------------------------
    // 修改入口：全部经过 CommandService
    // -----------------------------------------------------------------

    /// 执行一次命令并同步 dirty 状态
    pub fn run<F>(&self, f: F) -> Result<CommandResult, CommandError>
    where
        F: FnOnce(&screenplay_store::command::CommandService) -> Result<CommandResult, CommandError>,
    {
        let ws = self.require().map_err(|m| CommandError::Internal { message: m })?;
        let result = f(&ws.service)?;
        self.dirty.store(self.is_dirty(), Ordering::SeqCst);
        // 每次修改都写一次崩溃恢复点（数据安全底线：任何输入都不能丢）
        self.write_recovery();
        Ok(result)
    }

    /// 执行导入并把对应原始文件绑定到同一个 Workspace。
    /// 文档替换失败时不改动当前项目的源文件归档。
    pub fn run_with_original_source<F>(
        &self,
        f: F,
        original_source: Vec<u8>,
    ) -> Result<CommandResult, CommandError>
    where
        F: FnOnce(&screenplay_store::command::CommandService) -> Result<CommandResult, CommandError>,
    {
        if original_source.len() as u64 > screenplay_store::project::MAX_ORIGINAL_SOURCE_BYTES {
            return Err(CommandError::InvalidArgument { message: "原始导入文件超过 512 MiB，无法归档".into() });
        }
        let ws = self.require().map_err(|message| CommandError::Internal { message })?;
        let result = f(&ws.service)?;
        ws.set_original_source(original_source)
            .map_err(|message| CommandError::Internal { message })?;
        self.sync_recovery_source_from(&ws);
        self.dirty.store(self.is_dirty(), Ordering::SeqCst);
        self.write_recovery();
        Ok(result)
    }

    /// 写入崩溃恢复点
    fn write_recovery(&self) -> bool {
        let ws = match self.workspace.lock().unwrap().clone() {
            Some(ws) => ws,
            None => return false,
        };
        let doc = ws.document();
        let hist_json = ws
            .service
            .store
            .history
            .lock()
            .ok()
            .and_then(|h| h.to_json().ok());
        let container = ws.container_path();
        self.recovery
            .lock()
            .map(|mut r| r.save(container.as_deref(), &doc, hist_json).is_ok())
            .unwrap_or(false)
    }

    /// 把项目当前源文件同步到本会话恢复 sidecar。仅在项目切换/导入时调用，
    /// 普通编辑只更新恢复 JSON，不会反复复制大型原稿文件。
    fn sync_recovery_source_from(&self, ws: &Workspace) {
        let _ = ws.with_original_source(|source| {
            self.recovery
                .lock()
                .map(|mut recovery| recovery.update_original_source(source).is_ok())
                .unwrap_or(false)
        });
    }

    /// 是否有待恢复的崩溃前状态
    pub fn has_pending_recovery(&self) -> bool {
        self.recovery.lock().map(|r| r.pending().is_some()).unwrap_or(false)
    }

    /// 崩溃恢复信息（供 UI 提示）
    pub fn pending_recovery(&self) -> Option<PendingRecovery> {
        self.recovery.lock().ok().and_then(|r| {
            r.pending().map(|rec| PendingRecovery {
                container: rec.container.clone(),
                title: rec.document.metadata.title.clone(),
                revision: rec.revision,
                block_count: rec.document.blocks.len(),
                saved_at: rec.saved_at.clone(),
                preview: rec
                    .document
                    .blocks
                    .iter()
                    .map(|b| b.text.as_str())
                    .filter(|t| !t.trim().is_empty())
                    .take(3)
                    .collect::<Vec<_>>()
                    .join(" / "),
            })
        })
    }

    /// 采纳恢复内容（替换当前文档）
    pub fn adopt_recovery(&self) -> Result<ScreenplayDocument, String> {
        let rec = {
            let r = self.recovery.lock().map_err(|_| "恢复管理器已损坏")?;
            r.pending().cloned()
        };
        let rec = rec.ok_or("没有待恢复的内容")?;

        // 恢复撤销栈（若有），这样恢复后仍可撤销到更早状态
        let restored_history = rec
            .history
            .as_deref()
            .and_then(|j| screenplay_store::history::HistoryStack::from_json(j).ok());

        // 若原项目容器仍存在，直接打开它并载入恢复的文档
        let ws = match rec.container.as_deref() {
            Some(p) if std::path::Path::new(p).exists() => {
                let w = Workspace::open(std::path::Path::new(p))?;
                self.saved_revision
                    .store(w.service.store.revision(), Ordering::SeqCst);
                w
            }
            _ => {
                // 容器已不存在：作为未保存项目恢复
                let w = Workspace::blank(&rec.document.metadata.title)?;
                self.saved_revision.store(0, Ordering::SeqCst);
                w
            }
        };

        if let Some(source) = rec.original_source.clone() {
            ws.set_original_source(source)?;
        }
        self.sync_recovery_source_from(&ws);

        if let Some(h) = restored_history {
            *ws.service.store.history.lock().unwrap() = h;
        }

        let r = ws
            .service
            .load_document(Origin::System, rec.document.clone())
            .map_err(|e| e.to_string())?;

        *self.workspace.lock().unwrap() = Some(Arc::new(ws));
        self.reset_autosave_for_project();
        self.dirty.store(true, Ordering::SeqCst);
        if self.write_recovery() {
            if let Ok(mut recovery) = self.recovery.lock() {
                recovery.consume_pending();
            }
        }
        Ok(r.document)
    }

    /// 丢弃恢复内容
    pub fn discard_recovery(&self) {
        if let Ok(mut r) = self.recovery.lock() {
            r.discard_pending();
        }
    }

    /// 未保存内容保留恢复点；已保存项目正常退出不留下崩溃提示。
    pub fn shutdown(&self) {
        if self.is_dirty() {
            self.write_recovery();
        } else if let Ok(mut recovery) = self.recovery.lock() {
            recovery.clear_current();
        }
    }

    // -----------------------------------------------------------------
    // 导入
    // -----------------------------------------------------------------

    /// 导入外部文件到当前项目（替换文档内容）
    pub fn import_file(&self, path: &Path) -> Result<CommandResult, CommandError> {
        let (document, original_source) = crate::read_import_with_source(path)
            .map_err(|error| CommandError::Internal { message: error.message() })?;
        self.run_with_original_source(
            |svc| svc.load_document(Origin::Import, document),
            original_source,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("screenplay-svc-test-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_file(&p);
        p
    }

    /// P0 回归：新建项目后不得继续写入旧项目
    /// 场景：打开 A → 新建 B → 保存 → A 的内容不能被 B 覆盖
    #[test]
    fn t01_new_project_does_not_write_to_old_container() {
        let svc = ProjectService::new();
        let path_a = temp_path("a.screenplay");

        // 打开 A 并写入内容
        svc.create_new("项目A").unwrap();
        svc.run(|s| {
            s.insert_block(
                Origin::GuiLeft,
                None,
                screenplay_core::ast::BlockType::Action,
                "项目A的内容",
                None,
            )
        })
        .unwrap();
        svc.save(Some(&path_a)).unwrap();

        // 新建 B
        svc.create_new("项目B").unwrap();
        svc.run(|s| {
            s.insert_block(
                Origin::GuiLeft,
                None,
                screenplay_core::ast::BlockType::Action,
                "项目B的内容",
                None,
            )
        })
        .unwrap();
        let path_b = temp_path("b.screenplay");
        svc.save(Some(&path_b)).unwrap();

        // 重新打开 A，内容必须是「项目A的内容」
        let reloaded = Workspace::open(&path_a).unwrap();
        let text = reloaded.document().blocks[0].text.clone();
        assert_eq!(text, "项目A的内容", "新建项目后不得污染旧项目");

        let _ = std::fs::remove_file(&path_a);
        let _ = std::fs::remove_file(&path_b);
    }

    /// P0 回归：保存使用后端权威状态，不接受前端快照
    #[test]
    fn t02_save_uses_backend_authority() {
        let svc = ProjectService::new();
        let p = temp_path("auth.screenplay");
        svc.create_new("权威测试").unwrap();

        // 模拟「前端声称的旧快照」——我们不提供这个入口，
        // 而是通过 CommandService 修改后保存，验证保存的是后端状态
        svc.run(|s| {
            s.insert_block(
                Origin::GuiLeft,
                None,
                screenplay_core::ast::BlockType::Action,
                "后端写入的内容",
                None,
            )
        })
        .unwrap();
        svc.save(Some(&p)).unwrap();

        let reloaded = Workspace::open(&p).unwrap();
        assert_eq!(reloaded.document().blocks[0].text, "后端写入的内容");
        let _ = std::fs::remove_file(&p);
    }

    /// P0 回归：dirty 基于 revision，保存后不残留
    #[test]
    fn t03_dirty_tracks_revision_not_frontend_flag() {
        let svc = ProjectService::new();
        let p = temp_path("dirty.screenplay");
        svc.create_new("dirty").unwrap();
        assert!(svc.is_dirty(), "新建项目应为 dirty（尚未落盘）");

        svc.run(|s| {
            s.insert_block(
                Origin::GuiLeft,
                None,
                screenplay_core::ast::BlockType::Action,
                "x",
                None,
            )
        })
        .unwrap();
        svc.save(Some(&p)).unwrap();
        assert!(!svc.is_dirty(), "保存后应为 clean");

        // 再修改 → dirty
        let block_id = svc.document().unwrap().blocks[0].id.clone();
        svc.run(|s| s.replace_block(Origin::GuiLeft, &block_id, "y", None))
            .unwrap();
        assert!(svc.is_dirty(), "修改后应为 dirty");
        let _ = std::fs::remove_file(&p);
    }

    /// P0 回归：关闭后必须拒绝所有修改操作
    #[test]
    fn t04_close_project_rejects_further_edits() {
        let svc = ProjectService::new();
        svc.create_new("t").unwrap();
        svc.close().unwrap();
        assert!(!svc.is_open());
        let r = svc.run(|s| {
            s.insert_block(
                Origin::GuiLeft,
                None,
                screenplay_core::ast::BlockType::Action,
                "x",
                None,
            )
        });
        assert!(r.is_err(), "关闭后修改必须失败");
    }

    #[test]
    fn imported_file_bytes_survive_project_service_save_and_reopen() {
        let svc = ProjectService::new();
        let source_path = temp_path("import-source.txt");
        let project_path = temp_path("import-archive.screenplay");
        let original = "原稿第一场\n人物：你好。\n".as_bytes();
        std::fs::write(&source_path, original).unwrap();

        svc.create_new("导入归档测试").unwrap();
        svc.import_file(&source_path).unwrap();
        svc.save(Some(&project_path)).unwrap();
        svc.open(&project_path).unwrap();
        assert_eq!(svc.require().unwrap().original_source().as_deref(), Some(original));

        svc.shutdown();
        let _ = std::fs::remove_file(source_path);
        let _ = std::fs::remove_file(project_path);
    }
}

#[cfg(test)] mod discard_tests {
    use super::*;
    #[test] fn explicit_discard_removes_only_current_recovery_and_keeps_saved_file() {
        let root = std::env::temp_dir().join(screenplay_core::ast::new_id("discard-qa"));
        let svc = ProjectService::with_recovery_directory(root.join("recovery"));
        svc.create_new("不保存验收").unwrap();
        svc.run(|s| s.insert_block(Origin::GuiLeft, None, screenplay_core::ast::BlockType::Action, "已保存的正文", None)).unwrap();
        let path = root.join("project.screenplay");
        svc.save(Some(&path)).unwrap();
        let before = std::fs::read(&path).unwrap();
        svc.run(|s| s.insert_block(Origin::GuiLeft, None, screenplay_core::ast::BlockType::Action, "必须放弃的修改", None)).unwrap();
        assert!(svc.is_dirty());
        svc.close_without_saving().unwrap();
        svc.shutdown();
        assert!(!svc.is_open());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(RecoveryManager::in_directory(root.join("recovery")).pending().is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test] fn unsaved_project_discard_is_not_restored_on_next_startup() {
        let root = std::env::temp_dir().join(screenplay_core::ast::new_id("discard-unsaved-qa"));
        let svc = ProjectService::with_recovery_directory(root.clone());
        svc.create_new("试写").unwrap();
        svc.run(|s| s.insert_block(Origin::GuiLeft, None, screenplay_core::ast::BlockType::Action, "临时正文", None)).unwrap();
        svc.close_without_saving().unwrap(); svc.shutdown();
        assert!(RecoveryManager::in_directory(root.clone()).pending().is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
}
