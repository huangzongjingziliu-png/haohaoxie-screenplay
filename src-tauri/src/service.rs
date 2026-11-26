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

/// 路径比较：能 canonicalize 就按真实路径比，避免软链/相对路径误判。
fn same_path(a: &Path, b: &Path) -> bool {
    let normalize = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    normalize(a) == normalize(b)
}

/// 项目服务：持有唯一 Workspace
pub struct ProjectService {
    workspace: Mutex<Option<Arc<Workspace>>>,
    project_lock: Mutex<Option<screenplay_store::bridge::ProjectLock>>,
    /// 最近一次成功保存时的 revision
    saved_revision: AtomicU64,
    /// 是否有未保存修改
    dirty: AtomicBool,
    /// 正在进行的保存操作计数（用于 in-flight dirty 判定）
    saving: Mutex<u64>,
    /// 崩溃恢复管理器
    recovery: Mutex<RecoveryManager>,
    /// 最近一次恢复点写入失败（供前端提示并可重试）
    recovery_error: Mutex<Option<String>>,
    /// 退出请求已开始：后端保存屏障，禁止任何自动保存落盘
    quit_barrier: AtomicBool,
    /// 自动保存状态
    autosave: AutosaveState,
}

/// 自动保存配置与计时
struct AutosaveState {
    tick_gate:Mutex<()>,
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
            project_lock: Mutex::new(None),
            saved_revision: AtomicU64::new(0),
            dirty: AtomicBool::new(false),
            saving: Mutex::new(0),
            recovery: Mutex::new(recovery),
            recovery_error: Mutex::new(None),
            quit_barrier: AtomicBool::new(false),
            autosave: AutosaveState {
                tick_gate:Mutex::new(()),
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
    pub fn autosave_tick(&self) -> Result<Option<AutosaveAction>, String> {self.autosave_tick_at(Instant::now())}
    pub fn autosave_tick_checked(&self,project_id:Option<&str>)->Result<Option<AutosaveAction>,String>{self.autosave_tick_at_checked(Instant::now(),project_id)}
    fn autosave_tick_at(&self, now: Instant) -> Result<Option<AutosaveAction>, String> {self.autosave_tick_at_checked(now,None)}
    fn autosave_tick_at_checked(&self,now:Instant,project_id:Option<&str>)->Result<Option<AutosaveAction>,String>{
        let a=&self.autosave;let Ok(_tick)=a.tick_gate.try_lock() else{return Ok(None);};
        // 退出请求已开始：后端保存屏障生效，自动保存绝不能在用户确认前落盘。
        if self.quit_barrier.load(Ordering::SeqCst){return Ok(None);}
        if !self.is_open()||!a.enabled.load(Ordering::SeqCst){return Ok(None);}
        let ws=self.require()?;let id=ws.document().id;if project_id.is_some_and(|p|p!=id){return Ok(None);}
        // Never hold clocks while saving: lifecycle changes take saving before clocks.
        let clocks=*a.clocks.lock().unwrap();let rev=ws.revision();let mut action=None;
        let version_interval=a.version_interval_secs.load(Ordering::SeqCst);
        if version_interval>0&&rev>a.last_version_rev.load(Ordering::SeqCst)&&now.saturating_duration_since(clocks.1)>=Duration::from_secs(version_interval){
            let active=self.workspace.lock().unwrap();if !active.as_ref().is_some_and(|w|Arc::ptr_eq(w,&ws)){return Ok(None);}
            // 版本创建失败不能连累正文保存：跳过本次检查点，继续走自动保存。
            if ws.service.auto_checkpoint("autosave").is_some(){
                a.last_version_rev.store(rev,Ordering::SeqCst);a.clocks.lock().unwrap().1=now;action=Some(AutosaveAction::VersionCreated);
            }
        }
        let interval=a.interval_secs.load(Ordering::SeqCst);
        if interval>0&&self.is_dirty()&&now.saturating_duration_since(clocks.0)>=Duration::from_secs(interval)&&ws.container_path().is_some(){
            self.save_checked(None,Some(&id))?;
            let active=self.workspace.lock().unwrap();if !active.as_ref().is_some_and(|w|Arc::ptr_eq(w,&ws)){return Ok(None);}
            a.last_saved_rev.store(self.saved_revision.load(Ordering::SeqCst),Ordering::SeqCst);a.clocks.lock().unwrap().0=now;action=Some(AutosaveAction::Saved);
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

    /// Called while holding `saving`; retain a lock for the bound engineering file.
    fn acquire_target(&self,path:&Path)->Result<Option<screenplay_store::bridge::ProjectLock>,String>{
        let same=self.workspace.lock().unwrap().as_ref().and_then(|w|w.container_path()).is_some_and(|p|screenplay_store::bridge::lock_path_for(&p)==screenplay_store::bridge::lock_path_for(path));
        if same&&self.project_lock.lock().unwrap().is_some(){return Ok(None);}
        screenplay_store::bridge::ProjectLock::acquire(path).map(Some).map_err(|e|e.message())
    }
    /// 新建空白项目。若已有项目，**无条件替换**（不得继续写入旧项目）。
    pub fn create_new(&self, title: &str) -> Result<(), String> {
        self.create_new_document(ScreenplayDocument::blank(title))
    }

    /// 用模板生成的文档新建项目。同样无条件替换当前项目。
    pub fn create_new_document(&self, document: ScreenplayDocument) -> Result<(), String> {
        let _saving=self.saving.lock().map_err(|e|e.to_string())?;
        let ws = Workspace::from_document(document)?;
        self.sync_recovery_source_from(&ws);
        *self.workspace.lock().unwrap() = Some(Arc::new(ws));
        *self.project_lock.lock().unwrap()=None;
        self.saved_revision.store(0, Ordering::SeqCst);
        self.dirty.store(true, Ordering::SeqCst);
        self.reset_autosave_for_project();
        Ok(())
    }

    /// 打开已有项目
    pub fn open(&self, path: &Path) -> Result<(), String> {
        let _saving=self.saving.lock().map_err(|e|e.to_string())?;
        let target_lock=self.acquire_target(path)?;
        let ws = Workspace::open(path)?;
        self.sync_recovery_source_from(&ws);
        *self.workspace.lock().unwrap() = Some(Arc::new(ws));
        if let Some(l)=target_lock{*self.project_lock.lock().unwrap()=Some(l);}
        // 刚打开的项目是已保存状态
        let rev = self.revision();
        self.saved_revision.store(rev, Ordering::SeqCst);
        self.dirty.store(false, Ordering::SeqCst);
        self.reset_autosave_for_project();
        Ok(())
    }

    /// 关闭当前项目（回到欢迎页）。
    ///
    /// 有未保存修改时写入恢复点，并把它转存成独立副本：
    /// 之后正常退出只清当前会话，不会删掉用户选择保留的副本。
    /// 恢复点写入/转存失败必须报错，不能静默关闭。
    pub fn close(&self) -> Result<(), String> {
        let _saving=self.saving.lock().map_err(|e|e.to_string())?;
        if self.is_dirty() {
            self.write_recovery()
                .map_err(|e| format!("恢复副本写入失败：{e}；请重试，或先保存后再关闭"))?;
            self.recovery
                .lock()
                .map_err(|e| e.to_string())?
                .retain_current()
                .map_err(|e| format!("恢复副本转存失败：{e}；请重试后再关闭"))?;
        } else if let Ok(mut recovery) = self.recovery.lock() {
            recovery.clear_current();
        }
        *self.workspace.lock().unwrap() = None;
        *self.project_lock.lock().unwrap()=None;
        self.saved_revision.store(0, Ordering::SeqCst);
        self.dirty.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Explicit discard only affects this session; never deletes the saved project or older recoveries.
    pub fn close_without_saving(&self) -> Result<(), String> {
        let _saving = self.saving.lock().map_err(|e| e.to_string())?;
        *self.workspace.lock().map_err(|e| e.to_string())? = None;
        *self.project_lock.lock().map_err(|e|e.to_string())?=None;
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
    // 覆盖保存留下的残留备份
    // -----------------------------------------------------------------

    /// 启动检查：该工程是否有「覆盖保存被强杀」留下的完整备份。
    ///
    /// 工程文件本身还能正常打开时说明那次保存已经完成，备份只是旧版本：
    /// 不提示用户，也不删除备份。
    pub fn stale_save_backup(&self, path: &Path) -> Option<screenplay_store::project::StaleSaveBackup> {
        let found = screenplay_store::project::find_stale_save_backup(path)?;
        if screenplay_store::project::load_project(path).is_ok() {
            return None;
        }
        Some(found)
    }

    /// 用户明确确认后，用残留备份还原工程文件。
    ///
    /// 绝不自动调用。若当前工程的未保存修改还没落盘，直接拒绝，
    /// 避免用磁盘备份冲掉内存里的新内容。
    pub fn restore_stale_backup(&self, path: &Path) -> Result<(), String> {
        let same_open = self
            .workspace
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|ws| ws.container_path())
            .is_some_and(|current| same_path(&current, path));
        if same_open && self.is_dirty() {
            return Err("当前工程有未保存的修改，请先保存或关闭后再恢复".into());
        }
        {
            let _saving = self.saving.lock().map_err(|e| e.to_string())?;
            screenplay_store::project::restore_stale_save_backup(path)?;
        }
        if same_open {
            // 内存里还是被截断的旧文档，必须从恢复后的文件重新载入。
            self.open(path)?;
        }
        Ok(())
    }

    /// 保留残留备份但不再提示（只改名，不删除数据）。
    pub fn dismiss_stale_backup(&self, path: &Path) -> Result<bool, String> {
        screenplay_store::project::dismiss_stale_save_backup(path)
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
    pub fn save(&self, target: Option<&Path>) -> Result<SaveOutcome, String> {self.save_checked(target,None)}
    pub fn save_checked(&self,target:Option<&Path>,project_id:Option<&str>)->Result<SaveOutcome,String>{
        // Serialize saves first, then resolve the destination inside the lock:
        // a concurrent 「另存为」/close must not let us compute a stale target path.
        let _saving = self.saving.lock().unwrap();
        let ws = self.require()?;
        let path: PathBuf = match target {
            Some(p) => p.to_path_buf(),
            None => ws
                .container_path()
                .ok_or("尚未指定保存路径，请使用「另存为」")?,
        };
        if !self.workspace.lock().unwrap().as_ref().is_some_and(|active| Arc::ptr_eq(active, &ws)) {
            return Err("项目已关闭，取消保存".into());
        }
        if project_id.is_some_and(|id|ws.document().id!=id){return Err("项目已切换，取消保存".into());}
        let target_lock=self.acquire_target(&path)?;
        let (saved_path, rev) = ws.save_with_revision(Some(&path))?;
        if let Some(l)=target_lock{*self.project_lock.lock().unwrap()=Some(l);}
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
        let _saving=self.saving.lock().map_err(|e|e.to_string())?;
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
        self.persist_recovery();
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
        self.persist_recovery();
        Ok(result)
    }

    /// 写入崩溃恢复点。没有打开项目时返回 `Ok(false)`。
    ///
    /// 写失败必须让调用方看见：关闭/退出流程据此中止，
    /// 普通编辑则记下来供前端提示，并在下次编辑时自动重试。
    fn write_recovery(&self) -> Result<bool, String> {
        let ws = match self.workspace.lock().unwrap().clone() {
            Some(ws) => ws,
            None => return Ok(false),
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
            .map_err(|e| e.to_string())?
            .save(container.as_deref(), &doc, hist_json)
            .map(|_| true)
            .map_err(|e| e.to_string())
    }

    /// 普通编辑后的恢复点写入：失败重试一次，仍失败则记录下来供前端提示。
    fn persist_recovery(&self) {
        if self.write_recovery().is_err() && self.write_recovery().is_err() {
            if let Ok(mut slot) = self.recovery_error.lock() {
                *slot = Some("崩溃恢复点写入失败；正在继续尝试，请及时保存正式文件".into());
            }
        } else if let Ok(mut slot) = self.recovery_error.lock() {
            *slot = None;
        }
    }

    /// 取出并清空最近一次恢复点写入失败提示（前端轮询）。
    pub fn take_recovery_error(&self) -> Option<String> {
        self.recovery_error.lock().ok().and_then(|mut slot| slot.take())
    }

    /// 退出请求开始时建立后端保存屏障：在用户作出选择前，任何自动保存都不得落盘。
    pub fn begin_quit_barrier(&self) {
        self.quit_barrier.store(true, Ordering::SeqCst);
    }

    /// 用户取消退出：解除保存屏障，恢复正常自动保存。
    pub fn clear_quit_barrier(&self) {
        self.quit_barrier.store(false, Ordering::SeqCst);
    }

    pub fn quit_barrier_active(&self) -> bool {
        self.quit_barrier.load(Ordering::SeqCst)
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
        let _saving=self.saving.lock().map_err(|e|e.to_string())?;
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
        // 恢复稿默认落成**未绑定**的新副本：绝不直接绑回原容器。
        // 原文件可能已经被更新的正式稿覆盖，直接绑定会让下一次保存把新稿冲掉。
        // 用户确认内容后通过「另存为」决定落到哪里，原文件在被明确覆盖前保持不动。
        self.saved_revision.store(0, Ordering::SeqCst);
        let ws = Workspace::blank(&rec.document.metadata.title)?;

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
        *self.project_lock.lock().unwrap() = None;
        self.reset_autosave_for_project();
        self.dirty.store(true, Ordering::SeqCst);
        // 先把恢复内容写成当前会话的恢复点，成功后才消费旧记录。
        // 写失败时旧记录保留，用户可重试；错误必须可见。
        if let Err(e) = self.write_recovery() {
            return Err(format!(
                "恢复内容已载入为未保存副本，但恢复点写入失败：{e}。请重试，或立即另存为。"
            ));
        }
        if let Ok(mut recovery) = self.recovery.lock() {
            recovery.consume_pending();
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
    ///
    /// 「保留恢复副本并关闭」留下的是一份独立会话记录，
    /// 这里的 `clear_current` 只清当前会话，不会动它。
    pub fn shutdown(&self) {
        if self.is_dirty() {
            let _ = self.write_recovery();
        } else if let Ok(mut recovery) = self.recovery.lock() {
            recovery.clear_current();
        }
    }

    // -----------------------------------------------------------------
    // 导入
    // -----------------------------------------------------------------
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
        let (document, original_source) = crate::read_import_with_source(&source_path).unwrap();
        svc.run_with_original_source(|s| s.load_document(Origin::Import, document), original_source).unwrap();
        svc.save(Some(&project_path)).unwrap();
        svc.open(&project_path).unwrap();
        assert_eq!(svc.require().unwrap().original_source().as_deref(), Some(original));

        svc.shutdown();
        let _ = std::fs::remove_file(source_path);
        let _ = std::fs::remove_file(project_path);
    }
}

#[cfg(test)] mod recovery_corruption_tests {
    use super::*;
    /// 原项目容器损坏时，恢复记录仍必须能打开；
    /// 且绝不能把恢复内容写回损坏文件，否则原稿失去抢修机会。
    #[test] fn corrupt_container_still_restores_pending_document() {
        let root = std::env::temp_dir().join(screenplay_core::ast::new_id("recover-corrupt-qa"));
        let dir = root.join("recovery");
        let path = root.join("broken.screenplay");
        std::fs::create_dir_all(&root).unwrap();

        let svc = ProjectService::with_recovery_directory(dir.clone());
        svc.create_new("损坏工程").unwrap();
        svc.run(|s| s.insert_block(Origin::GuiLeft, None, screenplay_core::ast::BlockType::Action, "已保存的正文", None)).unwrap();
        svc.save(Some(&path)).unwrap();
        svc.run(|s| s.insert_block(Origin::GuiLeft, None, screenplay_core::ast::BlockType::Action, "崩溃前未保存的正文", None)).unwrap();
        svc.shutdown();
        drop(svc);

        // 模拟崩溃后容器被外部程序写坏
        std::fs::write(&path, b"not a zip").unwrap();

        let svc2 = ProjectService::with_recovery_directory(dir.clone());
        let doc = svc2.adopt_recovery().expect("容器损坏时仍应能采纳恢复内容");
        assert!(doc.blocks.iter().any(|b| b.text == "崩溃前未保存的正文"), "恢复内容必须完整");
        assert_eq!(std::fs::read(&path).unwrap(), b"not a zip", "不得覆盖损坏的原文件");
        assert!(svc2.require().unwrap().container_path().is_none(), "损坏容器应作为未保存项目恢复，要求另存为");

        svc2.shutdown();
        std::fs::remove_dir_all(root).unwrap();
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

#[cfg(test)] mod stale_save_backup_service_tests {
    use super::*;
    /// 用户确认后：工程文件被写回，备份与归属记录被清理。
    #[test] fn confirmed_restore_writes_container_and_cleans_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("recover-me.screenplay");
        let svc = ProjectService::with_recovery_directory(dir.path().join("recovery"));
        svc.create_new("完整内容").unwrap();
        svc.save(Some(&path)).unwrap();
        let backup = screenplay_store::project::create_save_backup(&path).unwrap();
        std::fs::write(&path, b"truncated by kill").unwrap();

        svc.restore_stale_backup(&path).unwrap();

        let restored = screenplay_store::project::load_project(&path).unwrap();
        assert_eq!(restored.document.metadata.title, "完整内容");
        assert!(!backup.exists());
        assert!(!backup.with_extension("json").exists());
    }

    /// 当前工程有未保存修改时必须拒绝，不能拿磁盘备份冲掉内存里的新内容。
    #[test] fn restore_refuses_when_open_project_has_unsaved_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("editing.screenplay");
        let svc = ProjectService::with_recovery_directory(dir.path().join("recovery"));
        svc.create_new("已保存").unwrap();
        svc.run(|s| s.insert_block(Origin::GuiLeft, None, screenplay_core::ast::BlockType::Action, "原始内容", None)).unwrap();
        svc.save(Some(&path)).unwrap();
        let backup = screenplay_store::project::create_save_backup(&path).unwrap();
        let before = std::fs::read(&path).unwrap();
        let block = svc.document().unwrap().blocks[0].id.clone();
        svc.run(|s| s.replace_block(Origin::GuiLeft, &block, "未保存的新内容", None)).unwrap();
        assert!(svc.is_dirty());

        let error = svc.restore_stale_backup(&path).unwrap_err();
        assert!(error.contains("未保存"), "错误信息：{error}");
        assert_eq!(std::fs::read(&path).unwrap(), before, "拒绝时不得改动工程文件");
        assert!(backup.exists(), "拒绝时不得删除备份");
        assert_eq!(svc.document().unwrap().blocks[0].text, "未保存的新内容");

        let _ = std::fs::remove_file(&backup);
        let _ = std::fs::remove_file(backup.with_extension("json"));
    }

    /// 工程文件本身完好时，旧备份不提示、也不删除（避免误导用户退回旧版本）。
    #[test] fn completed_save_is_not_offered_as_stale_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("finished.screenplay");
        let svc = ProjectService::with_recovery_directory(dir.path().join("recovery"));
        svc.create_new("完整版本").unwrap();
        svc.save(Some(&path)).unwrap();
        let backup = screenplay_store::project::create_save_backup(&path).unwrap();

        assert!(svc.stale_save_backup(&path).is_none(), "工程可正常打开时不应提示恢复");
        assert!(backup.exists(), "不提示时也不得删除用户的备份");

        let _ = std::fs::remove_file(&backup);
        let _ = std::fs::remove_file(backup.with_extension("json"));
    }

    /// 用户拒绝后：备份保留（只改名），不再被启动检查列出。
    #[test] fn dismiss_keeps_data_but_stops_prompting() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("declined.screenplay");
        let svc = ProjectService::with_recovery_directory(dir.path().join("recovery"));
        svc.create_new("待定").unwrap();
        svc.save(Some(&path)).unwrap();
        let backup = screenplay_store::project::create_save_backup(&path).unwrap();

        assert!(svc.dismiss_stale_backup(&path).unwrap());
        assert!(screenplay_store::project::find_stale_save_backup(&path).is_none());
        assert_eq!(std::fs::read(format!("{}.declined", backup.display())).unwrap(), std::fs::read(&path).unwrap());

        let _ = std::fs::remove_file(format!("{}.declined", backup.display()));
        let _ = std::fs::remove_file(format!("{}.declined", backup.with_extension("json").display()));
    }
}

#[cfg(test)]mod lock_and_save_contract_tests{
 use super::*;
 #[test]fn gui_open_and_save_as_respect_existing_process_locks(){let dir=tempfile::tempdir().unwrap();let file=dir.path().join("occupied.screenplay");let ws=Workspace::blank("原稿").unwrap();ws.save(Some(&file)).unwrap();let original=std::fs::read(&file).unwrap();let _held=screenplay_store::bridge::ProjectLock::acquire(&file).unwrap();let svc=ProjectService::new();svc.create_new("新稿").unwrap();assert!(svc.open(&file).is_err());assert_eq!(svc.document().unwrap().metadata.title,"新稿");assert!(svc.save(Some(&file)).is_err());assert_eq!(std::fs::read(&file).unwrap(),original);}
 #[test]fn gui_keeps_lock_until_close_and_can_reopen_own_project(){let dir=tempfile::tempdir().unwrap();let file=dir.path().join("owned.screenplay");let svc=ProjectService::new();svc.create_new("测试").unwrap();svc.save(Some(&file)).unwrap();assert!(screenplay_store::bridge::ProjectLock::acquire(&file).is_err());svc.open(&file).unwrap();svc.revert().unwrap();svc.close().unwrap();assert!(screenplay_store::bridge::ProjectLock::acquire(&file).is_ok());}
 #[test]fn save_and_autosave_reject_the_wrong_project_identity(){let dir=tempfile::tempdir().unwrap();let file=dir.path().join("wrong.screenplay");let svc=ProjectService::new();svc.create_new("当前工程").unwrap();assert!(svc.save_checked(Some(&file),Some("old-project")).is_err());assert!(!file.exists());assert!(svc.autosave_tick_checked(Some("old-project")).unwrap().is_none());}
}

#[cfg(test)] mod recovery_retention_tests {
 use super::*;

