//! 本地项目桥（Local Project Bridge）—— 让 GUI / CLI / MCP 共享**同一个**权威项目状态。
//!
//! # 问题背景
//!
//! 原实现里 `Workspace::open` 会把 `.screenplay`（ZIP）解压到
//! `temp_dir()/screenplay-<uuid>/`，每个进程各得一份独立 SQLite。
//! 结果是：
//! - GUI 里看到的文档和 MCP 改的文档**不是同一份**
//! - 外部写入不会实时出现在 GUI
//! - 各自保存会互相覆盖
//! - revision 冲突检测形同虚设（不同进程根本不在同一个 revision 序列上）
//!
//! # 方案
//!
//! GUI（唯一持有权威 `ProjectService` 的进程）在启动时监听一个
//! **Unix domain socket**，并发布运行时描述文件（默认
//! `~/Library/Application Support/ScreenplayStudio/bridge.json`）。
//!
//! CLI / MCP 启动时按以下顺序解析项目：
//!
//! 1. 读描述文件，连上 GUI 的 socket → **转发所有命令给 GUI**，
//!    操作的是 GUI 当前打开的项目，revision 与 dirty 完全一致。
//! 2. 描述文件不存在或连接失败 → 回退到「独立进程模式」：
//!    自己打开 `.screenplay` 并**持有文件锁**。
//!    - 若锁被占用（GUI 正在写同一个文件），返回结构化错误
//!      `PROJECT_IN_USE`，而不是静默读到旧数据。
//!
//! # 为什么用 Unix socket 而不是 HTTP
//!
//! - macOS 原生支持，零依赖
//! - 仅本机可访问（文件系统权限即边界），不需要鉴权
//! - 与 Tauri 事件机制天然契合：GUI 侧收到请求后走同一条
//!   `CommandService` 路径，改完直接广播 `document-changed`

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

/// 运行时描述文件：告诉外部进程「GUI 在哪里，当前打开什么」
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeDescriptor {
    /// Unix socket 路径
    pub socket: PathBuf,
    /// GUI 进程 pid（用于检测 GUI 是否已退出）
    pub pid: u32,
    /// 当前打开的项目容器路径（None = 未保存的新项目）
    pub container: Option<PathBuf>,
    /// 项目标题（仅供展示与诊断）
    pub title: String,
    /// GUI 启动时的协议版本，用于不兼容检测
    pub protocol: u32,
    /// 描述文件写入时间（用于陈旧检测）
    pub updated_at: String,
}

impl BridgeDescriptor {
    pub const PROTOCOL: u32 = 1;

    pub fn new(
        socket: PathBuf,
        pid: u32,
        container: Option<PathBuf>,
        title: String,
    ) -> Self {
        Self {
            socket,
            pid,
            container,
            title,
            protocol: Self::PROTOCOL,
            updated_at: now_iso(),
        }
    }

    /// 描述文件是否已陈旧（GUI 进程已退出但文件没被清理）
    pub fn is_stale(&self) -> bool {
        !process_alive(self.pid)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }

    pub fn from_json(s: &str) -> Result<Self, String> {
        serde_json::from_str(s).map_err(|e| format!("桥接描述文件损坏: {e}"))
    }
}

