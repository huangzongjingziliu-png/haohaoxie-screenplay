//! Tauri IPC 命令层 —— 薄壳。
//!
//! 铁律（说明书第 82 条）：
//! 1. 本文件不含任何业务逻辑，只做「参数解码 → ProjectService 调用 → 编码返回」。
//! 2. 任何修改都必须经过 `CommandService`，前端不得持有可写文档副本。
//! 3. 保存不接受前端传入的文档 JSON，只接受路径。

pub mod bridge_host;
pub mod diag;
mod connection;
mod updates;
mod help;
mod series;
mod continuity;
mod agent_config;
mod model_profiles;
mod agent_text;
mod agent_harness;
mod credentials;
pub mod service;

use crate::service::{
    AutosaveAction, AutosaveConfig, PendingRecovery, ProjectService, ProjectState, SaveOutcome,
};
use screenplay_core::ast::{BlockType, ScreenplayDocument};
use screenplay_store::bridge::{
    BridgeError, BridgeRequest, BridgeResponse, BridgeServer,
};
use screenplay_store::command::{CommandError, CommandResult};
use screenplay_store::db::{AuditEntry, Branch, Origin, VersionRecord};
use screenplay_agent::{AgentRunner, ModelRouter, SkillRegistry};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

// Finder open events can arrive before the webview subscribes. Keep requests until polled.
static FILE_OPEN_REQUESTS: Mutex<Vec<String>> = Mutex::new(Vec::new());

#[tauri::command]
fn take_file_open_requests() -> Vec<String> {
    std::mem::take(&mut *FILE_OPEN_REQUESTS.lock().unwrap())
}

// ---------------------------------------------------------------------------
// 统一错误返回：结构化，不手工拼接
// ---------------------------------------------------------------------------

/// IPC 错误：直接用 CommandError 的 serde 形态，不再双重编码成字符串
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "error", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IpcError {
    #[serde(rename = "MODEL_REQUIRED")]
    ModelRequired { message: String },
    #[serde(rename = "REVISION_CONFLICT")]
    RevisionConflict {
        expected: u64,
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
    /// 应用级错误（未打开项目、保存失败等）
    #[serde(rename = "APP_STATE")]
    AppState { message: String },
}

impl From<CommandError> for IpcError {
    fn from(e: CommandError) -> Self {
        match e {
            CommandError::RevisionConflict {
                expected,
                actual,
                message,
            } => IpcError::RevisionConflict {
                expected,
                actual,
                message,
            },
            CommandError::NotFound { what, id } => IpcError::NotFound { what, id },
            CommandError::InvalidArgument { message } => IpcError::InvalidArgument { message },
            CommandError::BlockLocked { id } => IpcError::BlockLocked { id },
            CommandError::Internal { message } => IpcError::Internal { message },
        }
    }
}

impl From<String> for IpcError {
    fn from(message: String) -> Self {
        IpcError::AppState { message }
    }
}

pub type IpcResult<T> = Result<T, IpcError>;

// ---------------------------------------------------------------------------
// 应用状态
// ---------------------------------------------------------------------------

pub struct AppState {
    pub quitting: std::sync::atomic::AtomicBool,
    pub imports: Mutex<std::collections::HashMap<String, screenplay_import::ImportControl>>,
    pub project: Arc<ProjectService>,
    /// 本地桥接服务：CLI / MCP 通过它操作本进程持有的权威项目。
    /// `None` 表示桥接启动失败（此时 CLI/MCP 回退独立进程模式 + 文件锁）。
    pub bridge: Mutex<Option<Arc<BridgeServer>>>,
    /// Agent 配置：云 AI 开关 + Provider 配置
    pub agent: Mutex<AgentConfig>,
}

/// Agent 运行配置
#[derive(Clone)]
pub struct AgentConfig {
    pub cloud_ai_enabled: bool,
    pub provider_kind: String,   // "echo" | "openai" | "ollama" | "compatible"
    pub api_key: String,
    pub model: String,
    pub base_url: String,
}

impl Default for AgentConfig {
    fn default() -> Self {
        let settings = agent_config::ProviderSettings::default();
        Self { cloud_ai_enabled: settings.cloud_ai_enabled, provider_kind: settings.provider_kind,
            api_key: String::new(), model: settings.model, base_url: settings.base_url }
    }
}
impl AgentConfig {
    fn load() -> Self {
        let settings = agent_config::load();
        // Keychain authorization must never delay the first writing window.
        if let Some(mut cfg) = model_profiles::active_config() { cfg.cloud_ai_enabled = settings.cloud_ai_enabled; return cfg; }
        let api_key = String::new();
        Self { cloud_ai_enabled: settings.cloud_ai_enabled, provider_kind: settings.provider_kind,
            api_key, model: settings.model, base_url: settings.base_url }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            quitting: std::sync::atomic::AtomicBool::new(false),
            imports: Mutex::new(std::collections::HashMap::new()),
            project: Arc::new(ProjectService::new()),
            bridge: Mutex::new(None),
            agent: Mutex::new(AgentConfig::load()),
        }
    }
}

impl AppState {
    /// 项目切换 / 另存为后刷新桥接描述文件，
    /// 让外部进程知道当前操作对象已经变了。
    pub fn refresh_bridge_descriptor(&self) {
        if let Some(b) = self.bridge.lock().unwrap().as_ref() {
            let st = self.project.state();
            b.update_project(st.path().map(|p| p.to_path_buf()), st.title());
        }
    }

    /// 主动清理桥接产物（socket + 描述文件）。
    ///
    /// 为什么不能只靠 `Drop`：进程被 SIGTERM / SIGKILL 杀掉时
    /// 不会运行析构函数，残留的 `bridge.json` 会让 CLI/MCP 误判
    /// 「GUI 还在运行」并反复尝试连接一个死 socket。
    pub fn cleanup_bridge(&self) {
        // 先取出（触发 BridgeServer 的 Drop：删 socket 与描述文件）
        let taken = self.bridge.lock().unwrap().take();
        drop(taken);
        // 双保险：即使 BridgeServer 已被 drop，也确保文件不在
        if let Some(p) = screenplay_store::bridge::descriptor_path() {
            let _ = std::fs::remove_file(p);
        }
        diag::log("桥接产物已清理");
    }
}

/// 变更广播载荷：前端据此判断是否需要重绘
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeNotice {
    pub revision: u64,
    pub origin: String,
    /// 受影响 block（空表示全量）
    pub affected: Vec<String>,
    pub dirty: bool,
    pub at: String,
}

fn broadcast(app: &AppHandle, origin: &str, result: &CommandResult) {
    diag::command(
        &format!("broadcast:{origin}"),
        &format!("affected={:?}", result.affected),
        &format!("rev={} dirty={}", result.revision, result.document.revision),
    );
    let svc = app.state::<AppState>();
    let notice = ChangeNotice {
        revision: result.revision,
        origin: origin.to_string(),
        affected: result.affected.clone(),
        dirty: svc.project.is_dirty(),
        at: chrono::Utc::now().to_rfc3339(),
    };
    let _ = app.emit("document-changed", notice);
}

/// 外部（CLI / MCP / Agent）写入后的广播。
///
/// 与 GUI 内部写入走同一个事件，前端因此不需要区分来源，
/// 收到 `document-changed` 就重新拉取后端权威状态。
pub fn broadcast_external(app: &AppHandle, result: &CommandResult) {
    diag::command(
        "broadcast:external",
        &format!("affected={:?}", result.affected),
        &format!("rev={}", result.revision),
    );
    broadcast(app, "external", result);
    // 项目身份可能变化（另存为），额外广播 project-changed
    let svc = app.state::<AppState>();
    let info = project_info(&svc.project);
    let _ = app.emit("project-changed", info);
}

/// 广播项目状态变化（供 GUI 前端与外部工具感知）
pub fn emit_project_changed(app: &AppHandle, info: &ProjectInfo) {
    let _ = app.emit("project-changed", info);
}

/// 刷新桥接描述文件（项目切换 / 另存为后调用）。
///
/// 让外部进程立刻知道「当前操作对象已变」，避免它们对着旧项目操作。
pub fn refresh_bridge_descriptor(app: &AppHandle, project: &ProjectService) {
    app.state::<AppState>().refresh_bridge_descriptor();
    let st = project.state();
    diag::log(&format!(
        "桥接描述已刷新: title={} path={}",
        st.title(),
        st.path().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| "<未保存>".into())
    ));
}

/// 读取并解析外部文件为文档（GUI 导入与桥接导入共用）。
///
/// 共用保证：GUI 点「导入」和 MCP 调 `import_file` 走**完全相同**的
/// 解析路径，不会出现两条导入实现产生不同 AST 的情况。
pub fn read_import(path: &std::path::Path) -> Result<ScreenplayDocument, BridgeError> {
    read_import_with_source_controlled(path, &screenplay_import::ImportControl::default())
        .map(|(document, _)| document)
}

