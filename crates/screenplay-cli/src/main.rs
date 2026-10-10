//! screenplay CLI —— 剧本工作台命令行入口
//!
//! # 两条铁律（说明书第 57 / 82 条）
//!
//! ## 1. 与 GUI 共享同一个权威项目
//!
//! 好好写剧本 GUI 正在运行时，CLI **不会**自己解压 `.screenplay`：
//! 它连上 GUI 的本地桥接（见 `screenplay_store::bridge`），
//! 操作 GUI 当前打开的项目。效果：
//! - 修改实时出现在 GUI 界面
//! - 共享同一个 revision 序列与撤销栈
//! - GUI 保存时一并落盘（CLI 不会私自覆盖磁盘）
//!
//! GUI 未运行时才进入独立进程模式，并用文件锁与其他进程互斥。
//!
//! ## 2. 写操作强制 `--expect-revision`
//!
//! 写命令必须显式给出 `--expect-revision <你读到的 revision>`。
//! 缺失时命令直接报错；过期时后端返回 `REVISION_CONFLICT`。
//! **不允许** CLI 替用户「猜」一个当前 revision —— 那等于绕过冲突检查。

use anyhow::Result;
use clap::{Parser, Subcommand};
use screenplay_core::ast::BlockType;
use screenplay_store::bridge::{
    BridgeClient, BridgeError, BridgeRequest, BridgeResponse, ProjectLock,
};
use screenplay_store::db::Origin;
use screenplay_store::workspace::Workspace;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "screenplay", about = "好好写剧本 · 专业剧本编辑器 CLI", version)]
struct Cli {
    /// 项目容器路径（GUI 正在运行时会被忽略：CLI 操作 GUI 当前项目）
    #[arg(short, long, global = true)]
    project: Option<PathBuf>,
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// 新建空白项目
    New {
        title: Option<String>,
        /// 保存到指定路径
        #[arg(short, long)]
        save_as: Option<PathBuf>,
    },
    /// 打开项目（GUI 运行时为「让 GUI 打开」）
    Open { path: PathBuf },
    /// 导入外部文件（PDF/DOCX/FDX/Fountain/TXT/MD/RTF/图片 OCR）
    Import {
        file: PathBuf,
        /// 导入后保存到指定路径（独立模式）
        #[arg(short, long)]
        save_as: Option<PathBuf>,
    },
    /// 导出为指定格式
    Export {
        file: PathBuf,
        #[arg(short, long)]
        format: Option<String>,
    },
    /// 列出场景
    Scenes,
    /// 列出所有 block
    Blocks,
    /// 搜索文本
    Search {
        query: String,
        #[arg(short, long)]
        case: bool,
    },
    /// 显示文档信息
    Info,
    /// 显示当前操作模式（bridge / standalone）与 revision
    Status,
    /// 撤销一步（GUI 运行时与 GUI 共享撤销栈）
    Undo,
    /// 重做一步
    Redo,

    /// 替换 block 文本
    Replace {
        id: String,
        text: String,
        /// 你读到的 revision（必填）
        #[arg(long)]
        expect_revision: u64,
    },
    /// 插入 block
    Insert {
        #[arg(short, long)]
        after: Option<String>,
        block_type: String,
        text: String,
        /// 你读到的 revision（必填）
        #[arg(long)]
        expect_revision: u64,
    },
    /// 删除 block
    Delete {
        id: String,
        /// 你读到的 revision（必填）
        #[arg(long)]
        expect_revision: u64,
    },
    /// 修改 block 元素类型（保留 block id）
    SetType {
        id: String,
        block_type: String,
        /// 你读到的 revision（必填）
        #[arg(long)]
        expect_revision: u64,
    },
    /// 保存当前项目
    Save {
        /// 另存为路径（省略则保存到原路径）
        path: Option<PathBuf>,
    },
    /// 关闭当前项目
    Close,

    /// 版本管理
    Version {
        #[command(subcommand)]
        sub: VersionCmd,
    },
    /// 审计日志
    Log {
        #[arg(short, long, default_value_t = 20)]
        limit: usize,
    },
}