/// 桥接请求：与 Tauri 命令一一对应
///
/// 刻意做成封闭枚举而不是任意 JSON：`serde` 的 tag 匹配会在协议
/// 不兼容时给出明确的 `INVALID_ARGUMENT`，而不是让外部进程传入
/// 任意命令名在服务端做字符串分发。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum BridgeRequest {
    // ---- 状态 ----
    Ping,
    ProjectInfo,
    Workbench { request: crate::workbench_bridge::WorkbenchRequest },

    // ---- 项目生命周期（外部进程驱动 GUI 切换项目）----
    /// 让 GUI 打开指定项目。成功后**后续所有操作都在该项目上进行**。
    OpenProject { path: String },
    /// 让 GUI 新建项目。
    NewProject { title: String },
    /// 保存当前项目（`path` 为空则保存到原路径）。
    SaveProject { #[serde(default)] path: Option<String> },
    /// 关闭当前项目，回到欢迎页。
    CloseProject,

    // ---- 读取 ----
    GetDocument,
    GetSourceText,
    Search {
        query: String,
        #[serde(default)]
        case_sensitive: bool,
    },

    // ---- 修改（全部强制 expected_revision）----
    ReplaceBlock {
        block_id: String,
        text: String,
        expected_revision: u64,
    },
    InsertBlock {
        after_id: Option<String>,
        block_type: String,
        text: String,
        expected_revision: u64,
    },
    DeleteBlock {
        block_id: String,
        expected_revision: u64,
    },
    SetBlockType {
        block_id: String,
        block_type: String,
        expected_revision: u64,
    },
    MergeBlocks {
        first_id: String,
        second_id: String,
        expected_revision: u64,
    },
    ReplaceSourceText {
        text: String,
        expected_revision: u64,
    },

    // ---- 撤销重做 ----
    Undo,
    Redo,
    HistoryStatus,

    // ---- 版本 ----
    ListVersions,
    CreateVersion {
        name: String,
        #[serde(default)]
        note: String,
    },
    RestoreVersion {
        version_id: String,
    },
    CompareVersions {
        #[serde(default)]
        version_a: Option<String>,
        #[serde(default)]
        version_b: Option<String>,
    },
    AuditLog {
        #[serde(default)]
        limit: Option<u32>,
    },
    ListBranches,
    CurrentBranch,
    SwitchBranch {
        name: String,
    },

    // ---- 导入导出 ----
    ImportFile {
        path: String,
    },
    ExportFile {
        path: String,
        #[serde(default)]
        format: Option<String>,
    },

    // ---- 分页与序列化 ----
    GetPagination,
    DocumentStats,
    ToFountain,
}

/// 桥接响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeResponse {
    pub ok: bool,
    /// 成功时的业务数据
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    /// 失败时的结构化错误（与 CommandError 同构）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<BridgeError>,
    /// 变更后的 revision（便于外部进程更新自己的游标）
    #[serde(default)]
    pub revision: u64,
}

impl BridgeResponse {
    pub fn ok(data: serde_json::Value, revision: u64) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
            revision,
        }
    }

    pub fn fail(error: BridgeError, revision: u64) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(error),
            revision,
        }
    }
}

/// 结构化错误：与 `CommandError` 语义一致，便于外部进程统一处理
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "error", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BridgeError {
    RevisionConflict {
        expected: u64,
        actual: u64,
        message: String,
    },
    NotFound {
        what: String,
        id: String,
    },
    InvalidArgument {
        message: String,
    },
    BlockLocked {
        id: String,
    },
    Internal {
        message: String,
    },
    /// GUI 未打开项目 / 未运行
    AppState {
        message: String,
    },
    /// 项目文件被另一个进程占用
    ProjectInUse {
        path: String,
        holder: String,
    },
    /// 协议版本不兼容
    ProtocolMismatch {
        expected: u32,
        actual: u32,
    },
    /// 桥接通道错误（socket 断开等）
    BridgeUnavailable {
        message: String,
    },
}

