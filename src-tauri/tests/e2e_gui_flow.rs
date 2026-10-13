//! 端到端闭环测试：模拟 GUI 的完整调用序列
//!
//! 说明书 A 阶段验收要求：
//! 新建 → 右侧写中文 → 左侧修改 → 保存 → 关闭 → 重开，
//! 文字、结构、项目身份与设置一致。
//!
//! 这个测试**不使用 mock**，直接驱动真实的 `ProjectService`，
//! 它与 GUI 的 Tauri 命令调用的是同一套代码路径
//! （GUI 命令层只是 `ProjectService` 的薄包装）。
//!
//! 真实 GUI E2E（含渲染、IME）见 `qa/evidence/A/e2e/`。

use screenplay_core::ast::BlockType;
use screenplay_core::serializer;
use screenplay_store::db::Origin;
use screenplay_store::workspace::Workspace;

use screenplay_desktop_lib::service::{AutosaveAction, ProjectService};

fn isolated_service() -> ProjectService {
    ProjectService::with_recovery_directory(std::env::temp_dir().join(screenplay_core::ast::new_id("muben-e2e-recovery")))
}

fn tmp(name: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("screenplay-e2e-{}-{}", std::process::id(), name));
    let _ = std::fs::remove_file(&p);
    p
}

#[test]
fn e2e_新建_写中文_左侧修改_保存_关闭_重开() {
    let svc = isolated_service();
    let path = tmp("full.screenplay");

    // ---- 1. 新建项目 ----
    svc.create_new("镇北堡").unwrap();
    assert!(svc.is_open());
    assert_eq!(svc.state().title(), "镇北堡");

    // ---- 2. 右侧写中文（replace_source_text，走完整解析器）----
    let script = "外 镇北堡城门 黄昏\n\n风从荒原吹来。\n\n老板娘（冷笑）：昨日，去了买卖城。\n";
    let r = svc
        .run(|s| s.replace_source_text(Origin::GuiRight, script, None))
        .unwrap();

    assert!(r.revision > 0, "写入后 revision 必须递增");
    let block_count = r.document.blocks.len();
    assert!(block_count >= 3, "应解析出多个块，实际 {block_count}");

    // 验证结构：场景标题 + 动作 + 人物 + 括注 + 对白
    let types: Vec<BlockType> = r.document.blocks.iter().map(|b| b.block_type).collect();
    assert!(
        types.contains(&BlockType::SceneHeading),
        "应识别出场景标题，实际 {types:?}"
    );
    assert!(
        types.contains(&BlockType::Dialogue),
        "应识别出对白，实际 {types:?}"
    );
    assert!(
        types.contains(&BlockType::Character),
        "应识别出人物，实际 {types:?}"
    );

    // 保存中文没有丢失
    let all_text = r
        .document
        .blocks
        .iter()
        .map(|b| b.text.as_str())
        .collect::<String>();
    assert!(
        all_text.contains("镇北堡城门"),
        "中文内容必须完整保留，实际: {all_text}"
    );
    assert!(
        all_text.contains("昨日"),
        "对白内容必须保留，实际: {all_text}"
    );

    // ---- 3. 左侧修改（replace_block，带 expectedRevision）----
    let dialogue_id = r
        .document
        .blocks
        .iter()
        .find(|b| b.block_type == BlockType::Dialogue)
        .expect("应有对白块")
        .id
        .clone();

    let rev_before = r.revision;
    // 左侧修改为**不同**的文本（no-op 不递增 revision 是有意设计）
    let new_dialogue = "昨日，去了那座城。";
    let r2 = svc
        .run(|s| {
            s.replace_block(Origin::GuiLeft, &dialogue_id, new_dialogue, Some(rev_before))
        })
        .unwrap();

    assert_eq!(r2.revision, rev_before + 1, "revision 必须 +1");
    let updated = r2
        .document
        .blocks
        .iter()
        .find(|b| b.id == dialogue_id)
        .unwrap();
    assert_eq!(updated.text, new_dialogue, "左侧修改必须生效");

    // 过期 revision 必须被拒绝
    let stale = svc.run(|s| {
        s.replace_block(Origin::GuiLeft, &dialogue_id, "过期写入", Some(rev_before))
    });
    assert!(
        stale.is_err(),
        "过期 expectedRevision 必须被拒绝，否则会覆盖用户内容"
    );

    // ---- 4. 设置（标题页 / 纸张）----
    let r3 = svc
        .run(|s| {
            s.mutate_document(Origin::GuiLeft, |doc| {
                doc.title_page.written_by = Some("黄宗敬".into());
                doc.settings.paper.template = "a4".into();
                doc.settings.paper.margin_left_inch = 1.25;
            })
        })
        .unwrap();

    // ---- 5. 保存 ----
    let outcome = svc.save(Some(&path)).unwrap();
    assert_eq!(outcome.path, screenplay_store::bridge::canonical_container_path(&path).to_string_lossy());
    assert_eq!(outcome.saved_revision, r3.revision, "保存的 revision 必须是最新的");
    assert!(!svc.is_dirty(), "保存后应为 clean");

    let expected_source = serializer::to_compact_source(&r3.document);
    let expected_blocks = r3.document.blocks.len();
    let expected_title = r3.document.title_page.title.clone();
    let expected_writer = r3.document.title_page.written_by.clone().unwrap();
    let expected_template = r3.document.settings.paper.template.clone();
    let expected_margin = r3.document.settings.paper.margin_left_inch;
    let expected_characters: Vec<String> = r3
        .document
        .characters
        .iter()
        .map(|c| c.name.clone())
        .collect();

    // ---- 6. 关闭 ----
    svc.close().unwrap();
    assert!(!svc.is_open());
    assert!(svc.document().is_err(), "关闭后不得能读文档");

    // ---- 7. 重开 ----
    svc.open(&path).unwrap();
    let doc = svc.document().unwrap();

    assert_eq!(
        doc.blocks.len(),
        expected_blocks,
        "重开后块数量必须一致"
    );
    assert_eq!(doc.title_page.title, expected_title, "标题页必须一致");
    assert_eq!(
        doc.title_page.written_by.as_deref(),
        Some(expected_writer.as_str()),
        "编剧必须一致"
    );
    assert_eq!(doc.settings.paper.template, expected_template, "纸张设置必须一致");
    assert_eq!(
        doc.settings.paper.margin_left_inch, expected_margin,
        "页边距必须一致"
    );

    let reopened_text = serializer::to_compact_source(&doc);
    assert_eq!(
        reopened_text, expected_source,
        "重开后的自由文本必须与保存前完全一致"
    );

    // 关键：中文内容与左侧修改都要保留
    assert!(
        reopened_text.contains("镇北堡城门"),
        "重开后中文场景标题必须保留"
    );
    assert!(
        reopened_text.contains(new_dialogue),
        "重开后左侧修改必须保留: {reopened_text}"
    );
    assert!(
        !reopened_text.contains("昨日，去了买卖城。"),
        "旧文本不应残留: {reopened_text}"
    );

    // 人物实体（派生数据）也要重建
    assert_eq!(
        doc.characters.iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
        expected_characters,
        "人物实体必须一致"
    );

    // 项目身份正确
    assert_eq!(svc.state().path(), Some(screenplay_store::bridge::canonical_container_path(&path).as_path()));
    assert!(!svc.is_dirty(), "刚打开应为 clean");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn e2e_新建后保存不会写入上一个项目() {
    let svc = isolated_service();
    let path_a = tmp("a.screenplay");
    let path_b = tmp("b.screenplay");

    // 项目 A
    svc.create_new("A").unwrap();
    svc.run(|s| {
        s.replace_source_text(Origin::GuiRight, "外 城门 黄昏\n\nA 的内容。\n", None)
    })
    .unwrap();
    svc.save(Some(&path_a)).unwrap();

    // 项目 B
    svc.create_new("B").unwrap();
    svc.run(|s| {
        s.replace_source_text(Origin::GuiRight, "内 酒肆 夜\n\nB 的内容。\n", None)
    })
    .unwrap();
    svc.save(Some(&path_b)).unwrap();

    // A 必须还是 A
    let a = Workspace::open(&path_a).unwrap().document();
    let a_text: String = a.blocks.iter().map(|b| b.text.as_str()).collect();
    assert!(a_text.contains("A 的内容"), "A 应保留自己的内容: {a_text}");
    assert!(!a_text.contains("B 的内容"), "A 不应被 B 污染: {a_text}");

    // B 是 B
    let b = Workspace::open(&path_b).unwrap().document();
    let b_text: String = b.blocks.iter().map(|b| b.text.as_str()).collect();
    assert!(b_text.contains("B 的内容"), "B 应有自己的内容: {b_text}");

    let _ = std::fs::remove_file(&path_a);
    let _ = std::fs::remove_file(&path_b);
}

#[test]
fn e2e_未保存的新项目拒绝无路径保存() {
    let svc = isolated_service();
    svc.create_new("未保存").unwrap();
    // 没有绑定路径，save(None) 必须报错而不是随便找个地方写
    let r = svc.save(None);
    assert!(r.is_err(), "未指定路径时必须报错，不能静默写到未知位置");
    assert_eq!(
        svc.state().path(),
        None,
        "保存失败后不应被绑定到某个路径"
    );
}

#[test]
fn e2e_保存期间的新修改不会被标记为已保存() {
    let svc = isolated_service();
    let path = tmp("inflight.screenplay");

    svc.create_new("并发").unwrap();
    svc.run(|s| {
        s.replace_source_text(Origin::GuiRight, "外 城门 黄昏\n\n初始内容。\n", None)
    })
    .unwrap();
    svc.save(Some(&path)).unwrap();
    assert!(!svc.is_dirty());

    // 修改后再保存，模拟「保存期间又发生修改」
    svc.run(|s| {
        s.replace_source_text(Origin::GuiRight, "外 城门 黄昏\n\n第二版内容。\n", None)
    })
    .unwrap();
    assert!(svc.is_dirty(), "修改后必须 dirty");

    svc.save(Some(&path)).unwrap();
    assert!(!svc.is_dirty());

    // 验证落盘的是第二版
    let reloaded = Workspace::open(&path).unwrap().document();
    let t: String = reloaded.blocks.iter().map(|b| b.text.as_str()).collect();
    assert!(t.contains("第二版内容"), "应保存最新版本: {t}");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn e2e_放弃修改恢复到磁盘版本() {
    let svc = isolated_service();
    let path = tmp("revert.screenplay");

    svc.create_new("回退").unwrap();
    svc.run(|s| {
        s.replace_source_text(Origin::GuiRight, "外 城门 黄昏\n\n磁盘上的内容。\n", None)
    })
    .unwrap();
    svc.save(Some(&path)).unwrap();

    // 做一堆修改
    svc.run(|s| s.replace_source_text(Origin::GuiRight, "临时改的内容", None))
        .unwrap();
    assert!(svc.is_dirty());

    // 放弃
    svc.revert().unwrap();
    assert!(!svc.is_dirty(), "放弃后应为 clean");

    let doc = svc.document().unwrap();
    let t: String = doc.blocks.iter().map(|b| b.text.as_str()).collect();
    assert!(t.contains("磁盘上的内容"), "应恢复到磁盘版本: {t}");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn e2e_版本历史跨重开保留() {
    let svc = isolated_service();
    let path = tmp("versions.screenplay");

    svc.create_new("版本").unwrap();
    svc.run(|s| s.replace_source_text(Origin::GuiRight, "外 城门 黄昏\n\n初稿。\n", None))
        .unwrap();
    svc.save(Some(&path)).unwrap();
    svc.run(|s| s.create_version(Origin::GuiLeft, "draft1", "第一稿"))
        .unwrap();

    svc.run(|s| {
        s.replace_source_text(Origin::GuiRight, "外 城门 黄昏\n\n第二稿。\n", None)
    })
    .unwrap();
    svc.save(Some(&path)).unwrap();
    svc.run(|s| s.create_version(Origin::GuiLeft, "draft2", "第二稿"))
        .unwrap();
    // 关键：版本是 SQLite 内容，必须再保存一次才落盘
    svc.save(Some(&path)).unwrap();

    let before = svc.require().unwrap().service.list_versions();
    assert_eq!(before.len(), 2, "应有 2 个版本");

    // 关闭重开
    svc.close().unwrap();
    svc.open(&path).unwrap();
    let after = svc.require().unwrap().service.list_versions();

    assert_eq!(after.len(), 2, "重开后版本历史必须保留");
    assert_eq!(after[0].name, "draft2");
    assert_eq!(after[1].name, "draft1");

    // 审计日志也要保留
    let audit = svc.require().unwrap().service.audit_log(100);
    assert!(!audit.is_empty(), "审计日志必须保留");
    assert!(
        audit.iter().any(|e| e.origin == "cli" || e.origin == "gui-left"),
        "审计日志必须记录来源"
    );

    // 恢复 draft1
    let v1 = after.iter().find(|v| v.name == "draft1").unwrap();
    let r = svc
        .run(|s| s.restore_version(Origin::GuiLeft, &v1.id))
        .unwrap();
    let t: String = r.document.blocks.iter().map(|b| b.text.as_str()).collect();
    assert!(t.contains("初稿"), "恢复后应回到初稿: {t}");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn e2e_导入替换整个文档且可保存重开() {
    let svc = isolated_service();
    let path = tmp("import.screenplay");
    let src = tmp("import_src.txt");

    std::fs::write(
        &src,
        "内 酒肆 夜\n\n烛火摇曳。\n\n酒客：来壶烧刀子。\n",
    )
    .unwrap();

    svc.create_new("导入目标").unwrap();
    let r = svc.import_file(&src).unwrap();
    assert!(r.document.blocks.len() >= 2, "导入应产生块");

    svc.save(Some(&path)).unwrap();
    svc.close().unwrap();
    svc.open(&path).unwrap();

    let doc = svc.document().unwrap();
    let t: String = doc.blocks.iter().map(|b| b.text.as_str()).collect();
    assert!(t.contains("烛火摇曳"), "导入内容应保留: {t}");
    assert!(t.contains("烧刀子"), "对白应保留: {t}");

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&src);
}

#[test]
fn e2e_损坏项目文件返回结构化错误而非崩溃() {
    let svc = isolated_service();
    let path = tmp("corrupt.screenplay");
    std::fs::write(&path, b"this is definitely not a zip archive").unwrap();

    let r = svc.open(&path);
    assert!(r.is_err(), "损坏文件必须返回错误");
    let msg = r.unwrap_err();
    assert!(
        msg.contains("容器") || msg.contains("损坏") || msg.contains("zip"),
        "错误消息应可理解: {msg}"
    );
    // 不能 panic，且不能处于半打开状态
    assert!(!svc.is_open(), "打开失败后应保持关闭状态");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn e2e_不存在的路径返回错误() {
    let svc = isolated_service();
    let r = svc.open(std::path::Path::new("/definitely/not/here.screenplay"));
    assert!(r.is_err());
    assert!(!svc.is_open());
}

// ---------------------------------------------------------------------------
// 自动保存 / 自动版本（B-03）
// ---------------------------------------------------------------------------

#[test]
fn autosave_保存已打开项目() {
    let svc = isolated_service();
    let path = tmp("autosave.screenplay");

    svc.create_new("自动保存").unwrap();
    svc.run(|s| {
        s.replace_source_text(Origin::GuiRight, "外 城门 黄昏\n\n初始。\n", None)
    })
    .unwrap();
    svc.save(Some(&path)).unwrap();

    // 关闭自动保存，先确认 baseline
    svc.configure_autosave(false, 0, 0);
    svc.run(|s| {
        s.replace_source_text(Origin::GuiRight, "外 城门 黄昏\n\n第二版。\n", None)
    })
    .unwrap();
    assert!(svc.autosave_tick().unwrap().is_none(), "关闭时不应自动保存");

    // 打开自动保存
    svc.configure_autosave(true, 1, 0);
    svc.run(|s| {
        s.replace_source_text(Origin::GuiRight, "外 城门 黄昏\n\n第三版。\n", None)
    })
    .unwrap();

    assert!(svc.autosave_tick().unwrap().is_none(), "间隔未到不得保存");
    std::thread::sleep(std::time::Duration::from_millis(1050));
    let action = svc.autosave_tick().unwrap();
    assert!(matches!(action, Some(AutosaveAction::Saved)), "应自动保存，实际 {action:?}");
    assert!(!svc.is_dirty(), "自动保存后应为 clean");

    // 验证磁盘内容是最新版
    let reloaded = Workspace::open(&path).unwrap().document();
    let t: String = reloaded.blocks.iter().map(|b| b.text.as_str()).collect();
    assert!(t.contains("第三版"), "磁盘应已是最新: {t}");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn autosave_未保存新项目跳过() {
    let svc = isolated_service();
    svc.create_new("未保存项目").unwrap();
    svc.run(|s| {
        s.replace_source_text(Origin::GuiRight, "外 X 夜\n\n内容。\n", None)
    })
    .unwrap();

    svc.configure_autosave(true, 1, 0);
    let action = svc.autosave_tick().unwrap();
    assert!(
        action.is_none(),
        "未保存的新项目没有路径，不能自动保存（否则会写到未知位置）"
    );
    assert!(svc.is_dirty(), "仍应为 dirty");
}

#[test]
fn autosave_无修改时不触发() {
    let svc = isolated_service();
    let path = tmp("autosave_noop.screenplay");
    svc.create_new("无修改").unwrap();
    svc.run(|s| s.replace_source_text(Origin::GuiRight, "外 X 夜\n\n内容。\n", None))
        .unwrap();
    svc.save(Some(&path)).unwrap();

    svc.configure_autosave(true, 1, 0);
    assert!(svc.autosave_tick().unwrap().is_none(), "没有新修改时不应保存");
    assert!(!svc.is_dirty());

    let _ = std::fs::remove_file(&path);
}

#[test]
fn autosave_自动创建版本检查点() {
    let svc = isolated_service();
    let path = tmp("autoversion.screenplay");
    svc.create_new("自动版本").unwrap();
    svc.run(|s| s.replace_source_text(Origin::GuiRight, "外 A 夜\n\n甲。\n", None))
        .unwrap();
    svc.save(Some(&path)).unwrap();

    // 只开自动版本，不开自动保存
    svc.configure_autosave(true, 0, 1);

    std::thread::sleep(std::time::Duration::from_millis(1050));
    // 到达真实时间间隔后，新修改触发版本检查点
    let mut last = None;
    for i in 0..8 {
        svc.run(|s| {
            s.replace_source_text(Origin::GuiRight, &format!("外 A 夜\n\n第{}版。\n", i), None)
        })
        .unwrap();
        if let Some(a) = svc.autosave_tick().unwrap() {
            last = Some(a);
            break;
        }
    }
    assert!(
        matches!(last, Some(AutosaveAction::VersionCreated)),
        "应自动创建版本检查点，实际 {last:?}"
    );

    // 版本应能在 SQLite 中查到
    svc.save(Some(&path)).unwrap();
    let versions = svc.require().unwrap().service.list_versions();
    assert!(!versions.is_empty(), "应有自动创建的版本");
    assert!(
        versions.iter().any(|v| v.auto),
        "应是自动版本（auto=true）"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn autosave_配置可读写() {
    let svc = isolated_service();
    svc.configure_autosave(true, 45, 600);
    let cfg = svc.autosave_config();
    assert!(cfg.enabled);
    assert_eq!(cfg.save_interval_secs, 45);
    assert_eq!(cfg.version_interval_secs, 600);

    svc.configure_autosave(false, 0, 0);
    let cfg2 = svc.autosave_config();
    assert!(!cfg2.enabled);
    assert_eq!(cfg2.save_interval_secs, 0);
}

#[test]
fn autosave_关闭时不崩溃() {
    let svc = isolated_service();
    // 未打开项目时调用
    assert!(svc.autosave_tick().unwrap().is_none());
}

// ---------------------------------------------------------------------------
// B5：专业编辑器结构操作（回车拆分 / 退格合并 / 类型切换）
// 这些是可见纸面真实按键驱动的命令，必须走完整 ProjectService 路径。
// ---------------------------------------------------------------------------

#[test]
fn editor_回车拆分块并按剧本语义切换类型() {
    let svc = isolated_service();
    svc.create_new("拆分").unwrap();
    let r = svc
        .run(|s| {
            s.replace_source_text(
                Origin::GuiRight,
                "外 城门 黄昏\n\n老板娘（冷笑）：昨日，去了买卖城。\n",
                None,
            )
        })
        .unwrap();

    let char_id = r
        .document
        .blocks
        .iter()
        .find(|b| b.block_type == BlockType::Character)
        .map(|b| b.id.clone())
        .expect("应有人物块");

    // 回车：人物 → 新建括注块（剧本语义）
    let r2 = svc
        .run(|s| s.insert_block(Origin::GuiLeft, Some(&char_id), BlockType::Parenthetical, "", None))
        .unwrap();

    let idx = r2.document.blocks.iter().position(|b| b.id == char_id).unwrap();
    assert_eq!(
        r2.document.blocks[idx + 1].block_type,
        BlockType::Parenthetical,
        "人物块后回车应插入括注"
    );
    assert_eq!(r2.document.blocks.len(), r.document.blocks.len() + 1);

    // 一次撤销必须完整回退（拆分是独立撤销单元）
    svc.run(|s| s.undo(Origin::GuiLeft)).unwrap();
    assert_eq!(
        svc.document().unwrap().blocks.len(),
        r.document.blocks.len(),
        "撤销拆分应恢复原块数"
    );
}

#[test]
fn editor_退格合并相邻块且保留稳定ID() {
    let svc = isolated_service();
    svc.create_new("合并").unwrap();
    let r = svc
        .run(|s| {
            s.replace_source_text(
                Origin::GuiRight,
                "外 城门 黄昏\n\n风从荒原吹来。\n",
                None,
            )
        })
        .unwrap();

    let action = r
        .document
        .blocks
        .iter()
        .find(|b| b.block_type == BlockType::Action)
        .expect("应有动作块");
    let prev = r.document.blocks[r.document.blocks.iter().position(|b| b.id == action.id).unwrap() - 1]
        .clone();

    let merged = svc
        .run(|s| s.merge_blocks(Origin::GuiLeft, &prev.id, &action.id, None))
        .unwrap();

    assert_eq!(merged.document.blocks.len(), r.document.blocks.len() - 1);
    let kept = merged
        .document
        .blocks
        .iter()
        .find(|b| b.id == prev.id)
        .expect("合并后应保留前一块");
    assert_eq!(kept.text, format!("{}{}", prev.text, action.text));
    assert!(!merged.document.blocks.iter().any(|b| b.id == action.id));

    // 保存重开后合并结果仍在（真实持久化）
    let path = tmp("merge.screenplay");
    svc.save(Some(&path)).unwrap();
    let reopened = Workspace::open(&path).unwrap().document();
    assert!(
        reopened.blocks.iter().any(|b| b.text.contains("风从荒原吹来")),
        "合并后的文字应被持久化"
    );
    assert!(!reopened.blocks.iter().any(|b| b.id == action.id));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn editor_类型切换保留ID且可撤销() {
    let svc = isolated_service();
    svc.create_new("类型").unwrap();
    let r = svc
        .run(|s| {
            s.replace_source_text(Origin::GuiRight, "外 城门 黄昏\n\n风从荒原吹来。\n", None)
        })
        .unwrap();
    let bid = r.document.blocks[1].id.clone();
    let original_type = r.document.blocks[1].block_type;

    let changed = svc
        .run(|s| s.set_block_type(Origin::GuiLeft, &bid, BlockType::Shot, None))
        .unwrap();
    let b = changed.document.blocks.iter().find(|b| b.id == bid).unwrap();
    assert_eq!(b.block_type, BlockType::Shot);
    assert_eq!(b.id, bid, "类型切换必须保留稳定 ID");
    assert_eq!(b.text, r.document.blocks[1].text, "类型切换不应改动文字");

    svc.run(|s| s.undo(Origin::GuiLeft)).unwrap();
    let back = svc.document().unwrap();
    let b2 = back.blocks.iter().find(|b| b.id == bid).unwrap();
    assert_eq!(b2.block_type, original_type, "撤销应恢复原类型");
}

#[test]
fn editor_过期revision的结构操作被拒绝() {
    let svc = isolated_service();
    svc.create_new("冲突").unwrap();
    let r = svc
        .run(|s| s.replace_source_text(Origin::GuiRight, "外 城门 黄昏\n\n内容。\n", None))
        .unwrap();
    let ids: Vec<String> = r.document.blocks.iter().map(|b| b.id.clone()).collect();
    let stale = r.revision;

    // 制造一次新修改，使 stale 过期
    svc.run(|s| s.replace_block(Origin::Cli, &ids[1], "外部改动", None))
        .unwrap();

    let err = svc
        .run(|s| s.merge_blocks(Origin::Mcp, &ids[0], &ids[1], Some(stale)))
        .unwrap_err();
    assert!(
        matches!(err, screenplay_store::command::CommandError::RevisionConflict { .. }),
        "过期 revision 必须被拒绝，得到 {err:?}"
    );

    let err2 = svc
        .run(|s| s.set_block_type(Origin::Mcp, &ids[0], BlockType::Shot, Some(stale)))
        .unwrap_err();
    assert!(
        matches!(err2, screenplay_store::command::CommandError::RevisionConflict { .. }),
        "过期 revision 必须被拒绝，得到 {err2:?}"
    );
}

#[test]
fn autosave_失败保留修改并允许重试() {
    let svc = isolated_service();
    let folder = tmp("autosave-failure-folder");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("draft.screenplay");
    svc.create_new("保存失败验收").unwrap();
    svc.save(Some(&path)).unwrap();
    svc.configure_autosave(true, 1, 0);
    svc.run(|s| s.replace_source_text(Origin::GuiRight, "外 城门 夜\n\n不能丢失的最后一句。", None)).unwrap();
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir(&folder).unwrap();
    std::fs::write(&folder, "占用目录路径").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1050));
    assert!(svc.autosave_tick().is_err());
    assert!(svc.is_dirty());
    std::fs::remove_file(&folder).unwrap();
    std::fs::create_dir(&folder).unwrap();
    assert_eq!(svc.autosave_tick().unwrap(), Some(AutosaveAction::Saved));
    assert!(Workspace::open(&path).unwrap().document().blocks.iter().any(|b| b.text.contains("最后一句")));
    let _ = std::fs::remove_dir_all(&folder);
}