 fn write_line(svc:&ProjectService,text:&str){
   svc.run(|s|s.insert_block(Origin::GuiLeft,None,screenplay_core::ast::BlockType::Action,text,None)).unwrap();
 }

 /// 「保留恢复副本并关闭」→ 正常退出，副本必须还在，且下次启动可恢复。
 #[test] fn retained_copy_survives_normal_exit() {
   let dir=tempfile::tempdir().unwrap();
   let recovery=dir.path().join("recovery");
   let svc=ProjectService::with_recovery_directory(recovery.clone());
   svc.create_new("保留副本").unwrap();
   write_line(&svc,"必须保留的未保存正文");
   assert!(svc.is_dirty());
   svc.close().unwrap();
   svc.shutdown(); // 模拟正常退出
   let mgr=RecoveryManager::in_directory(recovery);
   let rec=mgr.pending().expect("保留副本在正常退出后必须还在");
   assert!(rec.retained,"留下的应是用户主动保留的副本");
   assert!(rec.document.blocks.iter().any(|b|b.text=="必须保留的未保存正文"));
 }

 /// 已保存且无未保存修改时关闭，不应留下会弹「发现未保存内容」的记录。
 #[test] fn clean_close_leaves_no_recovery_prompt() {
   let dir=tempfile::tempdir().unwrap();
   let recovery=dir.path().join("recovery");
   let svc=ProjectService::with_recovery_directory(recovery.clone());
   let path=dir.path().join("clean.screenplay");
   svc.create_new("干净工程").unwrap();
   svc.save(Some(&path)).unwrap();
   svc.close().unwrap();
   svc.shutdown();
   assert!(RecoveryManager::in_directory(recovery).pending().is_none());
 }

 /// 退出请求开始后，后端保存屏障必须拦住自动保存；取消退出后恢复。
 #[test] fn quit_barrier_blocks_autosave_until_cancelled() {
   let dir=tempfile::tempdir().unwrap();
   let svc=ProjectService::with_recovery_directory(dir.path().join("recovery"));
   let path=dir.path().join("barrier.screenplay");
   svc.create_new("屏障").unwrap();
   svc.save(Some(&path)).unwrap();
   svc.configure_autosave(true,1,0);
   write_line(&svc,"退出前的修改");
   let later=Instant::now()+Duration::from_secs(3600);
   svc.begin_quit_barrier();
   assert!(svc.autosave_tick_at(later).unwrap().is_none(),"退出确认期间不得自动保存");
   assert!(svc.is_dirty());
   svc.clear_quit_barrier();
   assert_eq!(svc.autosave_tick_at(later).unwrap(),Some(AutosaveAction::Saved),"取消退出后应恢复正常自动保存");
   assert!(!svc.is_dirty());
 }
}