impl BridgeError {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_else(|_| {
            serde_json::json!({"error": "INTERNAL", "message": "错误序列化失败"})
        })
    }

    pub fn message(&self) -> String {
        match self {
            BridgeError::RevisionConflict {
                expected,
                actual,
                message,
            } => format!("REVISION_CONFLICT: {message}（你基于 rev {expected}，当前 rev {actual}）"),
            BridgeError::NotFound { what, id } => format!("NOT_FOUND: 找不到{what} {id}"),
            BridgeError::InvalidArgument { message } => format!("INVALID_ARGUMENT: {message}"),
            BridgeError::BlockLocked { id } => format!("LOCKED: {id} 已锁定"),
            BridgeError::Internal { message } => format!("INTERNAL: {message}"),
            BridgeError::AppState { message } => format!("APP_STATE: {message}"),
            BridgeError::ProjectInUse { path, holder } => format!(
                "PROJECT_IN_USE: {path} 正被 {holder} 打开。请关闭该实例后重试，或先在 GUI 中保存。"
            ),
            BridgeError::ProtocolMismatch { expected, actual } => format!(
                "PROTOCOL_MISMATCH: 桥接协议不兼容（客户端 {expected}，服务端 {actual}）"
            ),
            BridgeError::BridgeUnavailable { message } => {
                format!("BRIDGE_UNAVAILABLE: {message}")
            }
        }
    }

    /// 错误码字符串（供 JSON 输出与测试断言）
    pub fn code(&self) -> &'static str {
        match self {
            BridgeError::RevisionConflict { .. } => "REVISION_CONFLICT",
            BridgeError::NotFound { .. } => "NOT_FOUND",
            BridgeError::InvalidArgument { .. } => "INVALID_ARGUMENT",
            BridgeError::BlockLocked { .. } => "LOCKED",
            BridgeError::Internal { .. } => "INTERNAL",
            BridgeError::AppState { .. } => "APP_STATE",
            BridgeError::ProjectInUse { .. } => "PROJECT_IN_USE",
            BridgeError::ProtocolMismatch { .. } => "PROTOCOL_MISMATCH",
            BridgeError::BridgeUnavailable { .. } => "BRIDGE_UNAVAILABLE",
        }
    }
}

impl BridgeRequest {
    /// 短标签（诊断日志用）
    pub fn tag(&self) -> &'static str {
        match self {
            BridgeRequest::Ping => "Ping",
            BridgeRequest::ProjectInfo => "ProjectInfo",
            BridgeRequest::Workbench { .. } => "Workbench",
            BridgeRequest::OpenProject { .. } => "OpenProject",
            BridgeRequest::NewProject { .. } => "NewProject",
            BridgeRequest::SaveProject { .. } => "SaveProject",
            BridgeRequest::CloseProject => "CloseProject",
            BridgeRequest::GetDocument => "GetDocument",
            BridgeRequest::GetSourceText => "GetSourceText",
            BridgeRequest::Search { .. } => "Search",
            BridgeRequest::ReplaceBlock { .. } => "ReplaceBlock",
            BridgeRequest::InsertBlock { .. } => "InsertBlock",
            BridgeRequest::DeleteBlock { .. } => "DeleteBlock",
            BridgeRequest::SetBlockType { .. } => "SetBlockType",
            BridgeRequest::MergeBlocks { .. } => "MergeBlocks",
            BridgeRequest::ReplaceSourceText { .. } => "ReplaceSourceText",
            BridgeRequest::Undo => "Undo",
            BridgeRequest::Redo => "Redo",
            BridgeRequest::HistoryStatus => "HistoryStatus",
            BridgeRequest::ListVersions => "ListVersions",
            BridgeRequest::CreateVersion { .. } => "CreateVersion",
            BridgeRequest::RestoreVersion { .. } => "RestoreVersion",
            BridgeRequest::CompareVersions { .. } => "CompareVersions",
            BridgeRequest::AuditLog { .. } => "AuditLog",
            BridgeRequest::ListBranches => "ListBranches",
            BridgeRequest::CurrentBranch => "CurrentBranch",
            BridgeRequest::SwitchBranch { .. } => "SwitchBranch",
            BridgeRequest::ImportFile { .. } => "ImportFile",
            BridgeRequest::ExportFile { .. } => "ExportFile",
            BridgeRequest::GetPagination => "GetPagination",
            BridgeRequest::DocumentStats => "DocumentStats",
            BridgeRequest::ToFountain => "ToFountain",
        }
    }
}

// ---------------------------------------------------------------------------
// 描述文件位置
// ---------------------------------------------------------------------------

/// 应用数据目录：`~/Library/Application Support/ScreenplayStudio`
pub fn app_support_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("ScreenplayStudio"))
}

/// 桥接描述文件路径
pub fn descriptor_path() -> Option<PathBuf> {
    app_support_dir().map(|d| d.join("bridge.json"))
}