#[derive(Subcommand)]
enum VersionCmd {
    /// 列出版本
    List,
    /// 创建命名版本
    Create {
        name: String,
        #[arg(short, long, default_value = "")]
        note: String,
    },
    /// 恢复到指定版本（会先自动 checkpoint，且可撤销）
    Restore { id: String },
    /// 比较两个版本
    Diff { a: String, b: Option<String> },
}

// ---------------------------------------------------------------------------
// 后端：桥接（GUI 运行中）或独立进程
// ---------------------------------------------------------------------------

enum Backend {
    Bridge(Box<BridgeClient>),
    Standalone {
        ws: Arc<Workspace>,
        _lock: Option<Box<ProjectLock>>,
    },
}

impl Backend {
    fn mode(&self) -> &'static str {
        match self {
            Backend::Bridge(_) => "bridge(GUI 当前项目)",
            Backend::Standalone { .. } => "standalone(独立进程)",
        }
    }
}

fn connect(project: Option<&PathBuf>, title: &str) -> Result<Backend> {
    // 1) 优先连 GUI
    match BridgeClient::connect_if_running() {
        Ok(Some(c)) => {
            eprintln!("[好好写剧本] 已连接到运行中的 GUI，将操作 GUI 当前项目");
            return Ok(Backend::Bridge(Box::new(c)));
        }
        Ok(None) => {}
        Err(BridgeError::ProtocolMismatch { expected, actual }) => {
            eprintln!("[好好写剧本] 桥接协议不兼容（{expected} vs {actual}），进入独立模式");
        }
        Err(e) => eprintln!("[好好写剧本] 连接 GUI 失败：{}，进入独立模式", e.message()),
    }

    // 2) 独立进程模式
    let (ws, lock) = match project {
        Some(p) if p.exists() => {
            let lock = ProjectLock::acquire(p).map_err(|e| {
                anyhow::anyhow!(
                    "项目 {} 无法独占：{}\n\
                     请先关闭好好写剧本 GUI（它可能正打开着该项目）。",
                    p.display(),
                    e.message()
                )
            })?;
            (Workspace::open(p).map_err(anyhow::Error::msg)?, Some(Box::new(lock)))
        }
        _ => (Workspace::blank(title).map_err(anyhow::Error::msg)?, None),
    };
    Ok(Backend::Standalone {
        ws: Arc::new(ws),
        _lock: lock,
    })
}

fn call(backend: &mut Backend, req: BridgeRequest) -> Result<serde_json::Value> {
    let resp: BridgeResponse = match backend {
        Backend::Bridge(c) => c.call(&req).map_err(|e| anyhow::anyhow!(e.message()))?,
        Backend::Standalone { ws, _lock } => standalone(ws, _lock, req),
    };
    if resp.ok {
        Ok(resp.data.unwrap_or(serde_json::Value::Null))
    } else {
        let e = resp.error.unwrap_or(BridgeError::Internal {
            message: "未知错误".into(),
        });
        Err(anyhow::anyhow!("[{}] {}", e.code(), e.message()))
    }
}

