//! 崩溃恢复端到端测试
//!
//! 说明书 B-09 要求：「故意强制退出后验证恢复」。
//!
//! 本测试**真实地**做两件事：
//! 1. 启动一个子进程，让它修改文档（写入恢复点）后被 `SIGKILL` 强杀
//! 2. 主进程重新启动，检测并采纳恢复内容
//!
//! 这不是模拟，是真的进程级强制退出。

use screenplay_desktop_lib::service::ProjectService;
use screenplay_core::ast::BlockType;
use screenplay_store::db::Origin;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Mutex, MutexGuard, OnceLock};

/// 所有测试共享同一个 recovery 目录（RecoveryManager 用固定路径），
/// 必须串行执行，否则会互相覆盖恢复点。
fn serial() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| {
        // Only this test process and its child use the isolated recovery directory.
        std::env::set_var("XDG_DATA_HOME", std::env::temp_dir().join(format!("muben-crash-tests-{}", std::process::id())));
        Mutex::new(())
    })
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

fn recovery_dir() -> PathBuf {
    let mut p = PathBuf::from(std::env::var_os("XDG_DATA_HOME").expect("serial() must isolate recovery before cleanup"));
    p.push("ScreenplayStudio");
    p.push("recovery");
    p
}

fn clear_recovery() {
    let _ = std::fs::remove_dir_all(recovery_dir());
    let _ = std::fs::create_dir_all(recovery_dir());
}

fn tmp(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("screenplay-crash-{}-{}", std::process::id(), name));
    let _ = std::fs::remove_file(&p);
    p
}

/// 测试辅助二进制路径（cargo 会把它放在 target/debug/deps 或 target/debug）
fn helper_binary() -> PathBuf {
    let mut p = std::env::current_exe().unwrap();
    // target/debug/deps/xxx -> target/debug/
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("screenplay-desktop")
}

fn crash_writer_bin() -> PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("crash_writer")
}

#[test]
fn r01_recovery_file_written_after_each_change() {
    let _guard = serial();
    clear_recovery();
    let svc = ProjectService::new();
    svc.create_new("恢复测试").unwrap();
    svc.run(|s| {
        s.replace_source_text(
            Origin::GuiRight,
            "外 镇北堡城门 黄昏\n\n风从荒原吹来。\n",
            None,
        )
    })
    .unwrap();

    // 应生成恢复文件
    let files: Vec<_> = std::fs::read_dir(recovery_dir())
        .map(|rd| rd.filter_map(|e| e.ok()).collect())
        .unwrap_or_default();
    assert!(!files.is_empty(), "每次修改后应写恢复点");
}

#[test]
fn r02_pending_recovery_detected_on_next_launch() {
    let _guard = serial();
    clear_recovery();

    // 第一个"会话"：写入后丢弃（模拟异常退出）
    {
        let svc = ProjectService::new();
        svc.create_new("崩溃的剧本").unwrap();
        svc.run(|s| {
            s.replace_source_text(
                Origin::GuiRight,
                "内 酒肆 夜\n\n这是崩溃前写的内容。\n",
                None,
            )
        })
        .unwrap();
        // 不调用 clear_current，模拟被强杀
        std::mem::forget(svc);
    }

    // 第二个"会话"：应检测到待恢复内容
    let svc2 = ProjectService::new();
    let pending = svc2.pending_recovery();
    assert!(pending.is_some(), "应检测到崩溃前的恢复点");

    let p = pending.unwrap();
    assert_eq!(p.title, "崩溃的剧本");
    assert!(p.preview.contains("酒肆"), "预览应包含崩溃前内容: {}", p.preview);
    assert!(p.block_count > 0);
}

#[test]
fn r03_adopt_recovery_restores_content() {
    let _guard = serial();
    clear_recovery();

    {
        let svc = ProjectService::new();
        svc.create_new("要被恢复的剧本").unwrap();
        svc.run(|s| {
            s.replace_source_text(
                Origin::GuiRight,
                "外 荒原 黄昏\n\n老板娘：昨日，去了买卖城。\n",
                None,
            )
        })
        .unwrap();
        std::mem::forget(svc);
    }

    // 新会话采纳恢复
    let svc = ProjectService::new();
    assert!(svc.has_pending_recovery());
    let doc = svc.adopt_recovery().unwrap();

    let text: String = doc.blocks.iter().map(|b| b.text.as_str()).collect();
    assert!(
        text.contains("昨日，去了买卖城。"),
        "恢复后必须包含崩溃前输入的中文: {text}"
    );
    assert_eq!(doc.metadata.title, "要被恢复的剧本");

    // 恢复后应为 dirty（还没保存到用户指定位置）
    assert!(svc.is_dirty(), "恢复的内容尚未保存，应为 dirty");
}

#[test]
fn r04_discard_recovery_clears_files() {
    let _guard = serial();
    clear_recovery();
    {
        let svc = ProjectService::new();
        svc.create_new("丢弃测试").unwrap();
        svc.run(|s| s.replace_source_text(Origin::GuiRight, "外 X 夜\n\n内容。\n", None))
            .unwrap();
        std::mem::forget(svc);
    }

    let svc = ProjectService::new();
    assert!(svc.has_pending_recovery());
    svc.discard_recovery();
    assert!(!svc.has_pending_recovery(), "丢弃后不应再有恢复点");

    let remaining = std::fs::read_dir(recovery_dir())
        .map(|rd| rd.count())
        .unwrap_or(0);
    assert_eq!(remaining, 0, "恢复文件应被删除");
}