/// 锁文件路径（独立进程模式使用）
pub fn canonical_container_path(container:&Path)->PathBuf{
    let absolute=if container.is_absolute(){container.to_path_buf()}else{std::env::current_dir().unwrap_or_default().join(container)};
    let mut cleaned=PathBuf::new();for part in absolute.components(){match part{std::path::Component::CurDir=>{},std::path::Component::ParentDir=>{cleaned.pop();},_=>cleaned.push(part.as_os_str())}}
    cleaned.canonicalize().unwrap_or_else(|_|cleaned.parent().and_then(|p|p.canonicalize().ok()).and_then(|p|cleaned.file_name().map(|n|p.join(n))).unwrap_or(cleaned))
}
pub fn lock_path_for(container: &Path) -> Option<PathBuf> {
    app_support_dir().map(|d|{let normalized=canonical_container_path(container);let h=fnv1a(normalized.to_string_lossy().as_bytes());d.join(format!("locks/{h:016x}.lock"))})
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

// ---------------------------------------------------------------------------
// 客户端：连接 GUI
// ---------------------------------------------------------------------------

/// 桥接客户端
pub struct BridgeClient {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl BridgeClient {
    /// 尝试连接正在运行的 GUI。
    ///
    /// 返回 `Ok(None)` 表示「没有 GUI 在运行」——这是正常情况，
    /// 调用方应回退到独立进程模式，而不是报错。
    pub fn connect_if_running() -> Result<Option<Self>, BridgeError> {
        let desc_path = descriptor_path().ok_or_else(|| BridgeError::Internal {
            message: "无法确定应用数据目录".into(),
        })?;

        if !desc_path.exists() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(&desc_path).map_err(|e| BridgeError::Internal {
            message: format!("读取桥接描述文件失败: {e}"),
        })?;
        let desc = BridgeDescriptor::from_json(&raw).map_err(|m| BridgeError::Internal {
            message: m,
        })?;

        if desc.protocol != BridgeDescriptor::PROTOCOL {
            // 协议不兼容：清理陈旧描述文件，让调用方回退
            let _ = std::fs::remove_file(&desc_path);
            return Err(BridgeError::ProtocolMismatch {
                expected: BridgeDescriptor::PROTOCOL,
                actual: desc.protocol,
            });
        }

        if desc.is_stale() {
            // GUI 已退出但没清理描述文件
            let _ = std::fs::remove_file(&desc_path);
            return Ok(None);
        }

        match UnixStream::connect(&desc.socket) {
            Ok(stream) => {
                let reader = BufReader::new(stream.try_clone().map_err(|e| {
                    BridgeError::BridgeUnavailable {
                        message: format!("复制 socket 句柄失败: {e}"),
                    }
                })?);
                Ok(Some(Self {
                    reader,
                    writer: stream,
                }))
            }
            Err(_) => {
                // socket 文件残留但连不上
                let _ = std::fs::remove_file(&desc_path);
                Ok(None)
            }
        }
    }

    /// 是否已有 GUI 在运行（用于 CLI 决定是否要提示用户）
    pub fn gui_running() -> bool {
        match descriptor_path() {
            Some(p) if p.exists() => std::fs::read_to_string(p)
                .ok()
                .and_then(|s| BridgeDescriptor::from_json(&s).ok())
                .map(|d| !d.is_stale())
                .unwrap_or(false),
            _ => false,
        }
    }

    /// 当前 GUI 打开的项目路径
    pub fn gui_project() -> Option<PathBuf> {
        let p = descriptor_path()?;
        let raw = std::fs::read_to_string(p).ok()?;
        let desc = BridgeDescriptor::from_json(&raw).ok()?;
        if desc.is_stale() {
            return None;
        }
        desc.container
    }

    /// 发送一条请求并读取响应
    pub fn call(&mut self, req: &BridgeRequest) -> Result<BridgeResponse, BridgeError> {
        let mut line = serde_json::to_string(req).map_err(|e| BridgeError::Internal {
            message: format!("请求序列化失败: {e}"),
        })?;
        line.push('\n');
        self.writer
            .write_all(line.as_bytes())
            .and_then(|_| self.writer.flush())
            .map_err(|e| BridgeError::BridgeUnavailable {
                message: format!("写入桥接请求失败: {e}"),
            })?;

        let mut buf = String::new();
        self.reader.read_line(&mut buf).map_err(|e| {
            BridgeError::BridgeUnavailable {
                message: format!("读取桥接响应失败: {e}"),
            }
        })?;
        // 诊断：请求与响应的 op 是否一致。
        // 不一致说明**响应错位**（读到了上一条请求的响应）。
        let want = req.tag();
        let got_op = serde_json::from_str::<serde_json::Value>(buf.trim())
            .ok()
            .map(|v| {
                v.get("data")
                    .map(|d| {
                        // 响应体不含 op，只能靠「是否有 blocks 字段」等启发式判断
                        let keys: Vec<&str> = d
                            .as_object()
                            .map(|m| m.keys().map(|k| k.as_str()).collect())
                            .unwrap_or_default();
                        if keys.contains(&"blocks") {
                            "GetDocument"
                        } else if keys.contains(&"pong") {
                            "Ping"
                        } else if keys.contains(&"state") {
                            "ProjectInfo"
                        } else {
                            "?"
                        }
                    })
                    .unwrap_or("null")
            })
            .unwrap_or("?");
        if std::env::var("SCREENPLAY_BRIDGE_TRACE").is_ok() {
            eprintln!("[bridge] req={want} resp_data_shape={got_op} raw={}", &buf[..buf.len().min(200)]);
        }

        if buf.trim().is_empty() {
            return Err(BridgeError::BridgeUnavailable {
                message: "桥接响应为空（GUI 可能已退出）".into(),
            });
        }
        serde_json::from_str(buf.trim()).map_err(|e| BridgeError::Internal {
            message: format!("桥接响应解析失败: {e}"),
        })
    }
}

// ---------------------------------------------------------------------------
// 项目文件锁（独立进程模式）
// ---------------------------------------------------------------------------

/// 项目锁：防止两个独立进程同时写同一个 `.screenplay`
///
/// GUI 运行时**不**持这个锁（GUI 通过桥接被访问，只有一个进程写），
/// 所以 GUI 打开的文件可以被 CLI 以「转发」方式操作，而不是被锁挡住。
#[derive(Debug)]
pub struct ProjectLock {
    path: PathBuf,
}

impl ProjectLock {
    /// 尝试获取锁。已被占用时返回 `Err(PROJECT_IN_USE)`。
    pub fn acquire(container: &Path) -> Result<Self, BridgeError> {
        let path = lock_path_for(container).ok_or_else(|| BridgeError::Internal {
            message: "无法确定锁文件路径".into(),
        })?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| BridgeError::Internal {
                message: format!("创建锁目录失败: {e}"),
            })?;
        }

        // 先检查是否已有活锁
        if let Ok(existing) = std::fs::read_to_string(&path) {
            let holder = existing.trim().to_string();
            let pid: u32 = holder
                .split(':')
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            if pid == 0 || !process_alive(pid) {
                // 陈旧锁，清除后重试
                let _ = std::fs::remove_file(&path);
            } else {
                return Err(BridgeError::ProjectInUse {
                    path: container.to_string_lossy().to_string(),
                    holder: holder.clone(),
                });
            }
        }

        let me = format!("{}:{}", std::process::id(), now_iso());
        // O_EXCL 保证原子性：两个进程同时抢只有一个成功
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => {
                let _ = f.write_all(me.as_bytes());
                Ok(Self { path })
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let holder = std::fs::read_to_string(&path).unwrap_or_default();
                Err(BridgeError::ProjectInUse {
                    path: container.to_string_lossy().to_string(),
                    holder: holder.trim().to_string(),
                })
            }
            Err(e) => Err(BridgeError::Internal {
                message: format!("创建锁文件失败: {e}"),
            }),
        }
    }
}