/// 独立进程模式的本地执行
fn standalone(
    ws: &Workspace,
    lock: &mut Option<Box<ProjectLock>>,
    req: BridgeRequest,
) -> BridgeResponse {
    let rev = ws.revision();
    let ok = |r: screenplay_store::command::CommandResult| {
        BridgeResponse::ok(serde_json::to_value(&r).unwrap_or_default(), r.revision)
    };
    let fail = |e: screenplay_store::command::CommandError| {
        BridgeResponse::fail(
            match e {
                screenplay_store::command::CommandError::RevisionConflict {
                    expected,
                    actual,
                    message,
                } => BridgeError::RevisionConflict {
                    expected,
                    actual,
                    message,
                },
                screenplay_store::command::CommandError::NotFound { what, id } => {
                    BridgeError::NotFound { what, id }
                }
                screenplay_store::command::CommandError::InvalidArgument { message } => {
                    BridgeError::InvalidArgument { message }
                }
                screenplay_store::command::CommandError::BlockLocked { id } => {
                    BridgeError::BlockLocked { id }
                }
                screenplay_store::command::CommandError::Internal { message } => {
                    BridgeError::Internal { message }
                }
            },
            ws.revision(),
        )
    };
    macro_rules! touched {
        ($e:expr) => {
            match $e {
                Ok(r) => {
                    ws.touch();
                    ok(r)
                }
                Err(e) => fail(e),
            }
        };
    }

    match req {
        BridgeRequest::Ping => BridgeResponse::ok(
            serde_json::json!({"pong": true, "mode": "standalone"}),
            rev,
        ),
        BridgeRequest::ProjectInfo => {
            let d = ws.document();
            BridgeResponse::ok(
                serde_json::json!({
                    "mode": "standalone",
                    "state": if ws.container_path().is_some() { "open" } else { "unsaved" },
                    "path": ws.container_path().map(|p| p.to_string_lossy().to_string()),
                    "title": d.metadata.title,
                    "revision": d.revision,
                    "dirty": ws.dirty.load(std::sync::atomic::Ordering::SeqCst),
                    "blockCount": d.blocks.len(),
                }),
                rev,
            )
        }
        BridgeRequest::OpenProject { .. }
        | BridgeRequest::NewProject { .. }
        | BridgeRequest::CloseProject => BridgeResponse::fail(
            BridgeError::AppState {
                message: "open/new/close 需要好好写剧本 GUI 正在运行（桥接模式）".into(),
            },
            rev,
        ),
        BridgeRequest::SaveProject { path } => {
            let target: Option<PathBuf> =
                path.map(PathBuf::from).or_else(|| ws.container_path());
            match ws.save(target.as_deref()) {
                Ok(p) => {
                    if lock.is_none() {
                        if let Ok(l) = ProjectLock::acquire(&p) {
                            *lock = Some(Box::new(l));
                        }
                    }
                    BridgeResponse::ok(
                        serde_json::json!({"path": p.to_string_lossy()}),
                        ws.revision(),
                    )
                }
                Err(m) => BridgeResponse::fail(
                    BridgeError::Internal { message: m },
                    ws.revision(),
                ),
            }
        }
        BridgeRequest::GetDocument => {
            let d = ws.document();
            BridgeResponse::ok(serde_json::to_value(&d).unwrap_or_default(), d.revision)
        }
        BridgeRequest::GetSourceText => {
            let d = ws.document();
            BridgeResponse::ok(
                serde_json::json!({"text": screenplay_core::serializer::to_compact_source(&d)}),
                d.revision,
            )
        }
        BridgeRequest::Search {
            query,
            case_sensitive,
        } => {
            let h = ws.service.search(&query, case_sensitive);
            BridgeResponse::ok(serde_json::to_value(&h).unwrap_or_default(), ws.revision())
        }
        BridgeRequest::ReplaceBlock {
            block_id,
            text,
            expected_revision,
        } => touched!(ws.service.replace_block(
            Origin::Cli,
            &block_id,
            &text,
            Some(expected_revision)
        )),
        BridgeRequest::InsertBlock {
            after_id,
            block_type,
            text,
            expected_revision,
        } => {
            let bt = match parse_block_type(&block_type) {
                Ok(b) => b,
                Err(m) => {
                    return BridgeResponse::fail(
                        BridgeError::InvalidArgument { message: m.to_string() },
                        rev,
                    )
                }
            };
            touched!(ws.service.insert_block(
                Origin::Cli,
                after_id.as_deref(),
                bt,
                &text,
                Some(expected_revision)
            ))
        }
        BridgeRequest::DeleteBlock {
            block_id,
            expected_revision,
        } => touched!(ws.service.delete_block(
            Origin::Cli,
            &block_id,
            Some(expected_revision)
        )),
        BridgeRequest::SetBlockType {
            block_id,
            block_type,
            expected_revision,
        } => {
            let bt = match parse_block_type(&block_type) {
                Ok(b) => b,
                Err(m) => {
                    return BridgeResponse::fail(
                        BridgeError::InvalidArgument { message: m.to_string() },
                        rev,
                    )
                }
            };
            touched!(ws.service.set_block_type(
                Origin::Cli,
                &block_id,
                bt,
                Some(expected_revision)
            ))
        }
        BridgeRequest::MergeBlocks {
            first_id,
            second_id,
            expected_revision,
        } => touched!(ws.service.merge_blocks(
            Origin::Cli,
            &first_id,
            &second_id,
            Some(expected_revision)
        )),
        BridgeRequest::ReplaceSourceText {
            text,
            expected_revision,
        } => touched!(ws.service.replace_source_text(
            Origin::Cli,
            &text,
            Some(expected_revision)
        )),
        BridgeRequest::Undo => match ws.service.undo(Origin::Cli) {
            Ok(r) => ok(r),
            Err(e) => fail(e),
        },
        BridgeRequest::Redo => match ws.service.redo(Origin::Cli) {
            Ok(r) => ok(r),
            Err(e) => fail(e),
        },
        BridgeRequest::HistoryStatus => {
            let h = ws.service.history_status();
            BridgeResponse::ok(serde_json::to_value(&h).unwrap_or_default(), ws.revision())
        }
        BridgeRequest::ListVersions => {
            let v = ws.service.list_versions();
            BridgeResponse::ok(serde_json::to_value(&v).unwrap_or_default(), ws.revision())
        }
        BridgeRequest::CreateVersion { name, note } => {
            match ws.service.create_version(Origin::Cli, &name, &note) {
                Ok(r) => ok(r),
                Err(e) => fail(e),
            }
        }
        BridgeRequest::RestoreVersion { version_id } => {
            match ws.service.restore_version(Origin::Cli, &version_id) {
                Ok(r) => {
                    ws.touch();
                    ok(r)
                }
                Err(e) => fail(e),
            }
        }
        BridgeRequest::CompareVersions { version_a, version_b } => {
            let a = version_a.unwrap_or_default();
            let b = version_b.unwrap_or_else(|| "HEAD".into());
            match ws.service.compare_versions(&a, &b) {
                Ok(d) => {
                    BridgeResponse::ok(serde_json::to_value(&d).unwrap_or_default(), ws.revision())
                }
                Err(e) => fail(e),
            }
        }
        BridgeRequest::AuditLog { limit } => {
            let l = ws.service.audit_log(limit.unwrap_or(200) as usize);
            BridgeResponse::ok(serde_json::to_value(&l).unwrap_or_default(), ws.revision())
        }
        BridgeRequest::ListBranches => {
            let b = ws.service.list_branches();
            BridgeResponse::ok(serde_json::to_value(&b).unwrap_or_default(), ws.revision())
        }
        BridgeRequest::CurrentBranch => BridgeResponse::ok(
            serde_json::json!({"branch": ws.service.current_branch()}),
            ws.revision(),
        ),
        BridgeRequest::SwitchBranch { name } => match ws.service.switch_branch(&name) {
            Ok(()) => BridgeResponse::ok(
                serde_json::json!({"branch": ws.service.current_branch()}),
                ws.revision(),
            ),
            Err(e) => fail(e),
        },
        BridgeRequest::ImportFile { path } => {
            let p = PathBuf::from(&path);
            if !p.exists() {
                return BridgeResponse::fail(
                    BridgeError::NotFound {
                        what: "文件".into(),
                        id: path.clone(),
                    },
                    rev,
                );
            }
            match read_import(&p) {
                Ok((doc, source)) => {
                    match ws.service.load_document(Origin::Cli, doc) {
                        Ok(result) => {
                            if let Err(message) = ws.set_original_source(source) {
                                return BridgeResponse::fail(BridgeError::Internal { message }, rev);
                            }
                            ws.touch();
                            ok(result)
                        }
                        Err(error) => fail(error),
                    }
                },
                Err(m) => BridgeResponse::fail(BridgeError::Internal { message: m.to_string() }, rev),
            }
        }
        BridgeRequest::ExportFile { path, format } => {
            let raw = match format.as_deref() {
                Some(f) => f.to_string(),
                None => Path::new(&path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase(),
            };
            let fmt = match screenplay_export::ExportFormat::from_extension(&raw) {
                Some(f) => f,
                None => {
                    return BridgeResponse::fail(
                        BridgeError::InvalidArgument {
                            message: format!("未知导出格式: {raw}"),
                        },
                        rev,
                    )
                }
            };
            let d = ws.document();
            match screenplay_export::export(&d, fmt) {
                Ok(b) => match std::fs::write(&path, &b) {
                    Ok(()) => BridgeResponse::ok(
                        serde_json::json!({"path": path, "bytes": b.len()}),
                        rev,
                    ),
                    Err(e) => BridgeResponse::fail(
                        BridgeError::Internal {
                            message: format!("写入文件失败: {e}"),
                        },
                        rev,
                    ),
                },
                Err(e) => BridgeResponse::fail(
                    BridgeError::Internal {
                        message: format!("导出失败: {e}"),
                    },
                    rev,
                ),
            }
        }
        BridgeRequest::GetPagination => {
            let d = ws.document();
            let p = screenplay_core::pagination::paginate(&d);
            BridgeResponse::ok(serde_json::to_value(&p).unwrap_or_default(), d.revision)
        }
        BridgeRequest::DocumentStats => {
            let d = ws.document();
            BridgeResponse::ok(serde_json::to_value(&d.stats()).unwrap_or_default(), d.revision)
        }
        BridgeRequest::ToFountain => {
            let d = ws.document();
            BridgeResponse::ok(
                serde_json::json!({"fountain": screenplay_core::serializer::to_fountain(&d)}),
                d.revision,
            )
        }
    }
}

pub fn parse_block_type(s: &str) -> Result<BlockType> {
    serde_json::from_value(serde_json::Value::String(s.to_string()))
        .map_err(|_| anyhow::anyhow!("未知 block 类型: {s}（可用: scene_heading action character parenthetical dialogue transition shot note general）"))
}

fn read_import(path: &Path) -> Result<(screenplay_core::ast::ScreenplayDocument, Vec<u8>)> {
    screenplay_import::import_path_with_source_controlled(path, None, &screenplay_import::ImportControl::default())
        .map(|(outcome, bytes)| (outcome.document, bytes))
}

// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    let cli = Cli::parse();
    let title = "好好写剧本 CLI";
    let mut be = connect(cli.project.as_ref(), title)?;

    // 只读命令不需要项目存在；写命令由各自分支校验
    let result = run(&mut be, cli.command);
    result
}

