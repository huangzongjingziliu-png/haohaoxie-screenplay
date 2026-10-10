//! 崩溃恢复测试辅助进程（**不参与发布**）
//!
//! 流程：
//! 1. 写一份文档（触发 ProjectService 写恢复点）
//! 2. 写标记文件告诉父进程「我准备好了」
//! 3. 无限等待，被父进程 SIGKILL
//!
//! 这模拟真实的崩溃/强制退出场景（没有机会执行任何清理）。

use screenplay_desktop_lib::service::ProjectService;
use screenplay_store::db::Origin;
use std::path::PathBuf;

fn main() {
    let marker: PathBuf = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("crash_marker.txt"));

    let svc = ProjectService::new();
    if svc.create_new("崩溃测试剧本").is_err() {
        std::process::exit(1);
    }

    let text = "外 镇北堡城门 黄昏\n\n风从荒原吹来。\n\n老板娘（冷笑）：强制退出前写下的内容。\n";
    if svc
        .run(|s| s.replace_source_text(Origin::GuiRight, text, None))
        .is_err()
    {
        std::process::exit(2);
    }

    // 告诉父进程可以 kill 了
    let _ = std::fs::write(&marker, b"ready");

    // 无限等待被强杀
    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
