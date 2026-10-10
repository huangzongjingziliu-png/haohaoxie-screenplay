//! 运行时诊断日志。
//!
//! 目的：让 GUI 的真实运行行为**可被外部验证**。
//! 没有屏幕录制/辅助功能权限时，截图与 AppleScript 都不可用，
//! 但「命令是否被真实调用、参数是什么、返回什么」必须有证据。
//!
//! 日志写入 `~/.screenplay-runtime.log`，仅在 `SCREENPLAY_DIAG=1` 时启用，
//! 不影响正常使用的性能与隐私。

use std::io::Write;
use std::sync::Mutex;
use std::sync::OnceLock;

static ENABLED: OnceLock<bool> = OnceLock::new();
static LOG_PATH: OnceLock<Mutex<Option<std::path::PathBuf>>> = OnceLock::new();

fn enabled() -> bool {
    *ENABLED.get_or_init(|| std::env::var("SCREENPLAY_DIAG").is_ok())
}

pub fn init() {
    if !enabled() {
        return;
    }
    let mut p = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    p.push(".screenplay-runtime.log");
    let _ = std::fs::write(&p, b"");
    let slot = LOG_PATH.get_or_init(|| Mutex::new(None));
    *slot.lock().unwrap() = Some(p);
    log("=== 好好写剧本运行时诊断日志已启动 ===");
}

/// 记录一条诊断信息（带时间戳）
pub fn log(msg: &str) {
    if !enabled() {
        return;
    }
    let guard = LOG_PATH.get_or_init(|| Mutex::new(None));
    let path = guard.lock().unwrap().clone();
    let path = match path {
        Some(p) => p,
        None => return,
    };
    let ts = chrono::Local::now().format("%H:%M:%S%.3f");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(f, "[{ts}] {msg}");
    }
}

/// 记录一次 IPC 命令调用
pub fn command(name: &str, args_summary: &str, result: &str) {
    log(&format!("IPC {name} args={args_summary} -> {result}"));
}

pub fn path() -> Option<std::path::PathBuf> {
    LOG_PATH.get().and_then(|m| m.lock().unwrap().clone())
}