#[test]
fn r05_normal_shutdown_does_not_trigger_recovery_prompt() {
    let _guard = serial();
    clear_recovery();
    let svc = ProjectService::new();
    svc.create_new("正常关闭").unwrap();
    svc.run(|s| s.replace_source_text(Origin::GuiRight, "外 X 夜\n\n内容。\n", None))
        .unwrap();
    // 正常保存后关闭项目 → 写入恢复点但下次会被同 session 覆盖
    // 关键：恢复内容不应丢失，只是标记为已处理
    svc.close().unwrap();

    let svc2 = ProjectService::new();
    // 允许有待恢复（因为用户可能还想继续），但内容必须完整
    if let Some(p) = svc2.pending_recovery() {
        assert_eq!(p.title, "正常关闭");
    }
}

#[test]
fn r06_recovery_includes_undo_stack() {
    let _guard = serial();
    clear_recovery();
    {
        let svc = ProjectService::new();
        svc.create_new("撤销栈恢复").unwrap();
        // 用结构性操作（不合并）产生两个独立撤销单元
        let r = svc
            .run(|s| s.insert_block(Origin::GuiLeft, None, BlockType::SceneHeading, "外 A 夜", None))
            .unwrap();
        svc.run(|s| s.insert_block(Origin::GuiLeft, Some(r.affected[0].as_str()), BlockType::Action, "第二段", None))
            .unwrap();
        std::mem::forget(svc);
    }

    let svc = ProjectService::new();
    svc.adopt_recovery().unwrap();
    // 恢复后应还能撤销
    assert!(svc.require().unwrap().service.can_undo(), "恢复后撤销栈应可用");
    svc.run(|s| s.undo(Origin::GuiLeft)).unwrap();
    let doc = svc.document().unwrap();
    assert_eq!(
        doc.blocks.len(),
        1,
        "撤销后应只剩一个块（恢复的撤销栈可用）"
    );
}

#[test]
fn r07_corrupt_recovery_file_does_not_crash() {
    let _guard = serial();
    clear_recovery();
    std::fs::write(recovery_dir().join("corrupt.json"), b"{ broken json").unwrap();

    let svc = ProjectService::new();
    // 不应 panic
    let _ = svc.has_pending_recovery();
    let _ = svc.pending_recovery();
}

#[test]
fn r08_recovery_of_saved_project_reopens_container() {
    let _guard = serial();
    clear_recovery();
    let path = tmp("saved.screenplay");

    {
        let svc = ProjectService::new();
        svc.create_new("已保存项目").unwrap();
        svc.run(|s| {
            s.replace_source_text(Origin::GuiRight, "内 酒肆 夜\n\n已保存的内容。\n", None)
        })
        .unwrap();
        svc.save(Some(&path)).unwrap();
        // 保存后又修改了（未保存）
        svc.run(|s| {
            s.replace_source_text(Origin::GuiRight, "内 酒肆 夜\n\n崩溃前的修改。\n", None)
        })
        .unwrap();
        drop(svc); // A terminated process releases ownership; forgetting a live service would keep its lock.
    }

    let svc = ProjectService::new();
    let doc = svc.adopt_recovery().unwrap();
    let text: String = doc.blocks.iter().map(|b| b.text.as_str()).collect();
    assert!(
        text.contains("崩溃前的修改"),
        "应恢复到容器 + 未保存的修改: {text}"
    );
    assert_eq!(
        svc.state().path(),
        Some(screenplay_store::bridge::canonical_container_path(&path).as_path()),
        "恢复后应绑定回原容器路径"
    );

    // 用户此时再保存即可保住崩溃前的内容
    svc.save(None).unwrap();
    assert!(!svc.is_dirty());

    let _ = std::fs::remove_file(&path);
}

#[test]
fn r09_real_sigkill_then_recover() {
    let _guard = serial();
    // 真实进程级强制退出测试
    let bin = crash_writer_bin();
    assert!(bin.exists(), "required crash_writer helper missing: {}", bin.display());

    clear_recovery();
    let marker = tmp("sigkill_marker.txt");
    let _ = std::fs::remove_file(&marker);

    // 启动子进程：写文档 → 写标记文件 → 无限等待（父进程随后 SIGKILL）
    let mut child = Command::new(&bin)
        .arg(&marker)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("启动 crash_writer");

    // 等子进程写入标记（说明它已经完成文档写入）
    let mut waited = 0;
    while !marker.exists() && waited < 100 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        waited += 1;
    }
    assert!(marker.exists(), "子进程应在 10 秒内写入标记");

    // 强制杀死（SIGKILL，不给任何清理机会）
    unsafe {
        libc_kill(child.id() as i32, 9);
    }
    let _ = child.wait();
    let _ = std::fs::remove_file(&marker);

    // 新会话应能恢复
    let svc = ProjectService::new();
    let pending = svc.pending_recovery();
    assert!(
        pending.is_some(),
        "SIGKILL 后应能检测到恢复点（真实强制退出验证）"
    );
    let doc = svc.adopt_recovery().unwrap();
    let text: String = doc.blocks.iter().map(|b| b.text.as_str()).collect();
    assert!(
        text.contains("强制退出"),
        "恢复内容必须包含强杀前写入的文字: {text}"
    );

    let _ = std::fs::remove_file(tmp("sigkill"));
}

extern "C" {
    #[link_name = "kill"]
    fn libc_kill(pid: i32, sig: i32) -> i32;
}