impl Drop for ProjectLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

// ---------------------------------------------------------------------------
// 服务端：GUI 侧监听
// ---------------------------------------------------------------------------

/// 桥接服务端：持有 listener 与描述文件，退出时自动清理
///
/// 生命周期绑定 GUI 进程：GUI 启动时创建，`Drop` 时删除 socket 与描述文件，
/// 这样外部进程不会连到一个已死的 socket。
pub struct BridgeServer {
    listener: UnixListener,
    socket_path: PathBuf,
    descriptor_path: PathBuf,
    /// 是否已成功发布描述文件
    published: bool,
    /// listener 是否已设为非阻塞（只设一次）
    nonblocking_set: std::sync::atomic::AtomicBool,
}

impl BridgeServer {
    /// 启动监听。
    ///
    /// `project_hint` 会在项目变化时由调用方 [`Self::update_project`] 更新。
    pub fn start(project_hint: Option<PathBuf>, title: &str) -> Result<Self, BridgeError> {
        let dir = app_support_dir().ok_or_else(|| BridgeError::Internal {
            message: "无法确定应用数据目录".into(),
        })?;
        std::fs::create_dir_all(&dir).map_err(|e| BridgeError::Internal {
            message: format!("创建应用数据目录失败: {e}"),
        })?;

        // socket 路径带 pid，避免与残留的旧 socket 冲突
        let socket_path = dir.join(format!("bridge-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&socket_path);

        let listener = UnixListener::bind(&socket_path).map_err(|e| BridgeError::Internal {
            message: format!("绑定桥接 socket 失败: {e}"),
        })?;

        let descriptor_path = descriptor_path().ok_or_else(|| BridgeError::Internal {
            message: "无法确定描述文件路径".into(),
        })?;

        let desc = BridgeDescriptor::new(
            socket_path.clone(),
            std::process::id(),
            project_hint,
            title.to_string(),
        );
        atomic_write(&descriptor_path, &desc.to_json()).map_err(|m| BridgeError::Internal {
            message: m,
        })?;

        Ok(Self {
            listener,
            socket_path,
            descriptor_path,
            published: true,
            nonblocking_set: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// 项目切换 / 另存为后更新描述文件，让外部进程知道当前操作对象
    pub fn update_project(&self, container: Option<PathBuf>, title: &str) {
        if !self.published {
            return;
        }
        let desc = BridgeDescriptor::new(
            self.socket_path.clone(),
            std::process::id(),
            container,
            title.to_string(),
        );
        let _ = atomic_write(&self.descriptor_path, &desc.to_json());
    }

    /// socket 路径
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// 非阻塞地接受一个连接（返回 None 表示当前无连接）
    ///
    /// `set_nonblocking` 只在 accept 循环里反复调用是安全的：
    /// O_NONBLOCK 作用于 **listener**，`accept` 返回的已连接 socket
    /// 不继承该标志（POSIX 规定）。
    pub fn try_accept(&self) -> Option<std::io::Result<BridgeConnection>> {
        if self.nonblocking_set.swap(true, std::sync::atomic::Ordering::SeqCst) == false {
            self.listener.set_nonblocking(true).ok()?;
        }
        match self.listener.accept() {
            Ok((stream, _)) => Some(BridgeConnection::new(stream)),
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => None,
            Err(e) => Some(Err(e)),
        }
    }
}

impl Drop for BridgeServer {
    fn drop(&mut self) {
        // 清理 socket 与描述文件：外部进程据此判断「GUI 已退出」
        let _ = std::fs::remove_file(&self.socket_path);
        if self.published {
            // 只删自己的描述文件（确认描述文件里的 pid 是自己，
            // 避免删掉后来启动的另一个 GUI 实例的）
            if let Ok(raw) = std::fs::read_to_string(&self.descriptor_path) {
                if let Ok(d) = BridgeDescriptor::from_json(&raw) {
                    if d.pid == std::process::id() {
                        let _ = std::fs::remove_file(&self.descriptor_path);
                    }
                }
            }
        }
    }
}

/// 单个桥接连接（一问一答，可复用）
pub struct BridgeConnection {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

/// 读取结果：区分「断开」「暂时无数据」「非法请求」
pub enum ReadOutcome {
    /// 读到一条合法请求
    Request(Box<BridgeRequest>),
    /// 对端已关闭写端
    Eof,
    /// 暂时无数据（非阻塞 socket 上的 WouldBlock），**不是断开**
    WouldBlock,
    /// 解析失败（已回错误响应）
    BadRequest,
    /// 真实 IO 错误
    Io(std::io::Error),
}

impl BridgeConnection {
    pub fn new(stream: UnixStream) -> std::io::Result<Self> {
        // ⚠️ 关键：显式把连接设为**阻塞**。
        // listener 上的 O_NONBLOCK 在部分平台会被 accept 出来的
        // socket 继承；若连接保持非阻塞，read_line 遇到「暂时无数据」
        // 会返回 WouldBlock，被误判为「客户端断开」而立刻关闭连接。
        stream.set_nonblocking(false)?;
        let reader = BufReader::new(stream.try_clone()?);
        Ok(Self {
            reader,
            writer: stream,
        })
    }

    /// 读取一条请求
    pub fn read_request(&mut self) -> Option<BridgeRequest> {
        match self.read_request_detailed() {
            ReadOutcome::Request(r) => Some(*r),
            ReadOutcome::Eof => None,
            ReadOutcome::WouldBlock => {
                // 保持连接：调用方应稍后重试。
                // 兼容旧语义：把「无数据」当作暂时无请求，
                // 但**不**关闭连接。
                None
            }
            ReadOutcome::BadRequest | ReadOutcome::Io(_) => None,
        }
    }

    /// 读取一条请求（带原因分类）
    pub fn read_request_detailed(&mut self) -> ReadOutcome {
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => ReadOutcome::Eof,
            Ok(_) => {
                let t = line.trim();
                if t.is_empty() {
                    // 空行：忽略，继续读下一行（客户端可能在心跳）
                    return ReadOutcome::WouldBlock;
                }
                match serde_json::from_str(t) {
                    Ok(r) => ReadOutcome::Request(Box::new(r)),
                    Err(e) => {
                        // 协议不兼容 / 非法请求：回结构化错误
                        let resp = BridgeResponse::fail(
                            BridgeError::InvalidArgument {
                                message: format!("无法解析桥接请求: {e}"),
                            },
                            0,
                        );
                        let _ = self.write_response(&resp);
                        ReadOutcome::BadRequest
                    }
                }
            }
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                // ⚠️ 必须与 EOF 区分：非阻塞/超时的暂时无数据**不是**断开。
                ReadOutcome::WouldBlock
            }
            Err(e) => ReadOutcome::Io(e),
        }
    }

    pub fn write_response(&mut self, resp: &BridgeResponse) -> std::io::Result<()> {
        let mut line = serde_json::to_string(resp).unwrap_or_else(|_| {
            r#"{"ok":false,"error":{"error":"INTERNAL","message":"响应序列化失败"},"revision":0}"#
                .to_string()
        });
        line.push('\n');
        self.writer.write_all(line.as_bytes())?;
        self.writer.flush()
    }
}

// ---------------------------------------------------------------------------
// 工具
// ---------------------------------------------------------------------------

pub fn process_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    // macOS/Linux 都可以用 kill -0 探测
    unsafe {
        libc_kill(pid as i32, 0) == 0
    }
}

extern "C" {
    #[link_name = "kill"]
    fn libc_kill(pid: i32, sig: i32) -> i32;
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// 原子写文件：先写 `.tmp` 再 rename，避免读到半个 JSON
pub fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建目录失败: {e}"))?;
    }
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&tmp, content).map_err(|e| format!("写临时文件失败: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("原子替换失败: {e}")
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t01_descriptor_roundtrip() {
        let d = BridgeDescriptor::new(
            PathBuf::from("/tmp/x.sock"),
            std::process::id(),
            Some(PathBuf::from("/tmp/a.screenplay")),
            "测试".into(),
        );
        let back = BridgeDescriptor::from_json(&d.to_json()).unwrap();
        assert_eq!(back.protocol, BridgeDescriptor::PROTOCOL);
        assert_eq!(back.pid, d.pid);
        assert_eq!(back.title, "测试");
        assert_eq!(back.socket, d.socket);
        assert_eq!(back.container, d.container);
    }

    #[test]
    fn t02_descriptor_detects_stale_process() {
        // pid 0 一定不存在
        let d = BridgeDescriptor::new(PathBuf::from("/tmp/x.sock"), 0, None, "x".into());
        assert!(d.is_stale(), "pid 0 应被判定为陈旧");
    }

    #[test]
    fn t03_request_serialization_is_tagged() {
        let req = BridgeRequest::ReplaceBlock {
            block_id: "B-1".into(),
            text: "x".into(),
            expected_revision: 7,
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"op\":\"replace_block\""), "必须有 op tag: {s}");
        assert!(s.contains("\"expected_revision\":7"), "必须带 revision: {s}");
        let back: BridgeRequest = serde_json::from_str(&s).unwrap();
        match back {
            BridgeRequest::ReplaceBlock {
                block_id,
                expected_revision,
                ..
            } => {
                assert_eq!(block_id, "B-1");
                assert_eq!(expected_revision, 7);
            }
            other => panic!("反序列化类型错误: {other:?}"),
        }
    }

    #[test]
    fn t04_unknown_op_is_rejected() {
        let bad = r#"{"op":"drop_database","path":"/"}"#;
        let r: Result<BridgeRequest, _> = serde_json::from_str(bad);
        assert!(r.is_err(), "未知 op 必须被拒绝，不能静默忽略");
    }

    #[test]
    fn t05_error_has_structured_code() {
        let e = BridgeError::RevisionConflict {
            expected: 3,
            actual: 9,
            message: "已被他人修改".into(),
        };
        let j = e.to_json();
        assert_eq!(j["error"], "REVISION_CONFLICT");
        assert_eq!(j["expected"], 3);
        assert_eq!(j["actual"], 9);
        assert!(e.message().contains("rev 3"));
        assert!(e.message().contains("rev 9"));
    }

    #[test]
    fn t06_atomic_write_leaves_no_tmp() {
        let dir = std::env::temp_dir().join(format!("bridge-atomic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.json");
        atomic_write(&f, "{\"a\":1}").unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "{\"a\":1}");
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "不应残留 tmp 文件");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn t07_lock_is_exclusive() {
        let dir = std::env::temp_dir().join(format!("bridge-lock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("proj.screenplay");
        std::fs::write(&fake, b"x").unwrap();

        let first = ProjectLock::acquire(&fake).expect("首次应成功");
        let second = ProjectLock::acquire(&fake).expect_err("二次应失败");
        match second {
            BridgeError::ProjectInUse { .. } => {}
            other => panic!("期望 PROJECT_IN_USE，得到 {other:?}"),
        }
        drop(first);
        // 释放后应能再次获取
        let _third = ProjectLock::acquire(&fake).expect("释放后应能再获取");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn t08_lock_path_is_safe_for_any_container() {
        // 路径含空格、中文、相对路径、上跳都不能逃出锁目录
        let lock_dir = lock_path_for(Path::new("/x")).unwrap().parent().unwrap().to_path_buf();
        for p in [
            "/tmp/a b.screenplay",
            "/tmp/中文项目.screenplay",
            "/tmp/../../etc/passwd",
            "relative.screenplay",
            "",
        ] {
            let lp = lock_path_for(Path::new(p)).unwrap();
            assert_eq!(
                lp.parent().unwrap(),
                lock_dir,
                "所有锁文件必须在同一目录下（防路径穿越）: {p}"
            );
            let name = lp.file_name().unwrap().to_string_lossy().to_string();
            assert!(
                name.ends_with(".lock") && !name.contains('/') && !name.contains(".."),
                "锁文件名必须是纯哈希 + .lock: {name}"
            );
        }
    }
}

#[cfg(test)]mod lock_path_alias_tests{use super::*;
 #[test]fn existing_and_not_yet_created_paths_use_the_same_lock_identity(){let dir=tempfile::tempdir().unwrap();let path=dir.path().join("project.screenplay");let before=lock_path_for(&path);std::fs::write(&path,b"test").unwrap();assert_eq!(before,lock_path_for(&path));let alias=dir.path().join(".").join("project.screenplay");assert_eq!(before,lock_path_for(&alias));let link=dir.path().join("alias.screenplay");std::os::unix::fs::symlink(&path,&link).unwrap();assert_eq!(before,lock_path_for(&link));}
}
