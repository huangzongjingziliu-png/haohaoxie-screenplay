//! 桥接请求处理：把 `BridgeRequest` 映射到权威 `ProjectService`。
//!
//! 这是 CLI/MCP 写入进入 GUI 的**唯一入口**。所有请求都：
//! 1. 走同一条 `CommandService` 路径（不绕过）
//! 2. 强制 `expectedRevision`（写操作）
//! 3. 返回结构化错误
//! 4. 在 Tauri 主线程执行，从而能安全广播 `document-changed`
//!
//! 注意：本文件**不做**任何协议解析以外的业务判断，也不缓存文档。

use screenplay_core::ast::BlockType;
use screenplay_store::bridge::{BridgeError, BridgeRequest, BridgeResponse};
use screenplay_store::command::CommandError;
use screenplay_store::db::Origin;
use tauri::AppHandle;

use crate::service::ProjectService;

/// `CommandError` → `BridgeError`
pub fn map_err(e: CommandError) -> BridgeError {
    match e {
        CommandError::RevisionConflict {
            expected,
            actual,
            message,
        } => BridgeError::RevisionConflict {
            expected,
            actual,
            message,
        },
        CommandError::NotFound { what, id } => BridgeError::NotFound { what, id },
        CommandError::InvalidArgument { message } => BridgeError::InvalidArgument { message },
        CommandError::BlockLocked { id } => BridgeError::BlockLocked { id },
        CommandError::Internal { message } => BridgeError::Internal { message },
    }
}

/// 解析 block 类型（与 Tauri 命令共用同一套合法值）
pub fn parse_block_type(s: &str) -> Result<BlockType, BridgeError> {
    serde_json::from_value(serde_json::Value::String(s.to_string())).map_err(|_| {
        BridgeError::InvalidArgument {
            message: format!("未知 block 类型: {s}"),
        }
    })
}