fn run(be: &mut Backend, cmd: Cmd) -> Result<()> {
    match cmd {
        // ---------------- 状态 ----------------
        Cmd::Status => {
            println!("模式: {}", be.mode());
            let v = call(be, BridgeRequest::ProjectInfo)?;
            println!("项目: {}", v.get("title").and_then(|x| x.as_str()).unwrap_or("-"));
            println!(
                "路径: {}",
                v.get("path").and_then(|x| x.as_str()).unwrap_or("<未保存>")
            );
            println!("revision: {}", v.get("revision").and_then(|x| x.as_u64()).unwrap_or(0));
            println!("dirty: {}", v.get("dirty").and_then(|x| x.as_bool()).unwrap_or(false));
        }

        // ---------------- 生命周期 ----------------
        Cmd::New {
            title,
            save_as,
        } => {
            let t = title.unwrap_or_else(|| "未命名剧本".into());
            let v = call(be, BridgeRequest::NewProject { title: t.clone() })?;
            println!("已新建项目: {t}（{}）", be.mode());
            if let Some(p) = save_as {
                let v = call(
                    be,
                    BridgeRequest::SaveProject {
                        path: Some(p.to_string_lossy().into()),
                    },
                )?;
                println!("已保存到 {}", v.get("path").and_then(|x| x.as_str()).unwrap_or("-"));
            }
            println!(
                "当前 revision: {}",
                v.get("revision").and_then(|x| x.as_u64()).unwrap_or(0)
            );
        }

        Cmd::Open { path } => {
            let v = call(
                be,
                BridgeRequest::OpenProject {
                    path: path.to_string_lossy().into(),
                },
            )?;
            println!("已打开: {}", v.get("title").and_then(|x| x.as_str()).unwrap_or("-"));
        }

        Cmd::Save { path } => {
            let v = call(
                be,
                BridgeRequest::SaveProject {
                    path: path.map(|p| p.to_string_lossy().into()),
                },
            )?;
            println!("已保存到 {}", v.get("path").and_then(|x| x.as_str()).unwrap_or("-"));
        }

        Cmd::Close => {
            call(be, BridgeRequest::CloseProject)?;
            println!("已关闭当前项目");
        }

        // ---------------- 读取 ----------------
        Cmd::Scenes => {
            let d = call(be, BridgeRequest::GetDocument)?;
            if let Some(blocks) = d.get("blocks").and_then(|b| b.as_array()) {
                let mut n = 0;
                for b in blocks {
                    if b.get("block_type").and_then(|t| t.as_str()) == Some("scene_heading") {
                        n += 1;
                        println!(
                            "{n:>4}. {}  [{}]",
                            b.get("text").and_then(|t| t.as_str()).unwrap_or(""),
                            b.get("id").and_then(|t| t.as_str()).unwrap_or("")
                        );
                    }
                }
                println!("共 {n} 个场景");
            }
        }

        Cmd::Blocks => {
            let d = call(be, BridgeRequest::GetDocument)?;
            if let Some(blocks) = d.get("blocks").and_then(|b| b.as_array()) {
                for (i, b) in blocks.iter().enumerate() {
                    let text = b.get("text").and_then(|t| t.as_str()).unwrap_or("");
                    let preview: String = text.chars().take(60).collect();
                    println!(
                        "{:>4} [{:<14}] {} {}",
                        i + 1,
                        b.get("block_type").and_then(|t| t.as_str()).unwrap_or(""),
                        b.get("id").and_then(|t| t.as_str()).unwrap_or(""),
                        preview
                    );
                }
                println!("\n共 {} 个 block", blocks.len());
            }
        }

        Cmd::Search { query, case } => {
            let hits = call(
                be,
                BridgeRequest::Search {
                    query,
                    case_sensitive: case,
                },
            )?;
            if let Some(arr) = hits.as_array() {
                for h in arr {
                    println!(
                        "{} [{}] {}",
                        h.get("block_id").and_then(|x| x.as_str()).unwrap_or(""),
                        h.get("block_type").and_then(|x| x.as_str()).unwrap_or(""),
                        h.get("text").and_then(|x| x.as_str()).unwrap_or("")
                    );
                }
                println!("找到 {} 处", arr.len());
            }
        }

        Cmd::Info => {
            let d = call(be, BridgeRequest::GetDocument)?;
            let s = call(be, BridgeRequest::DocumentStats)?;
            println!("标题: {}", d.get("metadata").and_then(|m| m.get("title")).and_then(|t| t.as_str()).unwrap_or("-"));
            println!("revision: {}", d.get("revision").and_then(|x| x.as_u64()).unwrap_or(0));
            println!("页数: {}", s.get("pages").and_then(|x| x.as_u64()).unwrap_or(0));
            println!("场景: {}", s.get("scenes").and_then(|x| x.as_u64()).unwrap_or(0));
            println!("人物: {}", s.get("characters").and_then(|x| x.as_u64()).unwrap_or(0));
            println!("对白: {}", s.get("dialogues").and_then(|x| x.as_u64()).unwrap_or(0));
        }

        // ---------------- 撤销重做 ----------------
        Cmd::Undo => {
            let r = call(be, BridgeRequest::Undo)?;
            println!("已撤销 (rev {})", r.get("revision").and_then(|x| x.as_u64()).unwrap_or(0));
        }
        Cmd::Redo => {
            let r = call(be, BridgeRequest::Redo)?;
            println!("已重做 (rev {})", r.get("revision").and_then(|x| x.as_u64()).unwrap_or(0));
        }

        // ---------------- 写（强制 expected_revision）----------------
        Cmd::Replace {
            id,
            text,
            expect_revision,
        } => {
            let r = call(
                be,
                BridgeRequest::ReplaceBlock {
                    block_id: id,
                    text,
                    expected_revision: expect_revision,
                },
            )?;
            println!("已替换 (rev {})", r.get("revision").and_then(|x| x.as_u64()).unwrap_or(0));
        }

        Cmd::Insert {
            after,
            block_type,
            text,
            expect_revision,
        } => {
            parse_block_type(&block_type)?;
            let r = call(
                be,
                BridgeRequest::InsertBlock {
                    after_id: after,
                    block_type,
                    text,
                    expected_revision: expect_revision,
                },
            )?;
            println!(
                "已插入 {} (rev {})",
                r.get("affected")
                    .and_then(|a| a.as_array())
                    .and_then(|a| a.first())
                    .and_then(|x| x.as_str())
                    .unwrap_or(""),
                r.get("revision").and_then(|x| x.as_u64()).unwrap_or(0)
            );
        }

        Cmd::Delete { id, expect_revision } => {
            let r = call(
                be,
                BridgeRequest::DeleteBlock {
                    block_id: id,
                    expected_revision: expect_revision,
                },
            )?;
            println!("已删除 (rev {})", r.get("revision").and_then(|x| x.as_u64()).unwrap_or(0));
        }

        Cmd::SetType {
            id,
            block_type,
            expect_revision,
        } => {
            parse_block_type(&block_type)?;
            call(
                be,
                BridgeRequest::SetBlockType {
                    block_id: id,
                    block_type,
                    expected_revision: expect_revision,
                },
            )?;
            println!("已修改类型");
        }

        // ---------------- 导入导出 ----------------
        Cmd::Import { file, save_as } => {
            let r = call(
                be,
                BridgeRequest::ImportFile {
                    path: file.to_string_lossy().into(),
                },
            )?;
            println!(
                "导入成功: {} 个 block",
                r.get("document")
                    .and_then(|d| d.get("blocks"))
                    .and_then(|b| b.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0)
            );
            if let Some(p) = save_as {
                let v = call(
                    be,
                    BridgeRequest::SaveProject {
                        path: Some(p.to_string_lossy().into()),
                    },
                )?;
                println!("已保存到 {}", v.get("path").and_then(|x| x.as_str()).unwrap_or("-"));
            }
        }

        Cmd::Export { file, format } => {
            let v = call(
                be,
                BridgeRequest::ExportFile {
                    path: file.to_string_lossy().into(),
                    format,
                },
            )?;
            println!(
                "已导出到 {} ({} 字节)",
                v.get("path").and_then(|x| x.as_str()).unwrap_or("-"),
                v.get("bytes").and_then(|x| x.as_u64()).unwrap_or(0)
            );
        }

        // ---------------- 版本与审计 ----------------
        Cmd::Version { sub } => match sub {
            VersionCmd::List => {
                let v = call(be, BridgeRequest::ListVersions)?;
                if let Some(arr) = v.as_array() {
                    for x in arr {
                        println!(
                            "{}  {}  rev={}  {}{}",
                            x.get("id").and_then(|i| i.as_str()).unwrap_or("").chars().take(8).collect::<String>(),
                            x.get("name").and_then(|i| i.as_str()).unwrap_or(""),
                            x.get("revision").and_then(|i| i.as_u64()).unwrap_or(0),
                            if x.get("auto").and_then(|b| b.as_bool()).unwrap_or(false) { "[自动] " } else { "" },
                            x.get("note").and_then(|i| i.as_str()).unwrap_or(""),
                        );
                    }
                }
            }
            VersionCmd::Create { name, note } => {
                let r = call(be, BridgeRequest::CreateVersion { name, note })?;
                println!(
                    "已创建版本 {}",
                    r.get("version_id").and_then(|x| x.as_str()).unwrap_or("-")
                );
            }
            VersionCmd::Restore { id } => {
                let r = call(be, BridgeRequest::RestoreVersion { version_id: id })?;
                println!(
                    "已恢复 (rev {}, 检查点 {:?})",
                    r.get("revision").and_then(|x| x.as_u64()).unwrap_or(0),
                    r.get("version_id").and_then(|x| x.as_str())
                );
            }
            VersionCmd::Diff { a, b } => {
                let d = call(
                    be,
                    BridgeRequest::CompareVersions {
                        version_a: Some(a),
                        version_b: b,
                    },
                )?;
                println!("{}", serde_json::to_string_pretty(&d).unwrap_or_default());
            }
        },

        Cmd::Log { limit } => {
            let l = call(be, BridgeRequest::AuditLog { limit: Some(limit as u32) })?;
            if let Some(arr) = l.as_array() {
                for e in arr {
                    println!(
                        "{}  {}  {}  {} -> {}",
                        e.get("at").and_then(|x| x.as_str()).unwrap_or(""),
                        e.get("origin").and_then(|x| x.as_str()).unwrap_or(""),
                        e.get("command").and_then(|x| x.as_str()).unwrap_or(""),
                        e.get("old_value").and_then(|x| x.as_str()).unwrap_or(""),
                        e.get("new_value").and_then(|x| x.as_str()).unwrap_or(""),
                    );
                }
            }
        }
    }
    Ok(())
}
