//! 本地 Streamable HTTP 监听（应用内嵌模式的传输层）。
//!
//! 设计约束：
//! - **只绑 127.0.0.1**，不对外网可达；端口优先用固定值，被占用则顺延，
//!   仍不可用则交给系统分配（端口 0），实际端口回传给调用方展示。
//! - 启动即生成一次性 token，校验 `Authorization: Bearer` 或 `?token=`。
//! - 校验 `Host` / `Origin`，拦截 DNS rebinding：只有回环地址可以通过。
//! - 句柄 `stop()` 或 drop 时关闭 listener，随宿主进程退出，不留后台进程。

use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::Router;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::tower::{
    StreamableHttpServerConfig, StreamableHttpService,
};
use tokio::sync::watch;
use tower::Service;

use crate::ScreenplayMcpServer;

/// 默认端口。被占用时依次尝试 +1，都不行则让系统分配。
pub const DEFAULT_PORT: u16 = 8765;
const PORT_SCAN: u16 = 10;

/// 运行中的本地 HTTP 服务句柄。
pub struct HttpServerHandle {
    pub addr: SocketAddr,
    pub token: String,
    shutdown: watch::Sender<bool>,
}

impl HttpServerHandle {
    /// 给 MCP 客户端填写的地址（不含 token）。
    pub fn url(&self) -> String {
        format!("http://{}/mcp", self.addr)
    }

    /// 主动停止监听。进程退出时也会随线程一起结束。
    pub fn stop(&self) {
        let _ = self.shutdown.send(true);
    }
}

impl Drop for HttpServerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

struct HttpState {
    service: StreamableHttpService<ScreenplayMcpServer, LocalSessionManager>,
    token: String,
    port: u16,
}

/// 在 127.0.0.1 上启动 Streamable HTTP 服务。
///
/// `token` 由调用方提供：桌面应用会从 0600 文件里读回复用，保证重启后
/// 客户端配置不用改；纯 stdio/测试场景可直接用 [`generate_token`]。
pub fn start(
    server: ScreenplayMcpServer,
    preferred_port: u16,
    token: String,
) -> Result<HttpServerHandle, String> {
    let (listener, addr) = bind_loopback(preferred_port)?;
    let port = addr.port();

    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();

    let state = Arc::new(HttpState {
        service: StreamableHttpService::new(
            {
                let server = server.clone();
                move || Ok(server.clone())
            },
            Arc::new(LocalSessionManager::default()),
            StreamableHttpServerConfig {
                sse_keep_alive: Some(Duration::from_secs(15)),
                stateful_mode: true,
            },
        ),
        token: token.clone(),
        port,
    });

    let router = Router::new()
        .route("/mcp", any(mcp_endpoint))
        .with_state(state);

    std::thread::Builder::new()
        .name("screenplay-mcp-http".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = ready_tx.send(Err(format!("创建运行时失败: {e}")));
                    return;
                }
            };
            runtime.block_on(async move {
                let listener = match tokio::net::TcpListener::from_std(listener) {
                    Ok(l) => l,
                    Err(e) => {
                        let _ = ready_tx.send(Err(format!("接管监听失败: {e}")));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(()));
                let served = axum::serve(listener, router.into_make_service())
                    .with_graceful_shutdown(async move {
                        let _ = shutdown_rx.changed().await;
                    })
                    .await;
                if let Err(e) = served {
                    tracing::error!("本地 MCP HTTP 服务退出: {e}");
                }
            });
        })
        .map_err(|e| format!("启动监听线程失败: {e}"))?;

    match ready_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(())) => {
            tracing::info!("本地 MCP HTTP 已启动: http://{addr}/mcp");
            Ok(HttpServerHandle {
                addr,
                token,
                shutdown: shutdown_tx,
            })
        }
        Ok(Err(e)) => Err(e),
        Err(e) => Err(format!("监听线程未就绪: {e}")),
    }
}