/// 解析导出格式（与 Tauri `export_file` 命令共用同一套合法值）
pub fn resolve_export_format(
    path: &str,
    format: Option<&str>,
) -> Result<screenplay_export::ExportFormat, BridgeError> {
    use screenplay_export::ExportFormat;
    let raw = match format {
        Some(f) => f.to_string(),
        None => std::path::PathBuf::from(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase(),
    };
    ExportFormat::from_extension(&raw).ok_or_else(|| BridgeError::InvalidArgument {
        message: if format.is_some() {
            format!("未知导出格式: {raw}")
        } else {
            format!("无法从扩展名推断导出格式: {raw}")
        },
    })
}

/// 外部写入的 Origin：统一标记来源，便于审计追溯
pub const EXTERNAL_ORIGIN: Origin = Origin::Mcp;

/// 处理项目生命周期请求。返回 `None` 表示「不是生命周期请求」。
///
/// 生命周期请求必须在 `require()` **之前**处理：它们正是为了
/// 「当前没有项目」的情况准备的（open / new / close）。
fn handle_lifecycle(
    app: &AppHandle,
    project: &ProjectService,
    req: &BridgeRequest,
) -> Option<BridgeResponse> {
    let rev = project.revision();
    let respond_info = |project: &ProjectService| {
        let info = crate::project_info(project);
        crate::emit_project_changed(app, &info);
        BridgeResponse::ok(
            serde_json::to_value(&info).unwrap_or_default(),
            project.revision(),
        )
    };

    match req {
        BridgeRequest::OpenProject { path } => {
            let p = std::path::PathBuf::from(path);
            if !p.exists() {
                return Some(BridgeResponse::fail(
                    BridgeError::NotFound {
                        what: "项目文件".into(),
                        id: path.clone(),
                    },
                    rev,
                ));
            }
            Some(match project.open(&p) {
                Ok(()) => {
                    crate::refresh_bridge_descriptor(app, project);
                    respond_info(project)
                }
                Err(m) => BridgeResponse::fail(
                    BridgeError::AppState { message: m },
                    project.revision(),
                ),
            })
        }
        BridgeRequest::NewProject { title } => Some(match project.create_new(title) {
            Ok(()) => {
                crate::refresh_bridge_descriptor(app, project);
                respond_info(project)
            }
            Err(m) => {
                BridgeResponse::fail(BridgeError::AppState { message: m }, project.revision())
            }
        }),
        BridgeRequest::SaveProject { path } => {
            let target = path.as_deref().map(std::path::PathBuf::from);
            Some(match project.save(target.as_deref()) {
                Ok(outcome) => {
                    crate::refresh_bridge_descriptor(app, project);
                    BridgeResponse::ok(
                        serde_json::json!({
                            "path": outcome.path,
                            "savedRevision": outcome.saved_revision,
                            "savedAt": outcome.saved_at,
                            "dirty": project.is_dirty(),
                        }),
                        project.revision(),
                    )
                }
                Err(m) => {
                    BridgeResponse::fail(BridgeError::AppState { message: m }, project.revision())
                }
            })
        }
        BridgeRequest::CloseProject => Some(match project.close() {
            Ok(()) => {
                crate::refresh_bridge_descriptor(app, project);
                respond_info(project)
            }
            Err(m) => BridgeResponse::fail(BridgeError::AppState { message: m }, project.revision()),
        }),
        _ => None,
    }
}

/// 处理一条桥接请求。
///
/// `app` 用于广播变更，让 GUI 前端实时刷新（说明书 C-08）。
pub fn handle(app: &AppHandle, project: &ProjectService, req: BridgeRequest) -> BridgeResponse {
    let rev = project.revision();

    // ---------------- 项目生命周期 ----------------
    // 注意：这些**不能**放在 require() 之后 —— 它们正是为
    // 「当前没有项目」的情况准备的（open / new / close）。
    if let Some(resp) = handle_lifecycle(app, project, &req) {
        return resp;
    }

    match req {
        BridgeRequest::Ping => {
            return BridgeResponse::ok(
                serde_json::json!({
                    "pong": true,
                    "container": project.state().path().map(|p| p.to_string_lossy().to_string()),
                    "title": project.state().title(),
                }),
                rev,
            )
        }
        BridgeRequest::ProjectInfo => return project_info(project),
        _ => {}
    }

    let ws = match project.require() {
        Ok(w) => w,
        Err(m) => return BridgeResponse::fail(BridgeError::AppState { message: m }, rev),
    };

    if let BridgeRequest::Workbench{request}=&req{let write=matches!(request,screenplay_store::workbench_bridge::WorkbenchRequest::EditEpisodes{..}|screenplay_store::workbench_bridge::WorkbenchRequest::SavePlanning{..}|screenplay_store::workbench_bridge::WorkbenchRequest::ApplyReplacement{..});let response=screenplay_store::workbench_bridge::execute(&ws,request.clone(),EXTERNAL_ORIGIN);if write&&response.ok{if let Some(data)=response.data.clone(){if let Ok(r)=serde_json::from_value::<screenplay_store::command::CommandResult>(data){crate::broadcast_external(app,&r);}}}return response;}
    match req {
        BridgeRequest::Workbench{..}=>unreachable!("handled above"),
        BridgeRequest::Ping
        | BridgeRequest::ProjectInfo
        | BridgeRequest::OpenProject { .. }
        | BridgeRequest::NewProject { .. }
        | BridgeRequest::SaveProject { .. }
        | BridgeRequest::CloseProject => unreachable!("已在上方处理"),

        BridgeRequest::GetDocument => {
            let doc = ws.document();
            BridgeResponse::ok(serde_json::to_value(&doc).unwrap_or(serde_json::Value::Null), doc.revision)
        }

        BridgeRequest::GetSourceText => {
            let doc = ws.document();
            let text = screenplay_core::serializer::to_compact_source(&doc);
            BridgeResponse::ok(serde_json::json!({ "text": text }), doc.revision)
        }

        BridgeRequest::Search {
            query,
            case_sensitive,
        } => {
            let hits = ws.service.search(&query, case_sensitive);
            BridgeResponse::ok(
                serde_json::to_value(&hits).unwrap_or(serde_json::Value::Null),
                ws.revision(),
            )
        }

        // ---------------- 修改（强制 expected_revision）----------------
        BridgeRequest::ReplaceBlock {
            block_id,
            text,
            expected_revision,
        } => run_and_broadcast(
            app,
            project,
            move |svc| {
                svc.replace_block(
                    EXTERNAL_ORIGIN,
                    &block_id,
                    &text,
                    Some(expected_revision),
                )
            },
        ),

        BridgeRequest::InsertBlock {
            after_id,
            block_type,
            text,
            expected_revision,
        } => {
            let bt = match parse_block_type(&block_type) {
                Ok(b) => b,
                Err(e) => return BridgeResponse::fail(e, rev),
            };
            run_and_broadcast(
                app,
                project,
                move |svc| {
                    svc.insert_block(
                        EXTERNAL_ORIGIN,
                        after_id.as_deref(),
                        bt,
                        &text,
                        Some(expected_revision),
                    )
                },
            )
        }

        BridgeRequest::DeleteBlock {
            block_id,
            expected_revision,
        } => run_and_broadcast(
            app,
            project,
            move |svc| svc.delete_block(EXTERNAL_ORIGIN, &block_id, Some(expected_revision)),
        ),

        BridgeRequest::SetBlockType {
            block_id,
            block_type,
            expected_revision,
        } => {
            let bt = match parse_block_type(&block_type) {
                Ok(b) => b,
                Err(e) => return BridgeResponse::fail(e, rev),
            };
            run_and_broadcast(
                app,
                project,
                move |svc| {
                    svc.set_block_type(EXTERNAL_ORIGIN, &block_id, bt, Some(expected_revision))
                },
            )
        }

        BridgeRequest::MergeBlocks {
            first_id,
            second_id,
            expected_revision,
        } => run_and_broadcast(
            app,
            project,
            move |svc| {
                svc.merge_blocks(
                    EXTERNAL_ORIGIN,
                    &first_id,
                    &second_id,
                    Some(expected_revision),
                )
            },
        ),

        BridgeRequest::ReplaceSourceText {
            text,
            expected_revision,
        } => run_and_broadcast(
            app,
            project,
            move |svc| {
                svc.replace_source_text(EXTERNAL_ORIGIN, &text, Some(expected_revision))
            },
        ),

        // ---------------- 撤销重做 ----------------
        BridgeRequest::Undo => run_and_broadcast(app, project, move |svc| svc.undo(EXTERNAL_ORIGIN)),
        BridgeRequest::Redo => run_and_broadcast(app, project, move |svc| svc.redo(EXTERNAL_ORIGIN)),
        BridgeRequest::HistoryStatus => {
            let h = ws.service.history_status();
            BridgeResponse::ok(
                serde_json::to_value(&h).unwrap_or(serde_json::Value::Null),
                ws.revision(),
            )
        }

        // ---------------- 版本 ----------------
        BridgeRequest::ListVersions => {
            let v = ws.service.list_versions();
            BridgeResponse::ok(
                serde_json::to_value(&v).unwrap_or(serde_json::Value::Null),
                ws.revision(),
            )
        }

        BridgeRequest::CreateVersion { name, note } => run_and_broadcast(
            app,
            project,
            move |svc| svc.create_version(EXTERNAL_ORIGIN, &name, &note),
        ),

        BridgeRequest::RestoreVersion { version_id } => run_and_broadcast(
            app,
            project,
            move |svc| svc.restore_version(EXTERNAL_ORIGIN, &version_id),
        ),

        BridgeRequest::CompareVersions { version_a, version_b } => {
            let a = match version_a {
                Some(v) => v,
                None => return BridgeResponse::fail(
                    BridgeError::InvalidArgument {
                        message: "compare_versions 需要 version_a".into(),
                    },
                    ws.revision(),
                ),
            };
            let b = version_b.unwrap_or_else(|| "HEAD".to_string());
            match ws.service.compare_versions(&a, &b) {
                Ok(d) => BridgeResponse::ok(
                    serde_json::to_value(&d).unwrap_or(serde_json::Value::Null),
                    ws.revision(),
                ),
                Err(e) => BridgeResponse::fail(map_err(e), ws.revision()),
            }
        }

        BridgeRequest::AuditLog { limit } => {
            let log = ws.service.audit_log(limit.unwrap_or(200) as usize);
            BridgeResponse::ok(
                serde_json::to_value(&log).unwrap_or(serde_json::Value::Null),
                ws.revision(),
            )
        }

        // ---------------- 分支 ----------------
        BridgeRequest::ListBranches => {
            let b = ws.service.list_branches();
            BridgeResponse::ok(
                serde_json::to_value(&b).unwrap_or(serde_json::Value::Null),
                ws.revision(),
            )
        }
        BridgeRequest::CurrentBranch => BridgeResponse::ok(
            serde_json::json!({ "branch": ws.service.current_branch() }),
            ws.revision(),
        ),
        BridgeRequest::SwitchBranch { name } => match ws.service.switch_branch(&name) {
            Ok(()) => BridgeResponse::ok(
                serde_json::json!({ "branch": ws.service.current_branch() }),
                ws.revision(),
            ),
            Err(e) => BridgeResponse::fail(map_err(e), ws.revision()),
        },

        // ---------------- 导入导出 ----------------
        BridgeRequest::ImportFile { path } => {
            let p = std::path::PathBuf::from(&path);
            if !p.exists() {
                return BridgeResponse::fail(
                    BridgeError::NotFound {
                        what: "文件".into(),
                        id: path.clone(),
                    },
                    ws.revision(),
                );
            }
            let (outcome, original_source) = match crate::read_import_with_source(&p) {
                Ok(o) => o,
                Err(e) => return BridgeResponse::fail(e, ws.revision()),
            };
            // 外部导入标为 MCP 来源，审计可追溯
            match project.run_with_original_source(|svc| svc.load_document(EXTERNAL_ORIGIN, outcome), original_source) {
                Ok(r) => {
                    crate::broadcast_external(app, &r);
                    BridgeResponse::ok(
                        serde_json::to_value(&r).unwrap_or(serde_json::Value::Null),
                        r.revision,
                    )
                }
                Err(e) => BridgeResponse::fail(map_err(e), ws.revision()),
            }
        }

        BridgeRequest::ExportFile { path, format } => {
            let fmt = match resolve_export_format(&path, format.as_deref()) {
                Ok(f) => f,
                Err(e) => return BridgeResponse::fail(e, ws.revision()),
            };
            let doc = ws.document();
            let bytes = match screenplay_export::export(&screenplay_core::series::publication_copy(&doc), fmt) {
                Ok(b) => b,
                Err(e) => {
                    return BridgeResponse::fail(
                        BridgeError::Internal {
                            message: format!("导出失败: {e}"),
                        },
                        ws.revision(),
                    )
                }
            };
            let target = std::path::PathBuf::from(&path);
            if let Err(e) = std::fs::write(&target, &bytes) {
                return BridgeResponse::fail(
                    BridgeError::Internal {
                        message: format!("写入文件失败: {e}"),
                    },
                    ws.revision(),
                );
            }
            BridgeResponse::ok(
                serde_json::json!({
                    "path": target.to_string_lossy(),
                    "bytes": bytes.len(),
                    "format": format!("{:?}", fmt).to_lowercase(),
                }),
                ws.revision(),
            )
        }

        // ---------------- 分页与序列化 ----------------
        BridgeRequest::GetPagination => {
            let doc = ws.document();
            let p = screenplay_core::pagination::paginate(&doc);
            BridgeResponse::ok(serde_json::to_value(&p).unwrap_or(serde_json::Value::Null), doc.revision)
        }

        BridgeRequest::DocumentStats => {
            let doc = ws.document();
            let s = doc.stats();
            BridgeResponse::ok(serde_json::to_value(&s).unwrap_or(serde_json::Value::Null), doc.revision)
        }

        BridgeRequest::ToFountain => {
            let doc = ws.document();
            let f = screenplay_core::serializer::to_fountain(&doc);
            BridgeResponse::ok(serde_json::json!({ "fountain": f }), doc.revision)
        }
    }
}

// ---------------------------------------------------------------------------

fn project_info(project: &ProjectService) -> BridgeResponse {
    let st = project.state();
    let document=project.document().ok();
    let doc_count = document.as_ref().map(|d| d.blocks.len()).unwrap_or(0);
    BridgeResponse::ok(
        serde_json::json!({
            "state": match st {
                crate::service::ProjectState::Empty => "empty",
                crate::service::ProjectState::Unsaved { .. } => "unsaved",
                crate::service::ProjectState::Open { .. } => "open",
            },
            "path": st.path().map(|p| p.to_string_lossy().to_string()),
            "title": st.title(),
            "projectId": document.as_ref().map(|d|&d.id),
            "activeEpisodeId": document.as_ref().and_then(|d|d.series.as_ref().map(|s|&s.active_id)),
            "episodeCount": document.as_ref().map(|d|d.series.as_ref().map(|s|s.episodes.len()).unwrap_or(1)),
            "revision": project.revision(),
            "dirty": project.is_dirty(),
            "blockCount": doc_count,
        }),
        project.revision(),
    )
}

/// 执行修改并广播变更
fn run_and_broadcast<F>(
    app: &AppHandle,
    project: &ProjectService,
    f: F,
) -> BridgeResponse
where
    F: FnOnce(&screenplay_store::command::CommandService) -> Result<
        screenplay_store::command::CommandResult,
        CommandError,
    >,
{
    match project.run(f) {
        Ok(r) => {
            // 广播给 GUI 前端：外部写入必须实时可见（说明书 C-08）
            crate::broadcast_external(app, &r);
            BridgeResponse::ok(
                serde_json::to_value(&r).unwrap_or(serde_json::Value::Null),
                r.revision,
            )
        }
        Err(e) => {
            let rev = project.revision();
            BridgeResponse::fail(map_err(e), rev)
        }
    }
}
