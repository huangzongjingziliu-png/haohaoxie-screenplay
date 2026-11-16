//! 应用内嵌的本地 MCP HTTP 服务（Streamable HTTP）。
//!
//! 只做三件事：把 `screenplay-mcp` 的 ServerHandler 挂到 127.0.0.1，
//! 把它的请求交回本进程现有的命令分发，以及把连接信息交给前端面板。
//! 工具实现、参数校验、revision 冲突检查全部沿用 MCP crate 与 `bridge_host`，
//! 本文件不新增任何业务逻辑。

use serde::Serialize;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::AppHandle;

use crate::{handle_on_main_thread, AppState};

/// token 落盘文件名（位于应用数据目录，沙盒下即容器内可写位置）。
const TOKEN_FILE: &str = "mcp-http-token";

/// 前端面板展示用的连接信息。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpHttpInfo {
    pub running: bool,
    /// 填进 MCP 客户端（Claude Code 等）的地址。
    pub url: String,
    /// 启动时生成的一次性 token，用 `Authorization: Bearer` 发送。
    pub token: String,
    pub port: u16,
    pub error: Option<String>,
}

impl McpHttpInfo {
    pub fn stopped(error: Option<String>) -> Self {
        Self {
            running: false,
            url: String::new(),
            token: String::new(),
            port: 0,
            error,
        }
    }
}

/// 启动本地 MCP HTTP 服务。失败只记日志，不影响 GUI。
pub fn start(app: &AppHandle) -> Result<screenplay_mcp::http::HttpServerHandle, String> {
    let handle = app.clone();
    // Embedded 分支：请求回到本进程现有的分发路径（含 GUI 广播）。
    let dispatcher: screenplay_mcp::EmbeddedHandler =
        Arc::new(move |req| handle_on_main_thread(&handle, req));
    let server = screenplay_mcp::ScreenplayMcpServer::embedded(dispatcher);
    screenplay_mcp::http::start(server, screenplay_mcp::http::DEFAULT_PORT, load_or_create_token())
}

fn token_path() -> Option<PathBuf> {
    screenplay_store::bridge::app_support_dir().map(|d| d.join(TOKEN_FILE))
}

/// 读回已持久化的 token；没有或不可用时生成一份并以 0600 写入。
///
/// 持久化是为了让客户端配置一次即可长期复用；文件放在应用数据目录，
/// 沙盒（App Store 版）下会自动落在容器内。
fn load_or_create_token() -> String {
    let Some(path) = token_path() else {
        crate::diag::log("无法确定应用数据目录，MCP token 仅本次有效");
        return screenplay_mcp::http::generate_token();
    };
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let token = existing.trim().to_string();
        if token.len() >= 32 && token.chars().all(|c| c.is_ascii_hexdigit()) {
            return token;
        }
    }
    let token = screenplay_mcp::http::generate_token();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
    {
        Ok(mut f) => {
            if let Err(e) = f.write_all(token.as_bytes()) {
                crate::diag::log(&format!("写入 MCP token 失败: {e}"));
            }
        }
        Err(e) => crate::diag::log(&format!("创建 MCP token 文件失败: {e}")),
    }
    token
}

pub fn info(state: &AppState) -> McpHttpInfo {
    let guard = state.mcp_http.lock().unwrap();
    match guard.as_ref() {
        Some(handle) => McpHttpInfo {
            running: true,
            url: handle.url(),
            token: handle.token.clone(),
            port: handle.addr.port(),
            error: None,
        },
        None => McpHttpInfo::stopped(None),
    }
}

#[tauri::command]
pub fn mcp_http_info(state: tauri::State<AppState>) -> McpHttpInfo {
    info(&state)
}