/// 读取并解析导入文件，同时保留其原始字节供项目容器归档。
pub fn read_import_with_source(path: &std::path::Path) -> Result<(ScreenplayDocument, Vec<u8>), BridgeError> {
    read_import_with_source_controlled(path, &screenplay_import::ImportControl::default())
}

fn read_import_with_source_controlled(path: &std::path::Path, control: &screenplay_import::ImportControl) -> Result<(ScreenplayDocument, Vec<u8>), BridgeError> {
    screenplay_import::import_path_with_source_controlled(path, None, control)
        .map(|(outcome, bytes)| (outcome.document, bytes))
        .map_err(|error| {
            if let Some(io) = error.downcast_ref::<std::io::Error>() {
                BridgeError::NotFound { what: "文件".into(), id: format!("{}: {io}", path.display()) }
            } else {
                BridgeError::InvalidArgument { message: error.to_string() }
            }
        })
}

// ---------------------------------------------------------------------------
// 跨进程桥接：让 CLI / MCP 操作 GUI 当前打开的项目（说明书第 82 条）
// ---------------------------------------------------------------------------

/// 安装 SIGTERM / SIGHUP / SIGINT 处理器：先清理桥接产物再退出。
///
/// 为什么需要：Rust 不会为这些信号运行析构函数。若不显式清理，
/// `bridge.json` 与 socket 会残留，外部进程会误判「GUI 还在运行」
/// 并反复连接一个死 socket。
///
/// 安全说明：处理器内只做 unlink 类操作与 `raise`，
/// 不分配内存、不加锁，符合异步信号安全要求。
extern "C" fn on_terminating_signal(sig: i32) {
    // 注意：这里不能用 `format!`（会分配内存）。
    // 直接用 C 字符串拼接描述文件路径。
    if let Some(dir) = screenplay_store::bridge::app_support_dir() {
        let mut p = dir.as_os_str().as_encoded_bytes().to_vec();
        p.extend_from_slice(b"/bridge.json");
        // SAFETY: p 以 NUL 结尾的 CString 在下方构造
        if let Ok(c) = std::ffi::CString::new(p) {
            unsafe { libc::unlink(c.as_ptr()) };
        }
        // socket 名含 pid，需拼接；此处只清理常见长度（pid ≤ 10 位）
        let pid = std::process::id();
        let mut sp = dir.as_os_str().as_encoded_bytes().to_vec();
        sp.extend_from_slice(b"/bridge-");
        sp.extend_from_slice(pid.to_string().as_bytes());
        sp.extend_from_slice(b".sock");
        if let Ok(c) = std::ffi::CString::new(sp) {
            unsafe { libc::unlink(c.as_ptr()) };
        }
    }
    // 恢复默认处理并重新抛出，让进程以正确状态码退出
    unsafe {
        libc::signal(sig, libc::SIG_DFL);
        libc::raise(sig);
    }
}

fn install_signal_handlers() {
    for sig in [libc::SIGTERM, libc::SIGHUP, libc::SIGINT] {
        unsafe {
            libc::signal(sig, on_terminating_signal as libc::sighandler_t);
        }
    }
}

/// 启动本地桥接服务：CLI / MCP 通过它操作 GUI 当前打开的项目。
///
/// 关键设计：
/// - socket 轮询在**独立线程**，通过 Tauri 的 `run_on_main_thread` 把命令
///   派发到主线程执行，保证与 GUI 写入串行化（不会并发改同一文档）。
/// - 桥接失败绝不影响 GUI 主流程：任何错误只记日志。
pub fn start_bridge(app: &AppHandle) -> Option<Arc<BridgeServer>> {
    install_signal_handlers();
    let server = match BridgeServer::start(None, "") {
        Ok(s) => Arc::new(s),
        Err(e) => {
            diag::log(&format!("桥接启动失败（CLI/MCP 将回退独立模式）: {}", e.message()));
            return None;
        }
    };
    diag::log(&format!(
        "桥接已启动: {}",
        server.socket_path().to_string_lossy()
    ));

    let srv = server.clone();
    let handle = app.clone();
    std::thread::Builder::new()
        .name("screenplay-bridge".into())
        .spawn(move || bridge_loop(srv, handle))
        .ok()?;
    Some(server)
}