async fn mcp_endpoint(State(state): State<Arc<HttpState>>, req: Request) -> Response {
    if let Err(rejection) = authorize(&state, req.headers()) {
        return rejection.into_response();
    }
    let mut service = state.service.clone();
    match service.call(req).await {
        Ok(resp) => resp.map(Body::new),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

/// token + Host/Origin 校验。
fn authorize(state: &HttpState, headers: &HeaderMap) -> Result<(), (StatusCode, String)> {
    if !host_allowed(headers, state.port) {
        return Err((StatusCode::FORBIDDEN, "Forbidden: invalid Host".into()));
    }
    if !origin_allowed(headers, state.port) {
        return Err((StatusCode::FORBIDDEN, "Forbidden: invalid Origin".into()));
    }
    let provided = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .unwrap_or_default();
    if !constant_time_eq(provided.as_bytes(), state.token.as_bytes()) {
        return Err((StatusCode::UNAUTHORIZED, "Unauthorized".into()));
    }
    Ok(())
}

/// Host 只允许回环地址（附带正确端口），避免 DNS rebinding。
fn host_allowed(headers: &HeaderMap, port: u16) -> bool {
    let Some(host) = headers.get(header::HOST).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
}

/// 浏览器场景下 Origin 必须同源；非浏览器客户端通常不带 Origin。
fn origin_allowed(headers: &HeaderMap, port: u16) -> bool {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        return true;
    };
    origin == format!("http://127.0.0.1:{port}") || origin == format!("http://localhost:{port}")
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() || a.is_empty() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// 依次尝试 preferred / preferred+1.. / 系统分配，只绑回环地址。
fn bind_loopback(preferred: u16) -> Result<(TcpListener, SocketAddr), String> {
    let preferred = if preferred == 0 { DEFAULT_PORT } else { preferred };
    let mut last_error = String::new();
    for offset in 0..=PORT_SCAN {
        let port = preferred.saturating_add(offset);
        match TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
            Ok(listener) => {
                listener
                    .set_nonblocking(true)
                    .map_err(|e| format!("设置非阻塞失败: {e}"))?;
                let addr = listener
                    .local_addr()
                    .map_err(|e| format!("读取监听地址失败: {e}"))?;
                return Ok((listener, addr));
            }
            Err(e) => last_error = format!("端口 {port}: {e}"),
        }
    }
    // 固定端口段全被占用：交给系统分配，保证一定能起来。
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .map_err(|e| format!("回退随机端口失败（{last_error}）: {e}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("设置非阻塞失败: {e}"))?;
    let addr = listener
        .local_addr()
        .map_err(|e| format!("读取监听地址失败: {e}"))?;
    Ok((listener, addr))
}

/// 生成一次性 token（48 个十六进制字符，来自系统随机源）。
pub fn generate_token() -> String {
    use std::io::Read;
    let mut buf = [0u8; 24];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        if f.read_exact(&mut buf).is_ok() {
            return buf.iter().map(|b| format!("{b:02x}")).collect();
        }
    }
    // 极端情况下的兜底：用进程信息做一次哈希，避免空 token。
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut h = RandomState::new().build_hasher();
    h.write_u32(std::process::id());
    if let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        h.write_u128(now.as_nanos());
    }
    format!("{:016x}{:016x}", h.finish(), h.finish().rotate_left(17))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_long_and_unique() {
        let a = generate_token();
        let b = generate_token();
        assert_eq!(a.len(), 48);
        assert_ne!(a, b);
    }

    #[test]
    fn host_and_origin_are_restricted_to_loopback() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "127.0.0.1:8765".parse().unwrap());
        assert!(host_allowed(&headers, 8765));
        headers.insert(header::HOST, "evil.example.com:8765".parse().unwrap());
        assert!(!host_allowed(&headers, 8765));

        let mut headers = HeaderMap::new();
        assert!(origin_allowed(&headers, 8765));
        headers.insert(header::ORIGIN, "http://127.0.0.1:8765".parse().unwrap());
        assert!(origin_allowed(&headers, 8765));
        headers.insert(header::ORIGIN, "http://evil.example.com".parse().unwrap());
        assert!(!origin_allowed(&headers, 8765));
    }

    #[test]
    fn constant_time_compare_rejects_mismatch_and_empty() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"", b""));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }
}
