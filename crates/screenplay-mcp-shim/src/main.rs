//! 包内 stdio 桥：把外部 Agent 的 stdio JSON-RPC 原样转发到应用内嵌的
//! 本地 MCP HTTP 服务（127.0.0.1），给只支持 stdio 的客户端用。
//!
//! 只做转发：不改 id、不改方法名、不解析工具语义。令牌取自应用写入的本机
//! 文件（沙盒版位于应用容器内），端口在 8765..=8775 中探测。

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::Duration;

const FIRST_PORT: u16 = 8765;
const LAST_PORT: u16 = 8775;

struct Args {
    port: Option<u16>,
    token: Option<String>,
    token_file: Option<PathBuf>,
}

fn main() {
    let args = parse_args();
    let token = match args.token.clone().or_else(|| find_token(args.token_file.clone())) {
        Some(token) => token,
        None => {
            eprintln!("未找到 MCP 令牌：请从好好写剧本的「连接面板」复制，或用 --token 传入。");
            std::process::exit(2);
        }
    };

    let ports = candidate_ports(args.port);
    let stdin = io::stdin();
    let mut session: Option<String> = None;
    let mut active: Option<u16> = None;

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let is_notification = serde_json::from_str::<serde_json::Value>(trimmed)
            .map(|value| value.get("id").is_none())
            .unwrap_or(false);

        let order = match active {
            Some(port) => vec![port],
            None => ports.clone(),
        };
        let mut last_error = String::new();
        let mut delivered = false;
        for port in order {
            match post(port, &token, session.as_deref(), trimmed) {
                Ok(response) => {
                    if response.status == 404 {
                        last_error = format!("端口 {port} 没有 MCP 服务");
                        continue;
                    }
                    if response.status == 401 {
                        eprintln!("令牌被拒绝（端口 {port}）：请从应用连接面板重新复制令牌。");
                        std::process::exit(3);
                    }
                    active = Some(port);
                    if response.session.is_some() {
                        session = response.session.clone();
                    }
                    if !is_notification {
                        for message in extract_messages(&response.body) {
                            println!("{message}");
                        }
                        let _ = io::stdout().flush();
                    }
                    delivered = true;
                    break;
                }
                Err(e) => last_error = format!("端口 {port}: {e}"),
            }
        }
        if !delivered {
            eprintln!("转发失败：{last_error}");
            std::process::exit(4);
        }
    }
}

fn parse_args() -> Args {
    let mut args = Args { port: None, token: None, token_file: None };
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--token" => args.token = iter.next(),
            "--token-file" => args.token_file = iter.next().map(PathBuf::from),
            "--url" => {
                // 只取端口：服务固定在 127.0.0.1 的 /mcp 路径。
                if let Some(url) = iter.next() {
                    let authority = url.split("//").nth(1).unwrap_or(&url);
                    let port = authority.split('/').next().and_then(|host| host.rsplit(':').next());
                    args.port = port.and_then(|p| p.parse().ok());
                }
            }
            _ => {}
        }
    }
    args
}

fn candidate_ports(explicit: Option<u16>) -> Vec<u16> {
    match explicit {
        Some(port) => vec![port],
        None => (FIRST_PORT..=LAST_PORT).collect(),
    }
}

/// 令牌文件：非沙盒在 `~/Library/Application Support`，沙盒在容器内。
fn find_token(explicit: Option<PathBuf>) -> Option<String> {
    if let Ok(value) = std::env::var("SCREENPLAY_MCP_TOKEN") {
        let value = value.trim().to_string();
        if !value.is_empty() {
            return Some(value);
        }
    }
    let mut candidates: Vec<PathBuf> = explicit.into_iter().collect();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        candidates.push(home.join("Library/Application Support/ScreenplayStudio/mcp-http-token"));
        if let Ok(containers) = std::fs::read_dir(home.join("Library/Containers")) {
            for entry in containers.flatten() {
                candidates.push(
                    entry
                        .path()
                        .join("Data/Library/Application Support/ScreenplayStudio/mcp-http-token"),
                );
            }
        }
    }
    candidates.into_iter().find_map(|path| {
        std::fs::read_to_string(path)
            .ok()
            .map(|text| text.trim().to_string())
            .filter(|text| text.len() >= 32)
    })
}

struct HttpResponse {
    status: u16,
    session: Option<String>,
    body: String,
}

fn post(port: u16, token: &str, session: Option<&str>, body: &str) -> io::Result<HttpResponse> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;

    let mut request = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    if let Some(session) = session {
        request.push_str(&format!("Mcp-Session-Id: {session}\r\n"));
    }
    request.push_str("\r\n");
    request.push_str(body);

    stream.write_all(request.as_bytes())?;
    stream.flush()?;
    read_response(stream)
}

fn read_response(stream: TcpStream) -> io::Result<HttpResponse> {
    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader.read_line(&mut status_line)?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "无法解析状态行"))?;

    let mut content_length: Option<usize> = None;
    let mut chunked = false;
    let mut session = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        let (name, value) = line.split_once(':').unwrap_or((line, ""));
        match name.trim().to_ascii_lowercase().as_str() {
            "content-length" => content_length = value.trim().parse().ok(),
            "transfer-encoding" => chunked = value.to_ascii_lowercase().contains("chunked"),
            "mcp-session-id" => session = Some(value.trim().to_string()),
            _ => {}
        }
    }

    let bytes = if chunked {
        read_chunked(&mut reader)?
    } else if let Some(length) = content_length {
        let mut buffer = vec![0u8; length];
        reader.read_exact(&mut buffer)?;
        buffer
    } else {
        let mut buffer = Vec::new();
        reader.read_to_end(&mut buffer)?;
        buffer
    };

    Ok(HttpResponse {
        status,
        session,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    })
}

fn read_chunked<R: BufRead>(reader: &mut R) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let mut size_line = String::new();
        if reader.read_line(&mut size_line)? == 0 {
            break;
        }
        let size_text = size_line.trim();
        if size_text.is_empty() {
            continue;
        }
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("0"), 16)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "非法 chunk 长度"))?;
        if size == 0 {
            loop {
                let mut trailer = String::new();
                if reader.read_line(&mut trailer)? == 0 || trailer.trim().is_empty() {
                    break;
                }
            }
            break;
        }
        let mut buffer = vec![0u8; size];
        reader.read_exact(&mut buffer)?;
        out.extend_from_slice(&buffer);
        let mut crlf = String::new();
        reader.read_line(&mut crlf)?;
    }
    Ok(out)
}

/// 响应体既可能是裸 JSON，也可能是 SSE（`data:` 行）。
fn extract_messages(body: &str) -> Vec<String> {
    let trimmed = body.trim();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return vec![trimmed.to_string()];
    }
    let mut messages = Vec::new();
    for block in body.split("\n\n") {
        let mut data = String::new();
        for line in block.lines() {
            if let Some(rest) = line.strip_prefix("data:") {
                data.push_str(rest.trim_start());
            }
        }
        if !data.is_empty() {
            messages.push(data);
        }
    }
    messages
}