fn bridge_loop(server: Arc<BridgeServer>, app: AppHandle) {
    loop {
        // 非阻塞 accept + 短暂休眠，避免空转烧 CPU
        match server.try_accept() {
            Some(Ok(conn)) => {
                // 连接放到 Arc<Mutex<_>>：主线程回调需要 'static 所有权，
                // 而本线程还要继续读下一条请求。
                let conn = Arc::new(Mutex::new(conn));
                let shared = conn.clone();
                let handle = app.clone();
                std::thread::Builder::new()
                    .name("screenplay-bridge-conn".into())
                    .spawn(move || serve_connection(shared, handle))
                    .ok();
                // 每个连接一个线程：MCP/CLI 是低频请求，
                // 线程模型比在 accept 循环里同步等待更简单也更安全。
                continue;
            }
            Some(Err(e)) => {
                diag::log(&format!("桥接 accept 失败: {e}"));
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            None => std::thread::sleep(std::time::Duration::from_millis(15)),
        }
    }
}

/// 处理一条连接上的多个请求（keep-alive）
fn serve_connection(
    conn: Arc<Mutex<screenplay_store::bridge::BridgeConnection>>,
    app: AppHandle,
) {
    use screenplay_store::bridge::ReadOutcome;
    diag::log("桥接：新连接线程启动");
    let mut served = 0u32;
    loop {
        let outcome = {
            let mut guard = match conn.lock() {
                Ok(g) => g,
                Err(_) => {
                    diag::log("桥接：连接锁已损坏");
                    return;
                }
            };
            guard.read_request_detailed()
        };
        let req = match outcome {
            ReadOutcome::Request(r) => *r,
            ReadOutcome::Eof => {
                diag::log(&format!("桥接：连接结束（已服务 {served} 个请求）"));
                return;
            }
            ReadOutcome::WouldBlock => {
                // 暂时无数据：**不是断开**。稍等后重试。
                std::thread::sleep(std::time::Duration::from_millis(20));
                continue;
            }
            ReadOutcome::BadRequest => {
                // 已回错误响应，继续服务下一条
                continue;
            }
            ReadOutcome::Io(e) => {
                diag::log(&format!("桥接：IO 错误（{e}），关闭连接"));
                return;
            }
        };

        // 在主线程执行：与 GUI 写入串行，避免并发改同一文档
        let resp = if let BridgeRequest::ImportFile {path}=&req {
            if agent_text::complex(path) {
                // Slow OCR/model I/O must never occupy the macOS UI thread.
                let state=app.state::<AppState>();
                match tauri::async_runtime::block_on(import_file(app.clone(),state,path.clone(),None)) {
                    Ok(result)=>BridgeResponse::ok(serde_json::to_value(&result).unwrap_or(serde_json::Value::Null),result.revision),
                    Err(error)=>BridgeResponse::fail(BridgeError::InvalidArgument{message:match error {IpcError::Internal{message}|IpcError::InvalidArgument{message}|IpcError::AppState{message}=>message,other=>format!("{other:?}")}},app.state::<AppState>().project.revision()),
                }
            } else {handle_on_main_thread(&app,req)}
        } else {handle_on_main_thread(&app,req)};
        served += 1;
        let write_result = {
            let mut guard = match conn.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            guard.write_response(&resp)
        };
        match write_result {
            Ok(()) => diag::log(&format!("桥接：第 {served} 个响应已写出")),
            Err(e) => {
                diag::log(&format!("桥接：写响应失败（{e}），关闭连接"));
                return;
            }
        }
    }
}

/// 把请求派发到 Tauri 主线程执行并等待结果
fn handle_on_main_thread(
    app: &AppHandle,
    req: screenplay_store::bridge::BridgeRequest,
) -> screenplay_store::bridge::BridgeResponse {
    use screenplay_store::bridge::BridgeError;
    let tag = format!("{req:?}");
    let tag = tag.split_whitespace().next().unwrap_or("?").to_string();
    let slot: Arc<Mutex<Option<screenplay_store::bridge::BridgeResponse>>> =
        Arc::new(Mutex::new(None));
    let slot2 = slot.clone();
    let handle = app.clone();
    let dispatched = app.run_on_main_thread(move || {
        let resp =
            bridge_host::handle(&handle, &handle.state::<AppState>().project, req);
        *slot2.lock().unwrap() = Some(resp);
    });
    if dispatched.is_err() {
        diag::log(&format!("桥接：主线程已退出（op={tag}）"));
        return BridgeResponse::fail(
            BridgeError::AppState {
                message: "主线程已退出，无法处理请求".into(),
            },
            0,
        );
    }
    // 等主线程处理完（桥接是低频操作，简单同步即可）
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        if let Some(resp) = slot.lock().unwrap().take() {
            diag::log(&format!(
                "桥接：op={tag} ok={} rev={}",
                resp.ok, resp.revision
            ));
            return resp;
        }
        if std::time::Instant::now() > deadline {
            diag::log(&format!("桥接：op={tag} 主线程 60 秒未响应"));
            return BridgeResponse::fail(
                BridgeError::BridgeUnavailable {
                    message: "主线程 60 秒未响应".into(),
                },
                0,
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

// ---------------------------------------------------------------------------
// 项目状态 DTO
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub state: String,
    pub path: Option<String>,
    pub title: String,
    pub revision: u64,
    pub dirty: bool,
    pub block_count: usize,
}

pub fn project_info(svc: &ProjectService) -> ProjectInfo {
    let st = svc.state();
    let doc_count = svc.document().map(|d| d.blocks.len()).unwrap_or(0);
    let state = match st {
        ProjectState::Empty => "empty".to_string(),
        ProjectState::Unsaved { .. } => "unsaved".to_string(),
        ProjectState::Open { .. } => "open".to_string(),
    };
    ProjectInfo {
        state,
        path: st.path().map(|p| p.to_string_lossy().to_string()),
        title: st.title().to_string(),
        revision: svc.revision(),
        dirty: svc.is_dirty(),
        block_count: doc_count,
    }
}

/// 前端启动时的完整状态快照（避免多次 IPC 往返）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub project: ProjectInfo,
    #[serde(serialize_with = "serialize_bootstrap_document")]
    pub document: Option<ScreenplayDocument>,
    pub source_text: String,
    pub recent: Vec<RecentEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentEntry {
    pub path: String,
    pub title: String,
    pub opened_at: String,
}

fn serialize_bootstrap_document<S:serde::Serializer>(document:&Option<ScreenplayDocument>,serializer:S)->Result<S::Ok,S::Error>{document.as_ref().map(screenplay_core::series::ui_snapshot).serialize(serializer)}

fn recent_file_path() -> PathBuf {
    let mut p = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push(".screenplay-recent.json");
    p
}

fn load_recent() -> Vec<RecentEntry> {
    std::fs::read_to_string(recent_file_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_recent(list: &[RecentEntry]) {
    if let Ok(s) = serde_json::to_string(list) {
        let _ = std::fs::write(recent_file_path(), s);
    }
}

fn push_recent(path: &str, title: &str) {
    let mut list = load_recent();
    list.retain(|e| e.path != path);
    list.insert(
        0,
        RecentEntry {
            path: path.to_string(),
            title: title.to_string(),
            opened_at: chrono::Utc::now().to_rfc3339(),
        },
    );
    list.truncate(10);
    save_recent(&list);
}

// ---------------------------------------------------------------------------
// 项目生命周期命令
// ---------------------------------------------------------------------------

/// 新建空白项目。
/// **保证**：调用后后端 Workspace 一定是新项目，不会继续写入旧项目。
#[tauri::command]
fn new_project(
    app: AppHandle,
    state: State<'_, AppState>,
    title: String,
) -> IpcResult<ProjectInfo> {
    state.project.create_new(&title)?;
    diag::log("new_project title={title}");
    state.refresh_bridge_descriptor();
    let info = project_info(&state.project);
    let _ = app.emit("project-changed", &info);
    Ok(info)
}

/// 打开已有项目
#[tauri::command]
fn open_project(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> IpcResult<Bootstrap> {
    let p = PathBuf::from(&path);
    state.project.open(&p)?;
    diag::log("open_project path={path}");
    state.refresh_bridge_descriptor();
    let info = project_info(&state.project);
    push_recent(&path, &info.title);
    let bootstrap = build_bootstrap(&state.project);
    let _ = app.emit("project-changed", &info);
    Ok(bootstrap)
}

/// 关闭当前项目，回到欢迎页
#[tauri::command]
fn close_project(app: AppHandle, state: State<'_, AppState>, discard_changes: Option<bool>) -> IpcResult<ProjectInfo> {
    if discard_changes.unwrap_or(false) { state.project.close_without_saving()?; } else { state.project.close()?; }
    diag::log("close_project");
    state.refresh_bridge_descriptor();
    let info = project_info(&state.project);
    let _ = app.emit("project-changed", &info);
    Ok(info)
}

/// 保存到当前绑定路径。
/// **不接受文档 JSON** —— 保存的是后端权威状态。
#[tauri::command]
fn save_project(app: AppHandle, state: State<'_, AppState>, project_id:Option<String>) -> IpcResult<SaveOutcome> {
    let outcome = state.project.save_checked(None,project_id.as_deref())?;
    diag::log("save_project");
    state.refresh_bridge_descriptor();
    let title = state.project.state().title().to_string();
    push_recent(&outcome.path, &title);
    let _ = app.emit("project-saved", &outcome);
    Ok(outcome)
}

/// 另存为
#[tauri::command]
fn save_project_as(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    project_id:Option<String>,
) -> IpcResult<SaveOutcome> {
    let outcome = state.project.save_checked(Some(&PathBuf::from(&path)),project_id.as_deref())?;
    diag::log("save_project_as path={path}");
    state.refresh_bridge_descriptor();
    let title = state.project.state().title().to_string();
    push_recent(&outcome.path, &title);
    let _ = app.emit("project-saved", &outcome);
    Ok(outcome)
}

/// 放弃未保存修改
#[tauri::command]
fn revert_project(state: State<'_, AppState>) -> IpcResult<Bootstrap> {
    state.project.revert()?;
    Ok(build_bootstrap(&state.project))
}

fn build_bootstrap(svc: &ProjectService) -> Bootstrap {
    let (document, source_text) = match svc.document() {
        Ok(doc) => {
            let text = screenplay_core::serializer::to_compact_source(&doc);
            (Some(doc), text)
        }
        Err(_) => (None, String::new()),
    };
    Bootstrap {
        project: project_info(svc),
        document,
        source_text,
        recent: load_recent(),
    }
}

/// 前端启动时拉取完整状态
#[tauri::command]
fn bootstrap(state: State<'_, AppState>) -> IpcResult<Bootstrap> {
    Ok(build_bootstrap(&state.project))
}

/// 当前项目信息（轻量轮询用）
#[tauri::command]
fn project_info_cmd(state: State<'_, AppState>) -> IpcResult<ProjectInfo> {
    Ok(project_info(&state.project))
}

// ---------------------------------------------------------------------------
// 文档读取
// ---------------------------------------------------------------------------

#[tauri::command]
fn get_document(state: State<'_, AppState>) -> IpcResult<ScreenplayDocument> {
    Ok(screenplay_core::series::ui_snapshot(&state.project.document()?))
}

#[tauri::command]
fn get_source_text(state: State<'_, AppState>) -> IpcResult<String> {
    let doc = state.project.document()?;
    Ok(screenplay_core::serializer::to_compact_source(&doc))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub block_id: String,
    pub block_index: usize,
    pub block_type: String,
    pub text: String,
    /// 命中字符在 text 中的位置
    pub start: usize,
    pub end: usize,
    pub scene_id: Option<String>,
}

#[tauri::command]
fn search(
    state: State<'_, AppState>,
    query: String,
    case_sensitive: bool,
) -> IpcResult<Vec<SearchHit>> {
    let doc = state.project.document()?;
    let q = if case_sensitive {
        query.clone()
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
        if let Some(start) = hay.find(&q) {
            hits.push(SearchHit {
                block_id: b.id.clone(),
                block_index: i,
                block_type: b.block_type.as_str().to_string(),
                text: b.text.clone(),
                start,
                end: start + query.chars().count(),
                scene_id: b.scene_id.clone(),
            });
        }
    }
    Ok(hits)
}

// ---------------------------------------------------------------------------
// 文档修改（全部经 CommandService）
// ---------------------------------------------------------------------------

#[tauri::command]
fn replace_block(
    app: AppHandle,
    state: State<'_, AppState>,
    block_id: String,
    text: String,
    expected_revision: Option<u64>,
) -> IpcResult<CommandResult> {
    let r = state.project.run(|svc| {
        svc.replace_block(Origin::GuiLeft, &block_id, &text, expected_revision)
    })?;
    broadcast(&app, "gui-left", &r);
    Ok(r)
}

#[tauri::command]
fn insert_block(
    app: AppHandle,
    state: State<'_, AppState>,
    after_id: Option<String>,
    block_type: String,
    text: String,
    expected_revision: Option<u64>,
) -> IpcResult<CommandResult> {
    let bt = parse_block_type(&block_type)?;
    let r = state.project.run(|svc| {
        svc.insert_block(
            Origin::GuiLeft,
            after_id.as_deref(),
            bt,
            &text,
            expected_revision,
        )
    })?;
    broadcast(&app, "gui-left", &r);
    Ok(r)
}

#[tauri::command]
fn delete_block(
    app: AppHandle,
    state: State<'_, AppState>,
    block_id: String,
    expected_revision: Option<u64>,
) -> IpcResult<CommandResult> {
    let r =
        state.project.run(|svc| svc.delete_block(Origin::GuiLeft, &block_id, expected_revision))?;
    broadcast(&app, "gui-left", &r);
    Ok(r)
}

#[tauri::command]
fn reorder_scenes(app: AppHandle, state: State<'_, AppState>, scene_ids: Vec<String>, expected_revision: Option<u64>) -> IpcResult<CommandResult> {
    let r = state.project.run(|svc| svc.reorder_scenes(Origin::GuiLeft, &scene_ids, expected_revision))?;
    broadcast(&app, "gui-left", &r); Ok(r)
}

#[tauri::command]
fn create_board_scene(app: AppHandle, state: State<'_, AppState>, title: String, summary: String, expected_revision: u64) -> IpcResult<CommandResult> {
    let r = state.project.run(|svc| svc.create_board_scene(Origin::GuiLeft, &title, &summary, expected_revision))?;
    broadcast(&app, "gui-left", &r);
    Ok(r)
}

#[tauri::command]
fn update_scene_board(app: AppHandle, state: State<'_, AppState>, board: screenplay_core::ast::SceneBoard, expected_revision: u64, scene_ids: Option<Vec<String>>) -> IpcResult<CommandResult> {
    fn color(v: &str) -> bool { v.len() == 7 && v.starts_with('#') && v[1..].bytes().all(|b| b.is_ascii_hexdigit()) }
    if !color(&board.color) || board.cards.len() > 10000 || board.cards.values().any(|c| (!c.color.is_empty() && !color(&c.color)) || !c.width.is_finite() || !c.height.is_finite() || (c.width != 0.0 && !(160.0..=640.0).contains(&c.width)) || (c.height != 0.0 && !(160.0..=640.0).contains(&c.height)) || c.summary.chars().count() > 1000 || c.summary_source.len() > 2000000 || c.comments.len()>1000 || c.comments.iter().any(|s|s.chars().count()>10000)) {
        return Err(IpcError::InvalidArgument { message: "便签设置无效".into() });
    }
    if board.groups.len()>10000 || board.links.len()>20000 || board.groups.iter().any(|g|g.id.is_empty()||g.name.chars().count()>200||g.scene_ids.len()>10000) || board.links.iter().any(|l|l.id.is_empty()||l.label.chars().count()>1000||!matches!(l.arrow.as_str(),"end"|"start"|"both"|"none")) {return Err(IpcError::InvalidArgument{message:"成组或连线设置无效".into()});}
    let r = state.project.run(|svc| {
        svc.check_revision(Some(expected_revision))?;
        let current=svc.get_document().document;
        let ids=scene_ids.unwrap_or_else(||current.blocks.iter().filter(|b|b.block_type==screenplay_core::ast::BlockType::SceneHeading).map(|b|b.id.clone()).collect());
        svc.reorder_scenes_with_board(Origin::GuiLeft,&ids,Some(expected_revision),Some(board))
    })?;
    broadcast(&app, "gui-left", &r); Ok(r)
}

/// 修改 block 元素类型（保留稳定 ID 与源文本范围）
#[tauri::command]
fn set_block_type(
    app: AppHandle,
    state: State<'_, AppState>,
    block_id: String,
    block_type: String,
    expected_revision: Option<u64>,
) -> IpcResult<CommandResult> {
    let bt = parse_block_type(&block_type)?;
    let r = state
        .project
        .run(|svc| svc.set_block_type(Origin::GuiLeft, &block_id, bt, expected_revision))?;
    broadcast(&app, "gui-left", &r);
    Ok(r)
}

/// 块首退格合并：把 `second_id` 合并进 `first_id`
#[tauri::command]
fn merge_blocks(
    app: AppHandle,
    state: State<'_, AppState>,
    first_id: String,
    second_id: String,
    expected_revision: Option<u64>,
) -> IpcResult<CommandResult> {
    let r = state.project.run(|svc| {
        svc.merge_blocks(Origin::GuiLeft, &first_id, &second_id, expected_revision)
    })?;
    broadcast(&app, "gui-left", &r);
    Ok(r)
}

/// 右侧自由文本整体替换
#[tauri::command]
fn replace_source_text(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
    expected_revision: Option<u64>,
) -> IpcResult<CommandResult> {
    let r = state.project.run(|svc| {
        svc.replace_source_text(Origin::GuiRight, &text, expected_revision)
    })?;
    broadcast(&app, "gui-right", &r);
    Ok(r)
}

/// 分页结果（GUI 纸面渲染与 PDF 导出共用同一份 Rust 计算结果）
#[tauri::command]
fn get_pagination(state: State<'_, AppState>) -> IpcResult<screenplay_core::pagination::Pagination> {
    let doc = state.project.document()?;
    Ok(screenplay_core::pagination::paginate(&doc))
}

/// 文档统计（字数、场景数、人物数等）
#[tauri::command]
fn document_stats(state: State<'_, AppState>) -> IpcResult<screenplay_core::ast::DocumentStats> {
    let doc = state.project.document()?;
    Ok(doc.stats())
}

/// 序列化到 Fountain（GUI 预览/复制用，与 Rust 导出同一实现）
#[tauri::command]
fn to_fountain(state: State<'_, AppState>) -> IpcResult<String> {
    let doc = state.project.document()?;
    Ok(screenplay_core::serializer::to_fountain(&doc))
}

/// 更新文档设置（纸张、字号等）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub scene_heading_template: Option<String>,
    pub scene_heading_language: Option<String>,
    pub scene_heading_auto: Option<bool>,
    pub project_memory: Option<screenplay_core::ast::ProjectMemory>,
    pub element_styles: Option<std::collections::BTreeMap<String, screenplay_core::ast::ElementStyle>>,
    pub style_preset_name: Option<String>,
    pub ocr_languages: Option<Vec<String>>,
    pub show_scene_numbers: Option<bool>,
    pub typography: Option<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub language: Option<String>,
    pub paper: Option<screenplay_core::ast::PaperSettings>,
}

#[tauri::command]
fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> IpcResult<CommandResult> {
    if patch.scene_heading_template.as_deref().is_some_and(|v| !screenplay_core::heading_format::valid_template(v)) || patch.scene_heading_language.as_deref().is_some_and(|v| !matches!(v,"zh"|"en")) {
        return Err(IpcError::InvalidArgument { message: "场景模板必须各包含一次 {type}、{location}、{time}，且不能换行".into() });
    }
    if patch.typography.as_deref().is_some_and(|v| v != "hollywood" && v != "chinese-reading") {
        return Err(IpcError::InvalidArgument { message: "无效的剧本排版模板".into() });
    }
    if patch.element_styles.as_ref().is_some_and(|v| !screenplay_core::typography::validate_styles(v)) {
        return Err(IpcError::InvalidArgument { message: "元素样式无效：字号须为 8–36 pt，颜色须为十六进制颜色".into() });
    }
    if let Some(memory)=patch.project_memory {
        if memory.markdown.len()+memory.author_notes.len()>128*1024||memory.history.len()>20||memory.history.iter().any(|s|s.len()>128*1024){return Err(IpcError::InvalidArgument{message:"项目记忆过大".into()});}
        let doc=state.project.document()?;
        let r=state.project.run(|svc|svc.update_project_memory(&doc.id,Some(doc.revision),memory))?;
        broadcast(&app,"memory",&r);return Ok(r);
    }
    let normalize_headings = patch.scene_heading_template.is_some() || patch.scene_heading_language.is_some() || patch.scene_heading_auto.is_some();
    let r = state.project.run(|svc| {
        svc.mutate_document(Origin::GuiLeft, |doc| {
            if let Some(v) = patch.scene_heading_template { doc.settings.scene_heading_template = v; }
            if let Some(v) = patch.scene_heading_language { doc.settings.scene_heading_language = v; }
            if let Some(v) = patch.scene_heading_auto { doc.settings.scene_heading_auto = v; }
            if normalize_headings {
                for b in &mut doc.blocks {
                    if b.block_type != screenplay_core::ast::BlockType::SceneHeading { continue; }
                    let n=screenplay_core::heading_format::format_heading(&b.text,&doc.settings).or_else(||b.scene_heading.as_ref().and_then(|d|screenplay_core::heading_format::format_heading(&screenplay_core::parser::format_scene_heading(d),&doc.settings)));
                    if let Some((text,data))=n { b.text=text; b.scene_heading=Some(data); }
                }
            }
            if let Some(styles) = patch.element_styles { doc.settings.element_styles = styles; }
            if let Some(name) = patch.style_preset_name { doc.settings.style_preset_name = name; }
            if let Some(langs) = patch.ocr_languages { doc.settings.ocr_languages = langs; }
            if let Some(value) = patch.show_scene_numbers { doc.settings.show_scene_numbers = value; }
            if let Some(style) = patch.typography {
                if style != doc.settings.typography {
                    doc.settings.font_size = if style == "chinese-reading" { 14.0 } else { 12.0 };
                    doc.settings.line_height = if style == "chinese-reading" { 1.6 } else { 1.0 };
                    doc.settings.font_family = if style == "chinese-reading" { "Songti SC, SimSun, serif".into() } else { "Courier, monospace".into() };
                }
                doc.settings.typography = style;
            }
            if let Some(t) = patch.title {
                doc.metadata.title = t.clone();
                doc.title_page.title = t;
            }
            if let Some(a) = patch.author {
                doc.metadata.author = a.clone();
                doc.title_page.written_by = Some(a);
            }
            if let Some(l) = patch.language {
                doc.settings.language = l.clone();
                doc.metadata.language = l;
            }
            if let Some(p) = &patch.paper {
                doc.settings.paper = p.clone();
            }
        })
    })?;
    broadcast(&app, "gui-left", &r);
    Ok(r)
}

// ---------------------------------------------------------------------------
// 版本
// ---------------------------------------------------------------------------

#[tauri::command]
fn list_versions(state: State<'_, AppState>) -> IpcResult<Vec<VersionRecord>> {
    let ws = state.project.require()?;
    Ok(ws.service.list_versions())
}

#[tauri::command]
fn preview_version(state: State<'_, AppState>, version_id: String) -> IpcResult<String> {
    let workspace=state.project.require()?;
    let document=workspace.service.store.version_document(&version_id).ok_or_else(||IpcError::NotFound{what:"version".into(),id:version_id})?;
    if let Some(series)=&document.series {
        let mut output=String::new();
        for episode in &series.episodes {
            let draft=if episode.id==series.active_id {&document} else {episode.draft.as_deref().ok_or_else(||IpcError::AppState{message:"分集正文缺失".into()})?};
            output.push_str(&format!("\n{}\n\n{}\n",episode.title,screenplay_core::serializer::to_compact_source(draft)));
        }
        Ok(output)
    } else {Ok(screenplay_core::serializer::to_compact_source(&document))}
}

#[tauri::command]
fn create_version(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    note: String,
    project_id: Option<String>,
) -> IpcResult<CommandResult> {
    let r =
        state.project.run(|svc| {
            if project_id.as_ref().is_some_and(|id|id!=&svc.store.snapshot().id) {return Err(screenplay_store::command::CommandError::InvalidArgument{message:"项目已切换，请重新打开历史记录".into()});}
            svc.create_version(Origin::GuiLeft, &name, &note)
        })?;
    broadcast(&app, "gui-left", &r);
    Ok(r)
}

#[tauri::command]
fn restore_version(
    app: AppHandle,
    state: State<'_, AppState>,
    version_id: String,
    project_id: Option<String>,
) -> IpcResult<CommandResult> {
    let r =
        state.project.run(|svc| {
            if project_id.as_ref().is_some_and(|id|id!=&svc.store.snapshot().id) {return Err(screenplay_store::command::CommandError::InvalidArgument{message:"项目已切换，请重新打开历史记录".into()});}
            svc.restore_version(Origin::GuiLeft, &version_id)
        })?;
    broadcast(&app, "gui-left", &r);
    Ok(r)
}

// ---------------------------------------------------------------------------
// 撤销 / 重做
// ---------------------------------------------------------------------------

#[tauri::command]
fn undo(app: AppHandle, state: State<'_, AppState>) -> IpcResult<CommandResult> {
    let r = state.project.run(|svc| svc.undo(Origin::GuiLeft))?;
    broadcast(&app, "gui-left", &r);
    Ok(r)
}

#[tauri::command]
fn redo(app: AppHandle, state: State<'_, AppState>) -> IpcResult<CommandResult> {
    let r = state.project.run(|svc| svc.redo(Origin::GuiLeft))?;
    broadcast(&app, "gui-left", &r);
    Ok(r)
}

#[tauri::command]
fn history_status(state: State<'_, AppState>) -> IpcResult<screenplay_store::command::HistoryStatus> {
    let ws = state.project.require()?;
    Ok(ws.service.history_status())
}

/// 比较两个版本（第二个为空表示与当前内容比较）
#[tauri::command]
fn compare_versions(
    state: State<'_, AppState>,
    version_a: String,
    version_b: Option<String>,
) -> IpcResult<serde_json::Value> {
    let ws = state.project.require()?;
    let b = version_b.unwrap_or_default();
    let d = ws.service.compare_versions(&version_a, &b)?;
    let mut v = serde_json::to_value(&d).map_err(|e| IpcError::Internal {
        message: e.to_string(),
    })?;
    // 附带渲染好的 diff 文本，前端可直接显示
    if let Some(obj) = v.as_object_mut() {
        let old = ws
            .service
            .store
            .version_document(&version_a)
            .unwrap_or_else(|| ws.document());
        let new = if b.is_empty() {
            ws.document()
        } else {
            ws.service
                .store
                .version_document(&b)
                .unwrap_or_else(|| ws.document())
        };
        obj.insert(
            "rendered".into(),
            serde_json::Value::String(screenplay_core::diff::render_diff(&old, &new, &d)),
        );
    }
    Ok(v)
}

/// 分支列表
#[tauri::command]
fn list_branches(state: State<'_, AppState>) -> IpcResult<Vec<Branch>> {
    let ws = state.project.require()?;
    Ok(ws.service.list_branches())
}

/// 当前分支
#[tauri::command]
fn current_branch(state: State<'_, AppState>) -> IpcResult<String> {
    let ws = state.project.require()?;
    Ok(ws.service.current_branch())
}

/// 切换/创建分支
#[tauri::command]
fn switch_branch(state: State<'_, AppState>, name: String) -> IpcResult<String> {
    let ws = state.project.require()?;
    // 分支切换可能失败（同名分支已存在、名称非法等）。
    // 必须把错误返回给前端，否则 UI 会显示"切换成功"但实际没切。
    ws.service.switch_branch(&name)?;
    Ok(ws.service.current_branch())
}

/// 审计日志
#[tauri::command]
fn audit_log(state: State<'_, AppState>, limit: Option<u32>) -> IpcResult<Vec<AuditEntry>> {
    let ws = state.project.require()?;
    Ok(ws.service.audit_log(limit.unwrap_or(200) as usize))
}

// ---------------------------------------------------------------------------
// 导入导出（统一走 Rust 核心）
// ---------------------------------------------------------------------------

#[tauri::command]
async fn import_file(app: AppHandle, state: State<'_, AppState>, path: String, request_id: Option<String>) -> IpcResult<CommandResult> {
    let complex = agent_text::complex(&path);
    let source_is_pdf=path.to_ascii_lowercase().ends_with(".pdf");
    let source_name=std::path::Path::new(&path).file_name().map(|s|s.to_string_lossy().into_owned());
    let project = state.project.clone();
    let before = project.document()?;
    let id = request_id.unwrap_or_else(|| format!("legacy-{}", before.id));
    if id.is_empty() || id.len() > 128 {
        return Err(IpcError::InvalidArgument { message: "无效的导入任务编号".into() });
    }
    let event_app = app.clone();
    let event_id = id.clone();
    let control = screenplay_import::ImportControl::with_progress(move |stage, completed, total| {
        let _ = event_app.emit("import-progress", serde_json::json!({
            "requestId": event_id, "stage": stage, "completed": completed, "total": total
        }));
    });
    {
        let mut jobs = state.imports.lock().unwrap();
        if !jobs.is_empty() {
            return Err(IpcError::AppState { message: "已有导入任务正在处理".into() });
        }
        jobs.insert(id.clone(), control.clone());
    }
    if complex {
        control.report("credential", 0, 0);
        let readiness = tokio::select! {
            result = agent_text::ready(&state) => result,
            _ = async { while !control.is_cancelled() { tokio::time::sleep(std::time::Duration::from_millis(100)).await; } } => Err(IpcError::AppState { message: "导入已取消".into() }),
        };
        if let Err(error) = readiness {
            state.imports.lock().unwrap().remove(&id);
            return Err(error);
        }
    }
    let worker_control = control.clone();
    let parsed = if complex {
        let extracted=tauri::async_runtime::spawn_blocking(move || agent_text::recognize(&path,&worker_control)).await;
        let result=async {
            let (text,bytes)=extracted.map_err(|e|IpcError::Internal{message:e.to_string()})??;
            if control.is_cancelled(){return Err(IpcError::AppState{message:"导入已取消".into()});}
            let _=app.emit("import-progress",serde_json::json!({"requestId":id,"stage":"agent","completed":0,"total":0}));
            let agent=build_agent(&state)?;
            let converted=tokio::select! {
                result=screenplay_agent::transform::transform_with_progress(&agent.router,&text,"忠实转换为标准剧本，不改写剧情",true,|completed,total|control.report("agent",completed,total))=>result.map_err(|e|IpcError::Internal{message:e.to_string()})?,
                _=async {while !control.is_cancelled(){tokio::time::sleep(std::time::Duration::from_millis(100)).await;}}=>return Err(IpcError::AppState{message:"导入已取消".into()}),
            };
            let mut document=screenplay_import::import_fountain(&converted,None).document;
            if let Some(info)=document.import_info.as_mut(){info.original_text=text;info.source_name=source_name;info.source_type=if source_is_pdf{screenplay_core::ast::SourceType::Pdf}else{screenplay_core::ast::SourceType::Image};}
            Ok((document,bytes))
        }.await;
        Ok(result.map_err(|e:IpcError|BridgeError::InvalidArgument{message:match e{IpcError::Internal{message}|IpcError::InvalidArgument{message}|IpcError::AppState{message}=>message,other=>format!("{other:?}")}}))
    } else {tauri::async_runtime::spawn_blocking(move || read_import_with_source_controlled(&PathBuf::from(path), &worker_control)).await};
    // The same lock serializes cancellation and commit. A successful cancellation
    // can never be followed by writing the parsed document.
    let mut jobs = state.imports.lock().unwrap();
    let result = (|| {
        if control.is_cancelled() {
            return Err(IpcError::AppState { message: "导入已取消".into() });
        }
        let (doc, original_source) = parsed.map_err(|e| IpcError::Internal { message: format!("导入任务失败: {e}") })?
            .map_err(|e| match e {
                BridgeError::NotFound { id, .. } => IpcError::NotFound { what: "文件".into(), id },
                other => IpcError::Internal { message: other.message() },
            })?;
        let r = project.run_with_original_source(|svc| {
            let current = svc.get_document().document;
            if current.id != before.id || current.revision != before.revision {
                return Err(CommandError::InvalidArgument { message: "导入期间剧本已发生变化，请重新导入".into() });
            }
            svc.load_document(Origin::Import, doc)
        }, original_source)?;
        broadcast(&app, "import", &r);
        Ok(r)
    })();
    jobs.remove(&id);
    result
}

#[tauri::command]
fn cancel_import(state: State<'_, AppState>, request_id: String) -> IpcResult<bool> {
    let jobs = state.imports.lock().unwrap();
    if let Some(control) = jobs.get(&request_id) {
        control.cancel();
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
fn export_file(
    state: State<'_, AppState>,
    path: String,
    format: Option<String>,
) -> IpcResult<String> {
    // 与 MCP 的 export_file 共用同一套格式解析与 screenplay_export 实现
    let fmt = bridge_host::resolve_export_format(&path, format.as_deref()).map_err(|e| {
        IpcError::InvalidArgument {
            message: e.message(),
        }
    })?;
    let doc = state.project.document()?;
    let doc = if let Some(series)=&doc.series { screenplay_core::series::export_drafts(&doc).map_err(|message|IpcError::InvalidArgument{message})?.into_iter().nth(series.episodes.iter().position(|e|e.id==series.active_id).unwrap_or(0)).ok_or_else(||IpcError::InvalidArgument{message:"分集不存在".into()})?.1 } else {screenplay_core::series::publication_copy(&doc)};
    let bytes = screenplay_export::export(&doc, fmt)
        .map_err(|e| IpcError::Internal {
            message: format!("导出失败: {e}"),
        })?;
    std::fs::write(&path, bytes).map_err(|e| IpcError::Internal {
        message: format!("写入文件失败: {e}"),
    })?;
    Ok(path)
}

// ---------------------------------------------------------------------------
// 自动保存
// ---------------------------------------------------------------------------

/// 读取自动保存配置
#[tauri::command]
fn autosave_config(state: State<'_, AppState>) -> IpcResult<AutosaveConfig> {
    Ok(state.project.autosave_config())
}

/// 配置自动保存
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutosaveOptions {
    pub enabled: bool,
    pub save_interval_secs: u64,
    pub version_interval_secs: u64,
}

#[tauri::command]
fn configure_autosave(app: AppHandle, state: State<'_, AppState>, options: AutosaveOptions) -> IpcResult<AutosaveConfig> {
    if options.save_interval_secs > 86400 || options.version_interval_secs > 86400 {
        return Err(IpcError::InvalidArgument { message: "自动保存间隔应在0至86400秒之间".into() });
    }
    if state.project.is_open() {
        let result = state.project.run(|service| service.mutate_document(Origin::GuiLeft, |doc| {
            doc.settings.auto_save = options.enabled;
            doc.settings.auto_save_interval_secs = options.save_interval_secs;
            doc.settings.auto_version_interval_secs = options.version_interval_secs;
        }))?;
        broadcast(&app, "gui-left", &result);
    }
    state.project.configure_autosave(
        options.enabled,
        options.save_interval_secs,
        options.version_interval_secs,
    );
    Ok(state.project.autosave_config())
}

/// 自动保存 tick（前端定时调用）
#[tauri::command]
async fn autosave_tick(state: State<'_, AppState>,project_id:Option<String>) -> IpcResult<Option<AutosaveAction>> {
    let project = state.project.clone();
    let action = tauri::async_runtime::spawn_blocking(move || project.autosave_tick_checked(project_id.as_deref())).await
        .map_err(|_| IpcError::Internal { message: "自动保存任务失败".into() })?
        .map_err(|message| IpcError::Internal { message })?;
    if let Some(a) = action {
        diag::log(&format!("autosave: {a:?}"));
    }
    Ok(action)
}

/// 是否有未保存修改（退出保护用）
#[tauri::command]
fn has_unsaved_changes(state: State<'_, AppState>) -> IpcResult<bool> {
    Ok(state.project.is_dirty())
}

/// 用户确认放弃修改并退出：写恢复点后关闭窗口
#[tauri::command]
fn confirm_close(window: tauri::Window, discard_changes: Option<bool>) -> IpcResult<()> {
    {
        let state = window.state::<AppState>();
        if discard_changes.unwrap_or(false) { state.project.close_without_saving()?; } else { state.project.shutdown(); }
        state.quitting.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    // 用 destroy 而不是 close，避免再次触发 CloseRequested 阻止
    window.destroy().map_err(|e| {
        window.state::<AppState>().quitting.store(false, std::sync::atomic::Ordering::SeqCst);
        IpcError::Internal { message: format!("关闭窗口失败: {e}") }
    })
}

// ---------------------------------------------------------------------------
// 崩溃恢复
// ---------------------------------------------------------------------------

#[tauri::command]
fn pending_recovery(state: State<'_, AppState>) -> IpcResult<Option<PendingRecovery>> {
    let p = state.project.pending_recovery();
    match &p {
        Some(r) => diag::log(&format!(
            "pending_recovery: title={} blocks={} rev={} preview={}",
            r.title, r.block_count, r.revision, r.preview
        )),
        None => diag::log("pending_recovery: 无（正常启动或已处理）"),
    }
    Ok(p)
}

#[tauri::command]
fn adopt_recovery(state: State<'_, AppState>) -> IpcResult<Bootstrap> {
    let doc = state.project.adopt_recovery()?;
    diag::log(&format!(
        "adopt_recovery: 已恢复 {} 个块，title={}",
        doc.blocks.len(),
        doc.metadata.title
    ));
    Ok(build_bootstrap(&state.project))
}

#[tauri::command]
fn discard_recovery(state: State<'_, AppState>) -> IpcResult<()> {
    state.project.discard_recovery();
    Ok(())
}

// ---------------------------------------------------------------------------
// 开发期诊断文件读写（供 GUI 自检探针写证据）
// ---------------------------------------------------------------------------

#[tauri::command]
fn home_dir() -> String {
    dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string())
}

#[tauri::command]
fn write_text_file(path: String, content: String) -> Result<(), IpcError> {
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| IpcError::Internal {
            message: format!("创建目录失败: {e}"),
        })?;
    }
    std::fs::write(&path, content).map_err(|e| IpcError::Internal {
        message: format!("写入文件失败: {e}"),
    })
}

#[tauri::command]
fn read_text_file(path: String) -> Result<String, IpcError> {
    std::fs::read_to_string(&path).map_err(|e| IpcError::Internal {
        message: format!("读取文件失败: {e}"),
    })
}

// ---------------------------------------------------------------------------
// 最近文件
// ---------------------------------------------------------------------------

#[tauri::command]
fn list_recent() -> IpcResult<Vec<RecentEntry>> {
    Ok(load_recent())
}

#[tauri::command]
fn remove_recent(path: String) -> IpcResult<()> {
    let mut list = load_recent();
    list.retain(|e| e.path != path);
    save_recent(&list);
    Ok(())
}

// ---------------------------------------------------------------------------
// Agent（E 阶段：智能编剧助理）
// ---------------------------------------------------------------------------

// Opening the app/panel never shows a Keychain authorization dialog.
async fn prepare_agent_credential(state: &AppState) -> IpcResult<()> {
    let cfg = state.agent.lock().unwrap().clone();
    if cfg.api_key.is_empty() && cfg.cloud_ai_enabled && matches!(cfg.provider_kind.as_str(), "compatible" | "openai") {
        let key = tokio::time::timeout(std::time::Duration::from_secs(30),
            tauri::async_runtime::spawn_blocking(credentials::load)).await
            .map_err(|_| IpcError::InvalidArgument { message: "读取密钥超时。请在编剧助理中填写 API Key 并保存后重试，密钥将记在本机。原稿未改变。".into() })?
            .map_err(|_| IpcError::Internal { message: "钥匙串读取任务失败".into() })?
            .map_err(|_| IpcError::InvalidArgument { message: "系统钥匙串暂不可访问。好好写剧本不会弹出系统授权窗口；请在编剧助理中重新填写 API Key 并保存，之后会记在本机。原稿未改变。".into() })?;
        if let Some(key) = key {
            let mut current = state.agent.lock().unwrap();
            if current.api_key.is_empty() { current.api_key = key; }
        }
    }
    Ok(())
}

/// 基于当前 AppState 构造 AgentRunner。
fn build_agent(state: &AppState) -> IpcResult<AgentRunner> {
    let service = state
        .project
        .command_service_arc()
        .map_err(|m| IpcError::AppState { message: m })?;
    let cfg = state.agent.lock().unwrap().clone();

    let mut router = ModelRouter::new(&cfg.provider_kind);
    match cfg.provider_kind.as_str() {
        "openai" => {
            if !cfg.api_key.is_empty() {
                router.register(Box::new(screenplay_agent::OpenAiProvider::openai(
                    &cfg.api_key,
                    &cfg.model,
                )));
            }
        }
        "ollama" => {
            router.register(Box::new(screenplay_agent::OpenAiProvider::ollama(&cfg.model)));
        }
        "compatible" => {
            if !cfg.api_key.is_empty() && !cfg.base_url.is_empty() {
                router.register(Box::new(screenplay_agent::OpenAiProvider::compatible(
                    &cfg.base_url,
                    &cfg.api_key,
                    &cfg.model,
                )));
            }
        }
        _ => {} // echo（内置，离线可用）
    }

    let skills = load_agent_skills();

    Ok(AgentRunner::new(service, router, skills).with_cloud_ai(cfg.cloud_ai_enabled))
}

fn load_agent_skills() -> SkillRegistry {
    let mut skills = SkillRegistry::new();
    for s in screenplay_agent::builtin_skills() {
        skills.register(s);
    }
    // 加载用户 skills 目录（如果存在）
    if let Some(dir) = dirs::data_dir().map(|d| d.join("ScreenplayStudio").join("skills")) {
        let _ = skills.load_from_dir(&dir);
    }

    skills
}

#[derive(Serialize)]
pub struct SkillInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub builtin: bool,
}

#[tauri::command]
fn agent_list_skills() -> IpcResult<Vec<SkillInfo>> {
    let skills = load_agent_skills();
    Ok(skills
        .list()
        .iter()
        .map(|s| SkillInfo {
            id: s.id.clone(),
            name: s.name.clone(),
            description: s.description.clone(),
            builtin: s.builtin,
        })
        .collect())
}

#[tauri::command]
async fn agent_apply_skill(
    app: AppHandle,
    state: State<'_, AppState>,
    skill_id: String,
    block_id: String,
    user_input: String,
) -> IpcResult<CommandResult> {
    agent_text::ready(&state).await?;
    let agent = build_agent(&state)?;
    let result = agent
        .apply_skill(&skill_id, &block_id, &user_input)
        .await
        .map_err(|e| IpcError::Internal {
            message: e.to_string(),
        })?;
    broadcast(&app, "agent", &result);
    Ok(result)
}

#[tauri::command]
async fn agent_freeform_edit(
    app: AppHandle,
    state: State<'_, AppState>,
    instruction: String,
) -> IpcResult<CommandResult> {
    agent_text::ready(&state).await?;
    let agent = build_agent(&state)?;
    let result = agent
        .freeform_edit(&instruction, 8)
        .await
        .map_err(|e| IpcError::Internal {
            message: e.to_string(),
        })?;
    broadcast(&app, "agent", &result);
    Ok(result)
}

/// Read-only model call: summarization cannot invoke editing tools.
#[tauri::command]
async fn agent_summarize_scene(state: State<'_, AppState>, scene_id: String) -> IpcResult<serde_json::Value> {
    agent_text::ready(&state).await?;
    prepare_agent_credential(&state).await?;
    let cfg = state.agent.lock().unwrap().clone();
    if cfg.provider_kind != "ollama" && !cfg.cloud_ai_enabled { return Err(IpcError::InvalidArgument { message: "请先在编剧助理中启用云 AI".into() }); }
    let doc = state.project.document()?;
    let agent = build_agent(&state)?;
    let result = screenplay_agent::summary::summarize_scene(&agent.router, &doc, &scene_id).await
        .map_err(|e| IpcError::Internal { message: e.to_string() })?;
    serde_json::to_value(result).map_err(|e| IpcError::Internal { message: e.to_string() })
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfigDto {
    pub cloud_ai_enabled: bool,
    pub provider_kind: String,
    pub model: String,
    pub api_key_set: bool,
    pub base_url: String,
}

#[tauri::command]
fn agent_get_config(state: State<AppState>) -> IpcResult<AgentConfigDto> {
    let cfg = state.agent.lock().unwrap().clone();
    Ok(AgentConfigDto {
        cloud_ai_enabled: cfg.cloud_ai_enabled,
        provider_kind: cfg.provider_kind,
        model: cfg.model,
        api_key_set: !cfg.api_key.is_empty(),
        base_url: cfg.base_url.clone(),
    })
}

#[tauri::command]
fn agent_set_config(
    state: State<AppState>,
    cloud_ai_enabled: Option<bool>,
    provider_kind: Option<String>,
    api_key: Option<String>,
    remember_key: Option<bool>,
    model: Option<String>,
    base_url: Option<String>,
) -> IpcResult<AgentConfigDto> {
    let mut guard = state.agent.lock().unwrap();
    let mut cfg = guard.clone();
    if let Some(v) = cloud_ai_enabled {
        cfg.cloud_ai_enabled = v;
    }
    if let Some(v) = provider_kind {
        cfg.provider_kind = v;
    }
    if let Some(v) = api_key {
        cfg.api_key = v;
    }
    if let Some(v) = model {
        cfg.model = v;
    }
    if let Some(v) = base_url {
        cfg.base_url = v;
    }
    if !["echo", "openai", "ollama", "compatible"].contains(&cfg.provider_kind.as_str()) || cfg.model.trim().is_empty() {
        return Err(IpcError::InvalidArgument { message: "请选择有效的 Provider 和模型".into() });
    }
    if cfg.provider_kind == "compatible" && !(cfg.base_url.starts_with("https://") || cfg.base_url.starts_with("http://localhost:") || cfg.base_url.starts_with("http://127.0.0.1:")) {
        return Err(IpcError::InvalidArgument { message: "远程模型地址必须使用 HTTPS".into() });
    }
    if remember_key.unwrap_or(false) && !cfg.api_key.is_empty() {
        model_profiles::remember(&cfg).map_err(|message| IpcError::Internal { message })?;
    }
    agent_config::save(&agent_config::ProviderSettings {
        cloud_ai_enabled: cfg.cloud_ai_enabled, provider_kind: cfg.provider_kind.clone(),
        model: cfg.model.clone(), base_url: cfg.base_url.clone(),
    }).map_err(|message| IpcError::Internal { message })?;
    *guard = cfg.clone();
    Ok(AgentConfigDto {
        cloud_ai_enabled: cfg.cloud_ai_enabled,
        provider_kind: cfg.provider_kind.clone(),
        model: cfg.model.clone(),
        api_key_set: !cfg.api_key.is_empty(),
        base_url: cfg.base_url.clone(),
    })
}

/// 用户 skills 目录：~/Library/Application Support/ScreenplayStudio/skills
fn user_skills_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("ScreenplayStudio").join("skills"))
}

/// 安装一个 skill：将源目录复制到用户 skills 目录。
#[tauri::command]
fn agent_install_skill(source_path: String) -> IpcResult<String> {
    let src = PathBuf::from(&source_path);
    if !src.is_dir() {
        return Err(IpcError::InvalidArgument {
            message: format!("源路径不是目录: {source_path}"),
        });
    }
    let skill_md = src.join("SKILL.md");
    if !skill_md.exists() {
        return Err(IpcError::InvalidArgument {
            message: "源目录缺少 SKILL.md".into(),
        });
    }
    // 解析以获取 skill id 作为目标目录名
    let content = std::fs::read_to_string(&skill_md).map_err(|e| IpcError::Internal {
        message: e.to_string(),
    })?;
    let skill = screenplay_agent::parse_skill_md(&content).map_err(|e| IpcError::InvalidArgument {
        message: e.to_string(),
    })?;

    let dest_dir = user_skills_dir().ok_or_else(|| IpcError::Internal {
        message: "无法确定用户数据目录".into(),
    })?;
    std::fs::create_dir_all(&dest_dir).map_err(|e| IpcError::Internal {
        message: e.to_string(),
    })?;
    if screenplay_agent::builtin_skills().iter().any(|builtin| builtin.id == skill.id) {
        return Err(IpcError::InvalidArgument { message: "不可覆盖内置 skill".into() });
    }
    let dest = dest_dir.join(&skill.id);
    let canonical_src = src.canonicalize().map_err(|error| IpcError::Internal { message: error.to_string() })?;
    let canonical_dest = dest_dir.canonicalize().map_err(|error| IpcError::Internal { message: error.to_string() })?.join(&skill.id);
    if canonical_src.starts_with(&canonical_dest) || canonical_dest.starts_with(&canonical_src) {
        return Err(IpcError::InvalidArgument { message: "安装源目录与目标目录不能相互包含".into() });
    }
    if dest.exists() {
        std::fs::remove_dir_all(&dest).map_err(|e| IpcError::Internal {
            message: e.to_string(),
        })?;
    }
    copy_dir_all(&src, &dest).map_err(|e| IpcError::Internal {
        message: e.to_string(),
    })?;
    Ok(skill.id)
}

/// 卸载一个 skill：从用户 skills 目录删除（内置 skill 不可卸载）。
#[tauri::command]
fn agent_unload_skill(skill_id: String) -> IpcResult<bool> {
    if !screenplay_agent::skill::is_valid_skill_id(&skill_id) {
        return Err(IpcError::InvalidArgument { message: "非法 skill id".into() });
    }
    // 先检查是否内置
    let skills = load_agent_skills();
    if let Some(s) = skills.get(&skill_id) {
        if s.builtin {
            return Err(IpcError::InvalidArgument {
                message: "内置 skill 不可卸载".into(),
            });
        }
    }
    let dest_dir = user_skills_dir().ok_or_else(|| IpcError::Internal {
        message: "无法确定用户数据目录".into(),
    })?;
    let target = dest_dir.join(&skill_id);
    if target.exists() {
        std::fs::remove_dir_all(&target).map_err(|e| IpcError::Internal {
            message: e.to_string(),
        })?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_symlink() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "skill 不允许符号链接"));
        }
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 工具
// ---------------------------------------------------------------------------

fn parse_block_type(s: &str) -> IpcResult<BlockType> {
    serde_json::from_value(serde_json::Value::String(s.to_string()))
        .map_err(|_| IpcError::InvalidArgument {
            message: format!("未知 block 类型: {s}"),
        })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    diag::init();
    let selftest = std::env::var("SCREENPLAY_SELFTEST").is_ok();
    if selftest {
        diag::log("SCREENPLAY_SELFTEST 已启用（启动后 1 秒自动执行闭环自检）");
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updates::UpdateState::default())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            updates::check_update,
            updates::download_update,
            updates::install_update,
            help::open_help_link,
            series::edit_series,
            series::preview_text_replacement,
            series::apply_text_replacement,
            series::save_series_planning,
            series::review_continuity,
            series::series_report_status,
            series::compare_project_versions,
            continuity::check_series_continuity,
            continuity::cancel_series_check,
            series::export_series_files,
            // 生命周期
            bootstrap,
            new_project,
            open_project,
            close_project,
            save_project,
            save_project_as,
            revert_project,
            project_info_cmd,
            // 读取
            get_document,
            get_source_text,
            search,
            // 修改
            replace_block,
            insert_block,
            delete_block,
            set_block_type,
            reorder_scenes,
            update_scene_board,
            create_board_scene,
            agent_summarize_scene,
            agent_text::agent_transform_text,
            agent_text::agent_attachment_text,
            agent_text::agent_apply_selection,
            agent_text::agent_apply_text,
            merge_blocks,
            replace_source_text,
            // 分页与序列化（与导出共用 Rust 实现）
            get_pagination,
            document_stats,
            to_fountain,
            update_settings,
            // 撤销重做
            undo,
            redo,
            history_status,
            // 版本
            list_versions,
            preview_version,
            create_version,
            restore_version,
            audit_log,
            compare_versions,
            list_branches,
            current_branch,
            switch_branch,
            // 导入导出
            import_file,
            cancel_import,
            export_file,
            // 自动保存
            connection::agent_connection_info,
            connection::check_agent_connection,
            connection::install_agent_cli,
            autosave_config,
            configure_autosave,
            autosave_tick,
            has_unsaved_changes,
            confirm_close,
            // 崩溃恢复
            pending_recovery,
            adopt_recovery,
            discard_recovery,
            // 开发期诊断
            home_dir,
            write_text_file,
            read_text_file,
            // 最近文件
            list_recent,
            take_file_open_requests,
            remove_recent,
            // Agent
            agent_harness::agent_chat,
            agent_harness::agent_refresh_memory,
            agent_harness::agent_share_project,
            agent_harness::agent_generate_skill,
            agent_harness::agent_create_skill,
            agent_harness::agent_memory,
            agent_harness::agent_save_memory,
            agent_list_skills,
            agent_apply_skill,
            agent_freeform_edit,
            agent_get_config,
            agent_set_config,
            model_profiles::agent_list_models,
            model_profiles::agent_save_model,
            model_profiles::agent_select_model,
            model_profiles::agent_delete_model,
            model_profiles::agent_test_model,
            model_profiles::agent_available_models,
            agent_install_skill,
            agent_unload_skill,
        ])
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::Destroyed => {
                // 窗口销毁时写最后一次恢复点
                let state = window.state::<AppState>();
                state.project.shutdown();
                // 主动清理桥接产物：进程被 SIGTERM 杀掉时不会跑 Drop，
                // 残留的 bridge.json 会让外部进程误判「GUI 还在运行」。
                state.cleanup_bridge();
            }
            tauri::WindowEvent::CloseRequested { api, .. } => {
                // Even a clean backend can have debounced edits still in the webview.
                // Always let the frontend flush drafts before deciding whether to close.
                api.prevent_close();
                let _ = window.emit("close-requested-with-unsaved", ());
            }
            _ => {}
        })
        .setup(move |app| {
            if selftest {
                use tauri::Manager;
                if let Some(w) = app.get_webview_window("main") {
                    let init = format!(
                        "window.__SCREENPLAY_SELFTEST__ = \"1\";"
                    );
                    let _ = w.eval(&init);
                }
            }

            // 启动跨进程桥接：CLI / MCP 通过它操作本进程的项目。
            // 失败不阻断 GUI —— 外部工具会回退独立进程模式 + 文件锁。
            let handle = app.handle().clone();
            if let Some(server) = start_bridge(&handle) {
                *handle.state::<AppState>().bridge.lock().unwrap() = Some(server);
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            if let tauri::RunEvent::Opened { urls } = &event {
                let mut pending = FILE_OPEN_REQUESTS.lock().unwrap();
                for url in urls {
                    if let Ok(path) = url.to_file_path() {
                        if path.extension().is_some_and(|ext| ext == "screenplay") {
                            let path = path.to_string_lossy().into_owned();
                            if !pending.contains(&path) { pending.push(path); }
                        }
                    }
                }
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                let state = app.state::<AppState>();
                if !state.quitting.load(std::sync::atomic::Ordering::SeqCst) {
                    if let Some(window) = app.get_webview_window("main") {
                        api.prevent_exit();
                        let _ = window.emit("close-requested-with-unsaved", ());
                    }
                }
            } else if matches!(event, tauri::RunEvent::Exit) {
                let state = app.state::<AppState>();
                state.project.shutdown();
                state.cleanup_bridge();
            }
        });
}
#[cfg(test)]
mod import_safety_tests {
    #[test]
    fn empty_import_is_rejected_before_replacing_current_document() {
        let path = std::env::temp_dir().join(format!("muben-empty-import-{}.txt", std::process::id()));
        std::fs::write(&path, "\n  \n").unwrap();
        let result = super::read_import(&path);
        let _ = std::fs::remove_file(&path);
        assert!(result.unwrap_err().message().contains("未识别到可编辑文字"));
    }

    #[test]
    fn imported_source_bytes_are_preserved_exactly() {
        let path = std::env::temp_dir().join(format!("muben-source-roundtrip-{}.txt", std::process::id()));
        let expected_name = path.file_name().unwrap().to_string_lossy().into_owned();
        let source = "原稿第一场\n人物：你好。\n".as_bytes();
        std::fs::write(&path, source).unwrap();
        let result = super::read_import_with_source(&path);
        let _ = std::fs::remove_file(&path);
        let (document, archived) = result.unwrap();
        assert_eq!(archived, source);
        assert_eq!(document.import_info.unwrap().source_name.as_deref(), Some(expected_name.as_str()));
    }
}
