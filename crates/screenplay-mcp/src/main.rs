//! screenplay MCP Server —— 通过 stdio 对外暴露剧本工作台能力。
//!
//! # 三条关键设计（说明书第 57 / 82 条）
//!
//! ## 1. 共享同一个权威项目
//!
//! MCP **不能**自己解压 `.screenplay` 得到独立副本。启动时：
//!
//! 1. GUI 正在运行 → 连上 GUI 的本地桥接，**所有操作转发给 GUI**：
//!    操作的就是 GUI 当前打开的项目；修改实时出现在界面；共享同一个
//!    revision 序列、同一个撤销栈；GUI 保存时一并落盘。
//! 2. GUI 未运行 → 独立进程模式：自己打开项目并持有**文件锁**，
//!    避免与另一个独立进程互相覆盖。
//!
//! ## 2. 所有写操作强制 `expected_revision`
//!
//! 写工具的 `expected_revision` 是**必填**（不是 `Option`）：缺失时 serde
//! 反序列化直接失败；过期时后端返回 `REVISION_CONFLICT`。这样 Agent 无法在
//! 不知情的情况下覆盖人的最新内容。
//!
//! **注意**：工具层绝不「帮忙」填入当前 revision —— 那样等于绕过冲突检查。
//! 调用方必须自己给出它读到的 revision。

use rmcp::{
    handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::{
        Annotated, CallToolRequestParam, CallToolResult, Content, ListResourceTemplatesResult,
        ListResourcesResult, ListToolsResult, PaginatedRequestParam, RawResource,
        RawResourceTemplate, ReadResourceRequestParam, ReadResourceResult, ResourceContents,
        ServerCapabilities, ServerInfo,
    },
    schemars, tool, tool_router, ErrorData as McpError, ServerHandler, ServiceExt,
};
use screenplay_store::bridge::{BridgeClient, BridgeError, BridgeRequest, BridgeResponse};
use screenplay_store::db::Origin;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use screenplay_store::workbench_bridge::WorkbenchRequest;
// ---------------------------------------------------------------------------
// 参数类型
// ---------------------------------------------------------------------------


#[derive(Debug,serde::Deserialize,schemars::JsonSchema)]pub struct EpisodeReadRequest{pub project_id:String,pub episode_id:String}
#[derive(Debug,serde::Deserialize,schemars::JsonSchema)]pub struct EpisodeEditRequest{pub project_id:String,pub operation:String,#[serde(default)]pub episode_id:String,#[serde(default)]pub title:String,#[serde(default)]pub order:Vec<String>,pub expected_revision:u64}
#[derive(Debug,serde::Deserialize,schemars::JsonSchema)]pub struct PlanningEditRequest{pub project_id:String,/// JSON array of {episode_id,field,value,expected_value}; empty episode_id + season_outline edits the season.
pub patches_json:String,pub expected_revision:u64}
#[derive(Debug,serde::Deserialize,schemars::JsonSchema)]pub struct ReplacementPreviewRequest{pub project_id:String,pub query:String,pub replacement:String,/// current or all
pub scope:String}
#[derive(Debug,serde::Deserialize,schemars::JsonSchema)]pub struct ReplacementApplyRequest{pub project_id:String,pub query:String,pub replacement:String,pub scope:String,/// Exact keys returned by preview_replacement; review suspicious hits explicitly.
pub selected:Vec<String>,pub expected_revision:u64}
#[derive(Debug,serde::Deserialize,schemars::JsonSchema)]pub struct DraftCompareRequest{pub project_id:String,pub version_a:String,#[serde(default)]pub version_b:Option<String>}
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EmptyRequest {}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SearchRequest {
    pub query: String,
    #[serde(default)]
    pub case_sensitive: bool,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ReplaceBlockRequest {
    pub block_id: String,
    pub text: String,
    /// 你读到的 revision。必填：缺失会被协议层拒绝，过期返回 REVISION_CONFLICT。
    pub expected_revision: u32,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct InsertBlockRequest {
    pub block_type: String,
    pub text: String,
    #[serde(default)]
    pub after_id: Option<String>,
    /// 你读到的 revision。必填。
    pub expected_revision: u32,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct DeleteBlockRequest {
    pub block_id: String,
    /// 你读到的 revision。必填。
    pub expected_revision: u32,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SetBlockTypeRequest {
    pub block_id: String,
    pub block_type: String,
    /// 你读到的 revision。必填。
    pub expected_revision: u32,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct MergeBlocksRequest {
    pub first_id: String,
    pub second_id: String,
    /// 你读到的 revision。必填。
    pub expected_revision: u32,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ReplaceSourceTextRequest {
    pub text: String,
    /// 你读到的 revision。必填。
    pub expected_revision: u32,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CreateVersionRequest {
    pub name: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RestoreVersionRequest {
    pub version_id: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CompareVersionsRequest {
    pub version_a: String,
    #[serde(default)]
    pub version_b: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct AuditLogRequest {
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SwitchBranchRequest {
    pub name: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ExportFileRequest {
    pub path: String,
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ImportFileRequest {
    pub path: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GetBlockRequest {
    pub block_id: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct OpenProjectRequest {
    /// `.screenplay` 文件路径
    pub path: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct NewProjectRequest {
    pub title: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SaveProjectRequest {
    /// 省略则保存到当前绑定路径
    #[serde(default)]
    pub path: Option<String>,
}

// ---------------------------------------------------------------------------
// 后端：桥接（GUI 运行中）或独立进程
// ---------------------------------------------------------------------------

enum Backend {
    /// 转发给正在运行的 GUI（权威状态在 GUI 进程里）
    Bridge(Box<BridgeClient>),
    /// 独立进程模式。`_lock` 持有文件锁，drop 时释放。
    Standalone {
        workspace: Arc<screenplay_store::workspace::Workspace>,
        _lock: Option<Box<screenplay_store::bridge::ProjectLock>>,
    },
}

/// 统一响应封装：把 `BridgeResponse` 转成 MCP 结果
enum Outcome {
    Ok(serde_json::Value),
    /// 结构化错误：code + message + 当前 revision
    Err {
        code: String,
        message: String,
        revision: u64,
    },
}

impl Outcome {
    fn into_result(self) -> Result<CallToolResult, McpError> {
        match self {
            Outcome::Ok(v) => Ok(CallToolResult::success(vec![Content::text(v.to_string())])),
            Outcome::Err {
                code,
                message,
                revision,
            } => {
                // 用 invalid_params 携带结构化 JSON：
                // 客户端既能拿到协议错误码，也能解析出错类别与当前 revision。
                let payload =
                    serde_json::json!({"error": code, "message": message, "revision": revision});
                Err(McpError::invalid_params(payload.to_string(), None))
            }
        }
    }
}

#[derive(Clone)]
pub struct ScreenplayMcpServer {
    backend: Arc<Mutex<Backend>>,
    tool_router: ToolRouter<Self>,
    last_revision: Arc<AtomicU64>,
}

impl ScreenplayMcpServer {
    pub fn new(project_path: Option<PathBuf>) -> Result<Self, String> {
        match BridgeClient::connect_if_running() {
            Ok(Some(client)) => {
                tracing::info!("已连接到运行中的好好写剧本 GUI，操作其当前项目");
                return Ok(Self::with_backend(Backend::Bridge(Box::new(client))));
            }
            Ok(None) => tracing::info!("好好写剧本 GUI 未运行，进入独立进程模式"),
            Err(BridgeError::ProtocolMismatch { expected, actual }) => {
                tracing::warn!("桥接协议不兼容（{expected} vs {actual}），进入独立进程模式");
            }
            Err(e) => tracing::warn!("连接 GUI 失败：{}，进入独立进程模式", e.message()),
        }

        let (ws, lock) = match project_path {
            Some(p) if p.exists() => {
                let lock = screenplay_store::bridge::ProjectLock::acquire(&p).map_err(|e| {
                    format!(
                        "项目 {} 无法独占：{}。请先关闭好好写剧本 GUI（它可能正打开着该项目），\
                         或让 MCP 在 GUI 运行时以桥接模式连接。",
                        p.to_string_lossy(),
                        e.message()
                    )
                })?;
                (screenplay_store::workspace::Workspace::open(&p)?, Some(Box::new(lock)))
            }
            other => {
                if let Some(p) = other {
                    tracing::warn!("项目 {} 不存在，创建空白项目", p.to_string_lossy());
                }
                (
                    screenplay_store::workspace::Workspace::blank("MCP Project")?,
                    None,
                )
            }
        };

        Ok(Self::with_backend(Backend::Standalone {
            workspace: Arc::new(ws),
            _lock: lock,
        }))
    }

    fn with_backend(backend: Backend) -> Self {
        Self {
            backend: Arc::new(Mutex::new(backend)),
            tool_router: Self::tool_router(),
            last_revision: Arc::new(AtomicU64::new(0)),
        }
    }

    fn mode(&self) -> &'static str {
        match &*self.backend.lock().unwrap() {
            Backend::Bridge(_) => "bridge(GUI)",
            Backend::Standalone { .. } => "standalone",
        }
    }

    /// 发送一条请求，产出统一 Outcome。
    ///
    /// `req` 由调用方构造 —— 工具层**不会**替换 `expected_revision`。
    fn call(&self, req: BridgeRequest) -> Outcome {
        let resp = {
            let mut backend = self.backend.lock().unwrap();
            match &mut *backend {
                Backend::Bridge(c) => match c.call(&req) {
                    Ok(r) => r,
                    Err(e) => {
                        return Outcome::Err {
                            code: e.code().to_string(),
                            message: e.message(),
                            revision: self.last_revision.load(Ordering::SeqCst),
                        }
                    }
                },
                Backend::Standalone { workspace, _lock } => {
                    self.exec_standalone(workspace, _lock, req)
                }
            }
        };
        self.finish(resp)
    }

    fn finish(&self, resp: BridgeResponse) -> Outcome {
        self.last_revision.store(resp.revision, Ordering::SeqCst);
        if resp.ok {
            Outcome::Ok(resp.data.unwrap_or(serde_json::Value::Null))
        } else {
            let e = resp.error.unwrap_or(BridgeError::Internal {
                message: "未知错误".into(),
            });
            Outcome::Err {
                code: e.code().to_string(),
                message: e.message(),
                revision: resp.revision,
            }
        }
    }

    /// 独立进程模式的本地实现。
    ///
    /// 注意：这里刻意**不缓存**文档，每次都从 Workspace 取权威状态，
    /// 与桥接模式语义一致。
    fn exec_standalone(
        &self,
        ws: &screenplay_store::workspace::Workspace,
        lock: &mut Option<Box<screenplay_store::bridge::ProjectLock>>,
        req: BridgeRequest,
    ) -> BridgeResponse {
        if let BridgeRequest::Workbench{request}=&req{return screenplay_store::workbench_bridge::execute(ws,request.clone(),Origin::Mcp);}
    let rev = ws.revision();
        let ok = |r: screenplay_store::command::CommandResult| {
            BridgeResponse::ok(serde_json::to_value(&r).unwrap_or(serde_json::Value::Null), r.revision)
        };
        let fail = |e: screenplay_store::command::CommandError| {
            BridgeResponse::fail(map_cmd_err(e), ws.revision())
        };
        // 写操作后标记 dirty
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
        BridgeRequest::Workbench{..}=>unreachable!("handled above"),
            // open/new/close 在独立模式下**无意义**：它们描述的是
            // 「让 GUI 切换当前项目」，独立进程没有 GUI 可驱动。
            // 直接改参数路径属于「假装成功」，必须明确报错。
            BridgeRequest::OpenProject { .. }
            | BridgeRequest::NewProject { .. }
            | BridgeRequest::CloseProject => BridgeResponse::fail(
                BridgeError::AppState {
                    message: "open/new/close_project 需要好好写剧本 GUI 正在运行（桥接模式）。\
                              当前为独立进程模式，无法操作 GUI 当前项目。"
                        .into(),
                },
                rev,
            ),
            // 独立模式可以保存**自己**的项目：这是它的持久化出口，
            // 否则独立模式写入的内容永远无法落盘。
            BridgeRequest::SaveProject { path } => {
            let target=path.map(PathBuf::from).or_else(||ws.container_path());
            let Some(target)=target else{return BridgeResponse::fail(BridgeError::InvalidArgument{message:"尚未指定保存路径".into()},rev);};
            let new_lock=if lock.is_none()||!ws.container_path().is_some_and(|p|screenplay_store::bridge::lock_path_for(&p)==screenplay_store::bridge::lock_path_for(&target)){match screenplay_store::bridge::ProjectLock::acquire(&target){Ok(l)=>Some(Box::new(l)),Err(e)=>return BridgeResponse::fail(e,rev)}}else{None};
            match ws.save(Some(&target)){Ok(p)=>{if let Some(l)=new_lock{*lock=Some(l);}ws.bind_container(&p);BridgeResponse::ok(serde_json::json!({"path":p.to_string_lossy(),"dirty":false}),ws.revision())},Err(message)=>BridgeResponse::fail(BridgeError::Internal{message},rev)}
        }
        BridgeRequest::Ping => BridgeResponse::ok(
                serde_json::json!({"pong": true, "mode": "standalone"}),
                rev,
            ),
            BridgeRequest::ProjectInfo => {
                let doc = ws.document();
                BridgeResponse::ok(
                    serde_json::json!({
                        "mode": "standalone",
                        "state": if ws.container_path().is_some() { "open" } else { "unsaved" },
                        "path": ws.container_path().map(|p| p.to_string_lossy().to_string()),
                        "title": doc.metadata.title,
                    "projectId": doc.id,
                    "activeEpisodeId": doc.series.as_ref().map(|s|&s.active_id),
                    "episodeCount": doc.series.as_ref().map(|s|s.episodes.len()).unwrap_or(1),
                        "revision": doc.revision,
                        "dirty": ws.dirty.load(Ordering::SeqCst),
                        "blockCount": doc.blocks.len(),
                    }),
                    rev,
                )
            }
            BridgeRequest::GetDocument => {
                let doc = ws.document();
                BridgeResponse::ok(serde_json::to_value(&doc).unwrap_or_default(), doc.revision)
            }
            BridgeRequest::GetSourceText => {
                let doc = ws.document();
                BridgeResponse::ok(
                    serde_json::json!({"text": screenplay_core::serializer::to_compact_source(&doc)}),
                    doc.revision,
                )
            }
            BridgeRequest::Search {
                query,
                case_sensitive,
            } => {
                let hits = ws.service.search(&query, case_sensitive);
                BridgeResponse::ok(serde_json::to_value(&hits).unwrap_or_default(), ws.revision())
            }
            BridgeRequest::ReplaceBlock {
                block_id,
                text,
                expected_revision,
            } => touched!(ws.service.replace_block(
                Origin::Mcp,
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
                            BridgeError::InvalidArgument { message: m },
                            rev,
                        )
                    }
                };
                touched!(ws.service.insert_block(
                    Origin::Mcp,
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
                Origin::Mcp,
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
                            BridgeError::InvalidArgument { message: m },
                            rev,
                        )
                    }
                };
                touched!(ws.service.set_block_type(
                    Origin::Mcp,
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
                Origin::Mcp,
                &first_id,
                &second_id,
                Some(expected_revision)
            )),
            BridgeRequest::ReplaceSourceText {
                text,
                expected_revision,
            } => touched!(ws.service.replace_source_text(
                Origin::Mcp,
                &text,
                Some(expected_revision)
            )),
            BridgeRequest::Undo => match ws.service.undo(Origin::Mcp) {
                Ok(r) => ok(r),
                Err(e) => fail(e),
            },
            BridgeRequest::Redo => match ws.service.redo(Origin::Mcp) {
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
                match ws.service.create_version(Origin::Mcp, &name, &note) {
                    Ok(r) => ok(r),
                    Err(e) => fail(e),
                }
            }
            BridgeRequest::RestoreVersion { version_id } => {
                // restore_version 内部会先 checkpoint 再恢复，并进入撤销栈
                match ws.service.restore_version(Origin::Mcp, &version_id) {
                    Ok(r) => {
                        ws.touch();
                        ok(r)
                    }
                    Err(e) => fail(e),
                }
            }
            BridgeRequest::CompareVersions { version_a, version_b } => {
                let a = version_a.unwrap_or_default();
                let b = version_b.unwrap_or_else(|| "HEAD".to_string());
                match ws.service.compare_versions(&a, &b) {
                    Ok(d) => BridgeResponse::ok(serde_json::to_value(&d).unwrap_or_default(), ws.revision()),
                    Err(e) => fail(e),
                }
            }
            BridgeRequest::AuditLog { limit } => {
                let log = ws.service.audit_log(limit.unwrap_or(200) as usize);
                BridgeResponse::ok(serde_json::to_value(&log).unwrap_or_default(), ws.revision())
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
                let (doc, source) = match read_import(&p) {
                    Ok(d) => d,
                    Err(m) => {
                        return BridgeResponse::fail(BridgeError::Internal { message: m }, rev)
                    }
                };
                match ws.service.load_document(Origin::Mcp, doc) {
                    Ok(result) => {
                        if let Err(message) = ws.set_original_source(source) {
                            return BridgeResponse::fail(BridgeError::Internal { message }, rev);
                        }
                        ws.touch();
                        ok(result)
                    }
                    Err(error) => fail(error),
                }
            }
            BridgeRequest::ExportFile { path, format } => {
                let fmt = match resolve_export_format(&path, format.as_deref()) {
                    Ok(f) => f,
                    Err(m) => {
                        return BridgeResponse::fail(
                            BridgeError::InvalidArgument { message: m },
                            rev,
                        )
                    }
                };
                let doc = ws.document();
                let bytes = match screenplay_export::export(&screenplay_core::series::publication_copy(&doc), fmt) {
                    Ok(b) => b,
                    Err(e) => {
                        return BridgeResponse::fail(
                            BridgeError::Internal {
                                message: format!("导出失败: {e}"),
                            },
                            rev,
                        )
                    }
                };
                if let Err(e) = std::fs::write(&path, &bytes) {
                    return BridgeResponse::fail(
                        BridgeError::Internal {
                            message: format!("写入文件失败: {e}"),
                        },
                        rev,
                    );
                }
                BridgeResponse::ok(
                    serde_json::json!({
                        "path": path,
                        "bytes": bytes.len(),
                        "format": format!("{:?}", fmt).to_lowercase(),
                    }),
                    rev,
                )
            }
            BridgeRequest::GetPagination => {
                let doc = ws.document();
                let p = screenplay_core::pagination::paginate(&doc);
                BridgeResponse::ok(serde_json::to_value(&p).unwrap_or_default(), doc.revision)
            }
            BridgeRequest::DocumentStats => {
                let doc = ws.document();
                BridgeResponse::ok(serde_json::to_value(&doc.stats()).unwrap_or_default(), doc.revision)
            }
            BridgeRequest::ToFountain => {
                let doc = ws.document();
                BridgeResponse::ok(
                    serde_json::json!({"fountain": screenplay_core::serializer::to_fountain(&doc)}),
                    doc.revision,
                )
            }
        }
    }
}

fn map_cmd_err(e: screenplay_store::command::CommandError) -> BridgeError {
    use screenplay_store::command::CommandError as CE;
    match e {
        CE::RevisionConflict {
            expected,
            actual,
            message,
        } => BridgeError::RevisionConflict {
            expected,
            actual,
            message,
        },
        CE::NotFound { what, id } => BridgeError::NotFound { what, id },
        CE::InvalidArgument { message } => BridgeError::InvalidArgument { message },
        CE::BlockLocked { id } => BridgeError::BlockLocked { id },
        CE::Internal { message } => BridgeError::Internal { message },
    }
}

pub fn parse_block_type(s: &str) -> Result<screenplay_core::ast::BlockType, String> {
    serde_json::from_value(serde_json::Value::String(s.to_string()))
        .map_err(|_| format!("未知 block 类型: {s}"))
}

pub fn resolve_export_format(
    path: &str,
    format: Option<&str>,
) -> Result<screenplay_export::ExportFormat, String> {
    let raw = match format {
        Some(f) => f.to_string(),
        None => Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase(),
    };
    screenplay_export::ExportFormat::from_extension(&raw).ok_or_else(|| {
        if format.is_some() {
            format!("未知导出格式: {raw}")
        } else {
            format!("无法从扩展名推断导出格式: {raw}")
        }
    })
}

/// 读取并解析外部文件（与 GUI 导入共用同一解析器）
pub fn read_import(path: &Path) -> Result<(screenplay_core::ast::ScreenplayDocument, Vec<u8>), String> {
    screenplay_import::import_path_with_source_controlled(path, None, &screenplay_import::ImportControl::default())
        .map(|(outcome, bytes)| (outcome.document, bytes))
        .map_err(|error| error.to_string())
}

// ---------------------------------------------------------------------------
// 工具
// ---------------------------------------------------------------------------

#[tool_router]
impl ScreenplayMcpServer {

    #[tool(description="Read episode directory, active episode, planning, draft label, project_id and revision. Start here before workbench writes. Existing block tools target only the active episode.")]
    async fn workbench_overview(&self,_:Parameters<EmptyRequest>)->Result<CallToolResult,McpError>{self.call(BridgeRequest::Workbench{request:WorkbenchRequest::Overview}).into_result()}
    #[tool(description="Read an episode script and board without switching the GUI. Supply project_id from overview and episode_id from the episode directory.")]
    async fn read_episode(&self,Parameters(r):Parameters<EpisodeReadRequest>)->Result<CallToolResult,McpError>{self.call(BridgeRequest::Workbench{request:WorkbenchRequest::Episode{project_id:r.project_id,episode_id:r.episode_id}}).into_result()}
    #[tool(description="Manage episodes. operation is enable/add/switch/rename/reorder/delete/disable. project_id and expected_revision required. disable requires one remaining episode. Undo shares the GUI history.")]
    async fn edit_episodes(&self,Parameters(r):Parameters<EpisodeEditRequest>)->Result<CallToolResult,McpError>{self.call(BridgeRequest::Workbench{request:WorkbenchRequest::EditEpisodes{project_id:r.project_id,operation:r.operation,episode_id:r.episode_id,title:r.title,order:r.order,expected_revision:r.expected_revision}}).into_result()}
    #[tool(description="Save season/episode planning through field-level compare-and-set. patches_json is an array of episode_id, field, value, expected_value. Episode fields are synopsis/conflict/turning_point/cliffhanger/status. Does not rewrite script text.")]
    async fn save_planning(&self,Parameters(r):Parameters<PlanningEditRequest>)->Result<CallToolResult,McpError>{let patches=serde_json::from_str(&r.patches_json).map_err(|e|McpError::invalid_params(format!("Invalid planning JSON: {e}"),None))?;self.call(BridgeRequest::Workbench{request:WorkbenchRequest::SavePlanning{project_id:r.project_id,patches,expected_revision:r.expected_revision}}).into_result()}
    #[tool(description="Preview offline literal replacements across current/all episodes, including script and card text. Returns occurrence keys, before/after excerpts, suspicious flags and revision. Review before applying; memory and story bible are excluded.")]
    async fn preview_replacement(&self,Parameters(r):Parameters<ReplacementPreviewRequest>)->Result<CallToolResult,McpError>{self.call(BridgeRequest::Workbench{request:WorkbenchRequest::PreviewReplacement{project_id:r.project_id,query:r.query,replacement:r.replacement,scope:r.scope}}).into_result()}
    #[tool(description="Apply only reviewed occurrence keys from preview_replacement using its project_id and revision. Exact same query/replacement/scope required. Conflicts are rejected; re-preview rather than guessing revision. One shared undo step.")]
    async fn apply_replacement(&self,Parameters(r):Parameters<ReplacementApplyRequest>)->Result<CallToolResult,McpError>{self.call(BridgeRequest::Workbench{request:WorkbenchRequest::ApplyReplacement{project_id:r.project_id,query:r.query,replacement:r.replacement,scope:r.scope,selected:r.selected,expected_revision:r.expected_revision}}).into_result()}
    #[tool(description="Compare named draft versions by episode identity, including script and planning changes. version_b omitted compares to current draft. Read-only, no model required.")]
    async fn compare_drafts(&self,Parameters(r):Parameters<DraftCompareRequest>)->Result<CallToolResult,McpError>{self.call(BridgeRequest::Workbench{request:WorkbenchRequest::CompareDrafts{project_id:r.project_id,version_a:r.version_a,version_b:r.version_b}}).into_result()}
    #[tool(description = "健康检查：返回当前是桥接模式（操作 GUI 正在打开的项目）还是独立模式")]
    async fn ping(&self, _: Parameters<EmptyRequest>) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::json!({
                "pong": true,
                "mode": self.mode(),
                "revision": self.last_revision.load(Ordering::SeqCst),
            })
            .to_string(),
        )]))
    }

    #[tool(description = "项目状态：标题、路径、revision、dirty。写操作前先读它取得 revision")]
    async fn project_info(&self, _: Parameters<EmptyRequest>) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::ProjectInfo).into_result()
    }

    #[tool(description = "让好好写剧本打开指定项目文件。GUI 运行时直接切换 GUI 当前项目，后续所有操作都在该项目上进行")]
    async fn open_project(
        &self,
        Parameters(req): Parameters<OpenProjectRequest>,
    ) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::OpenProject { path: req.path })
            .into_result()
    }

    #[tool(description = "新建空白项目。GUI 运行时会在 GUI 中新建，后续操作针对新项目")]
    async fn new_project(
        &self,
        Parameters(req): Parameters<NewProjectRequest>,
    ) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::NewProject { title: req.title })
            .into_result()
    }

    #[tool(description = "保存当前项目到磁盘（path 省略则保存到原路径）")]
    async fn save_project(
        &self,
        Parameters(req): Parameters<SaveProjectRequest>,
    ) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::SaveProject { path: req.path })
            .into_result()
    }

    #[tool(description = "关闭当前项目，回到欢迎页")]
    async fn close_project(&self, _: Parameters<EmptyRequest>) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::CloseProject).into_result()
    }

    #[tool(description = "获取完整剧本 AST（含全部 block、设置、场景、人物）")]
    async fn get_document(&self, _: Parameters<EmptyRequest>) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::GetDocument).into_result()
    }

    #[tool(description = "获取自由文本（右侧编辑区内容）")]
    async fn get_source_text(&self, _: Parameters<EmptyRequest>) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::GetSourceText).into_result()
    }

    #[tool(description = "按 id 获取单个 block")]
    async fn get_block(
        &self,
        Parameters(req): Parameters<GetBlockRequest>,
    ) -> Result<CallToolResult, McpError> {
        let doc = match self.call(BridgeRequest::GetDocument) {
            Outcome::Ok(d) => d,
            Outcome::Err {
                code,
                message,
                revision,
            } => {
                return Err(McpError::invalid_params(
                    serde_json::json!({"error": code, "message": message, "revision": revision})
                        .to_string(),
                    None,
                ))
            }
        };
        let blocks = doc
            .get("blocks")
            .and_then(|b| b.as_array())
            .cloned()
            .unwrap_or_default();
        match blocks
            .into_iter()
            .find(|b| b.get("id").and_then(|i| i.as_str()) == Some(req.block_id.as_str()))
        {
            Some(b) => Ok(CallToolResult::success(vec![Content::text(b.to_string())])),
            None => Err(McpError::invalid_params(
                serde_json::json!({
                    "error": "NOT_FOUND",
                    "message": format!("找不到 block: {}", req.block_id),
                })
                .to_string(),
                None,
            )),
        }
    }

    #[tool(description = "全文搜索，返回命中的 block id、类型、文本与位置")]
    async fn search(
        &self,
        Parameters(req): Parameters<SearchRequest>,
    ) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::Search {
            query: req.query,
            case_sensitive: req.case_sensitive,
        })
        .into_result()
    }

    #[tool(description = "替换 block 文本。expected_revision 必填：填你读到的 revision；过期会被拒绝，不得绕过")]
    async fn replace_block(
        &self,
        Parameters(req): Parameters<ReplaceBlockRequest>,
    ) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::ReplaceBlock {
            block_id: req.block_id,
            text: req.text,
            expected_revision: req.expected_revision as u64,
        })
        .into_result()
    }

    #[tool(description = "插入新 block。block_type 取 scene_heading/action/character/parenthetical/dialogue/transition/shot/note/general。expected_revision 必填")]
    async fn insert_block(
        &self,
        Parameters(req): Parameters<InsertBlockRequest>,
    ) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::InsertBlock {
            after_id: req.after_id,
            block_type: req.block_type,
            text: req.text,
            expected_revision: req.expected_revision as u64,
        })
        .into_result()
    }

    #[tool(description = "删除 block。expected_revision 必填")]
    async fn delete_block(
        &self,
        Parameters(req): Parameters<DeleteBlockRequest>,
    ) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::DeleteBlock {
            block_id: req.block_id,
            expected_revision: req.expected_revision as u64,
        })
        .into_result()
    }

    #[tool(description = "修改 block 元素类型，保留 block id 与文本。expected_revision 必填")]
    async fn set_block_type(
        &self,
        Parameters(req): Parameters<SetBlockTypeRequest>,
    ) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::SetBlockType {
            block_id: req.block_id,
            block_type: req.block_type,
            expected_revision: req.expected_revision as u64,
        })
        .into_result()
    }

    #[tool(description = "把 second_id 合并进 first_id（块首退格语义），保留 first 的 id。expected_revision 必填")]
    async fn merge_blocks(
        &self,
        Parameters(req): Parameters<MergeBlocksRequest>,
    ) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::MergeBlocks {
            first_id: req.first_id,
            second_id: req.second_id,
            expected_revision: req.expected_revision as u64,
        })
        .into_result()
    }

    #[tool(description = "整体替换自由文本并重新解析全文。expected_revision 必填")]
    async fn replace_source_text(
        &self,
        Parameters(req): Parameters<ReplaceSourceTextRequest>,
    ) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::ReplaceSourceText {
            text: req.text,
            expected_revision: req.expected_revision as u64,
        })
        .into_result()
    }

    #[tool(description = "撤销一步（与 GUI 共享同一撤销栈）")]
    async fn undo(&self, _: Parameters<EmptyRequest>) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::Undo).into_result()
    }

    #[tool(description = "重做一步（与 GUI 共享同一撤销栈）")]
    async fn redo(&self, _: Parameters<EmptyRequest>) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::Redo).into_result()
    }

    #[tool(description = "撤销栈状态：能否撤销/重做、栈深、动作标签")]
    async fn history_status(&self, _: Parameters<EmptyRequest>) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::HistoryStatus).into_result()
    }

    #[tool(description = "列出全部命名版本")]
    async fn list_versions(&self, _: Parameters<EmptyRequest>) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::ListVersions).into_result()
    }

    #[tool(description = "创建命名版本检查点")]
    async fn create_version(
        &self,
        Parameters(req): Parameters<CreateVersionRequest>,
    ) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::CreateVersion {
            name: req.name,
            note: req.note,
        })
        .into_result()
    }

    #[tool(description = "恢复到指定版本。会先自动 checkpoint 当前内容，且恢复本身可撤销")]
    async fn restore_version(
        &self,
        Parameters(req): Parameters<RestoreVersionRequest>,
    ) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::RestoreVersion {
            version_id: req.version_id,
        })
        .into_result()
    }

    #[tool(description = "比较两个版本的语义差异（version_b 省略时与 HEAD 比较）")]
    async fn compare_versions(
        &self,
        Parameters(req): Parameters<CompareVersionsRequest>,
    ) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::CompareVersions {
            version_a: Some(req.version_a),
            version_b: req.version_b,
        })
        .into_result()
    }

    #[tool(description = "审计日志：每条含 origin（gui-left/gui-right/mcp/cli/agent/import/system），用于追溯谁改了什么")]
    async fn audit_log(
        &self,
        Parameters(req): Parameters<AuditLogRequest>,
    ) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::AuditLog { limit: req.limit })
            .into_result()
    }

    #[tool(description = "列出所有分支")]
    async fn list_branches(&self, _: Parameters<EmptyRequest>) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::ListBranches).into_result()
    }

    #[tool(description = "当前分支名")]
    async fn current_branch(&self, _: Parameters<EmptyRequest>) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::CurrentBranch).into_result()
    }

    #[tool(description = "切换/创建分支")]
    async fn switch_branch(
        &self,
        Parameters(req): Parameters<SwitchBranchRequest>,
    ) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::SwitchBranch { name: req.name })
            .into_result()
    }

    #[tool(description = "导入外部文件替换全文（fountain/docx/fdx/txt/md/pdf/rtf…）")]
    async fn import_file(
        &self,
        Parameters(req): Parameters<ImportFileRequest>,
    ) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::ImportFile { path: req.path })
            .into_result()
    }

    #[tool(description = "导出到文件（fountain/docx/pdf/txt/md/html；格式可从扩展名推断）")]
    async fn export_file(
        &self,
        Parameters(req): Parameters<ExportFileRequest>,
    ) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::ExportFile {
            path: req.path,
            format: req.format,
        })
        .into_result()
    }

    #[tool(description = "分页结果（与 GUI 纸面渲染、PDF 导出同一份计算）")]
    async fn get_pagination(&self, _: Parameters<EmptyRequest>) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::GetPagination).into_result()
    }

    #[tool(description = "文档统计：字数、场景数、人物数、页数")]
    async fn document_stats(&self, _: Parameters<EmptyRequest>) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::DocumentStats).into_result()
    }

    #[tool(description = "导出 Fountain 纯文本")]
    async fn to_fountain(&self, _: Parameters<EmptyRequest>) -> Result<CallToolResult, McpError> {
        self.call(BridgeRequest::ToFountain).into_result()
    }
}

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

fn resource_uris() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("screenplay://document", "剧本全文", "完整 AST（含全部 block 与设置）"),
        ("screenplay://source", "自由文本", "右侧编辑区的纯文本内容"),
        ("screenplay://fountain", "Fountain", "标准 Fountain 格式"),
        ("screenplay://outline", "场景大纲", "所有场景标题与编号"),
        ("screenplay://characters", "人物表", "人物名称与出场统计"),
        ("screenplay://stats", "文档统计", "字数、场景数、页数"),
        ("screenplay://versions", "版本列表", "全部命名版本"),
        ("screenplay://audit", "审计日志", "带来源标记的操作历史"),
        ("screenplay://project", "项目信息", "标题、路径、revision、dirty"),
        ("screenplay://workbench", "剧集工作台", "分集、规划、当前集和可用接口"),
    ]
}

/// 从完整文档派生大纲 / 人物表
fn derive_resource(uri: &str, doc: &serde_json::Value) -> String {
    let blocks = doc
        .get("blocks")
        .and_then(|b| b.as_array())
        .cloned()
        .unwrap_or_default();
    match uri {
        "screenplay://outline" => {
            let mut out = String::new();
            let mut n = 0;
            for b in &blocks {
                if b.get("block_type").and_then(|v| v.as_str()) == Some("scene_heading") {
                    n += 1;
                    let text = b.get("text").and_then(|v| v.as_str()).unwrap_or("");
                    let id = b.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    out.push_str(&format!("{n}. {text}  [{id}]\n"));
                }
            }
            if out.is_empty() {
                out.push_str("（暂无场景）\n");
            }
            out
        }
        "screenplay://characters" => {
            use std::collections::BTreeMap;
            let mut counts: BTreeMap<String, usize> = BTreeMap::new();
            for b in &blocks {
                if b.get("block_type").and_then(|v| v.as_str()) == Some("character") {
                    let text = b.get("text").and_then(|v| v.as_str()).unwrap_or("").trim();
                    if !text.is_empty() {
                        *counts.entry(text.to_string()).or_insert(0) += 1;
                    }
                }
            }
            if counts.is_empty() {
                return "（暂无人物）\n".into();
            }
            counts
                .into_iter()
                .map(|(name, c)| format!("{name}\t{c} 场\n"))
                .collect()
        }
        _ => String::new(),
    }
}

// ---------------------------------------------------------------------------
// ServerHandler —— 必须显式转发，否则工具不可达
// ---------------------------------------------------------------------------

impl ScreenplayMcpServer {
    fn resource_text(&self, uri: &str) -> Result<String, Outcome> {
        let req = match uri {
            "screenplay://document" => BridgeRequest::GetDocument,
            "screenplay://source" => BridgeRequest::GetSourceText,
            "screenplay://fountain" => BridgeRequest::ToFountain,
            "screenplay://stats" => BridgeRequest::DocumentStats,
            "screenplay://versions" => BridgeRequest::ListVersions,
            "screenplay://audit" => BridgeRequest::AuditLog { limit: Some(100) },
            "screenplay://project" => BridgeRequest::ProjectInfo,
            "screenplay://workbench" => BridgeRequest::Workbench{request:WorkbenchRequest::Overview},
            "screenplay://outline" | "screenplay://characters" => {
                // 这两个由文档派生
                return match self.call(BridgeRequest::GetDocument) {
                    Outcome::Ok(doc) => Ok(derive_resource(uri, &doc)),
                    e => Err(e),
                };
            }
            other => {
                return Err(Outcome::Err {
                    code: "NOT_FOUND".into(),
                    message: format!("未知资源: {other}"),
                    revision: self.last_revision.load(Ordering::SeqCst),
                })
            }
        };
        match self.call(req) {
            Outcome::Ok(data) => {
                // 纯文本资源直接给内容，其余给 pretty JSON
                let text = match uri {
                    "screenplay://source" => data
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    "screenplay://fountain" => data
                        .get("fountain")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    _ => serde_json::to_string_pretty(&data).unwrap_or_default(),
                };
                Ok(text)
            }
            e => Err(e),
        }
    }

    fn outcome_to_mcp(self_outcome: Outcome) -> Result<CallToolResult, McpError> {
        self_outcome.into_result()
    }
}

impl ServerHandler for ScreenplayMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "好好写剧本 MCP Server。\n\
                 \n\
                 用法：\n\
                 1. 先调 project_info 或 get_document 取得 revision。\n\
                 2. 写操作必须把该 revision 作为 expected_revision 传回。\
                                 缺失会被协议层拒绝；过期返回 REVISION_CONFLICT，\
\
                                 此时应重新读取文档而不是强行覆盖。\n\
                 3. 好好写剧本 GUI 运行时本 server 自动转发到 GUI 当前项目：\
                                 修改实时出现在界面、共享撤销栈、GUI 保存时一并落盘。\n\
                 4. 也可只读 Resources：screenplay://document、//source、//outline、\
                                 //characters、//stats、//versions、//audit、//project。\n\
                 5. screenplay://block/{block_id} 可读单个块。\n\
                 6. 分集工程先调 workbench_overview，确认 project_id、active_episode_id 和 revision。read_episode 不切换 GUI；旧块工具只操作当前集。\n\
                 7. 批量替换先调 preview_replacement，核对 suspicious 与原文，再将 selected keys 和预览 revision 交给 apply_replacement。冲突后重新预览，不得只更新 revision。\n\
                 8. edit_episodes、save_planning、compare_drafts 与 GUI 共用数据及撤销机制。独立 MCP 会话修改后请调用 save_project 保存。"
                    .into(),
            ),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
            ..Default::default()
        }
    }

    /// **关键**：`#[tool_router]` 只生成构造函数，必须在此转发，
    /// 否则 `tools/list` 返回空、`tools/call` 返回 -32601。
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult {
            tools: self.tool_router.list_all(),
            next_cursor: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        self.tool_router.call(tcc).await
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: resource_uris()
                .into_iter()
                .map(|(uri, name, desc)| {
                    let mut r = RawResource::new(uri, name);
                    r.description = Some(desc.to_string());
                    r.mime_type = Some("application/json".into());
                    Annotated::new(r, None)
                })
                .collect(),
            next_cursor: None,
        })
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult {
            resource_templates: vec![Annotated::new(
                RawResourceTemplate {
                    uri_template: "screenplay://block/{block_id}".into(),
                    name: "单个 block".into(),
                    title: None,
                    description: Some("按 block id 读取单个块的完整内容".into()),
                    mime_type: Some("application/json".into()),
                },
                None,
            )],
            next_cursor: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParam,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let uri = request.uri;

        if let Some(id) = uri.strip_prefix("screenplay://block/") {
            let doc = self.outcome_to_mcp_resource("screenplay://document")?;
            let v: serde_json::Value = serde_json::from_str(&doc).unwrap_or_default();
            let found = v
                .get("blocks")
                .and_then(|b| b.as_array())
                .and_then(|arr| {
                    arr.iter()
                        .find(|b| b.get("id").and_then(|i| i.as_str()) == Some(id))
                        .cloned()
                });
            return match found {
                Some(b) => Ok(ReadResourceResult {
                    contents: vec![ResourceContents::text(b.to_string(), uri)],
                }),
                None => Err(McpError::invalid_params(
                    serde_json::json!({
                        "error": "NOT_FOUND",
                        "message": format!("找不到 block: {id}"),
                    })
                    .to_string(),
                    None,
                )),
            };
        }

        match self.resource_text(&uri) {
            Ok(text) => Ok(ReadResourceResult {
                contents: vec![ResourceContents::text(text, uri)],
            }),
            Err(Outcome::Err {
                code,
                message,
                revision,
            }) => Err(McpError::invalid_params(
                serde_json::json!({"error": code, "message": message, "revision": revision})
                    .to_string(),
                None,
            )),
            Err(Outcome::Ok(_)) => unreachable!(),
        }
    }
}

impl ScreenplayMcpServer {
    fn outcome_to_mcp_resource(&self, uri: &str) -> Result<String, McpError> {
        match self.resource_text(uri) {
            Ok(t) => Ok(t),
            Err(Outcome::Err {
                code,
                message,
                revision,
            }) => Err(McpError::invalid_params(
                serde_json::json!({"error": code, "message": message, "revision": revision})
                    .to_string(),
                None,
            )),
            Err(Outcome::Ok(_)) => unreachable!(),
        }
    }
}

// ---------------------------------------------------------------------------

fn startup_project(args:&[String])->Result<Option<PathBuf>,String>{
    match args {
        []=>Ok(None),
        [path] if !path.starts_with('-')=>Ok(Some(path.into())),
        [flag,path] if flag=="--project" || flag=="-p" || flag=="--"=>Ok(Some(path.into())),
        _=>Err("无效参数。用法：screenplay-mcp [--project <项目.screenplay>]；--help 查看说明".into()),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args=std::env::args().skip(1).collect::<Vec<_>>();
    if args.len()==1 && matches!(args[0].as_str(),"--help"|"-h") {
        println!("好好写剧本 MCP Server\nUsage: screenplay-mcp [--project <项目.screenplay>]\n\n通过 stdio 提供 MCP 工具；GUI 运行时连接当前工程。\n-p, --project  GUI 未运行时打开指定工程（也支持直接传入路径）\n-h, --help     查看说明并退出\n-V, --version  查看版本并退出");return Ok(());
    }
    if args.len()==1 && matches!(args[0].as_str(),"--version"|"-V") {println!("screenplay-mcp {}",env!("CARGO_PKG_VERSION"));return Ok(());}
    let selected=startup_project(&args)?;
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // 优先级：命令行参数 > SCREENPLAY_PROJECT 环境变量
    let project_path = selected
        .or_else(|| std::env::var("SCREENPLAY_PROJECT").ok().map(PathBuf::from));

    let server = ScreenplayMcpServer::new(project_path)?;
    let service = server.serve(rmcp::transport::io::stdio()).await?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)] mod startup_argument_regressions {
 use super::*;
 #[test] fn flags_are_not_mistaken_for_project_filenames(){
  assert!(startup_project(&["--unknown".into()]).is_err());
  assert!(startup_project(&["--project".into()]).is_err());
  assert_eq!(startup_project(&["--project".into(),"project.screenplay".into()]).unwrap(),Some(PathBuf::from("project.screenplay")));
  assert_eq!(startup_project(&["project.screenplay".into()]).unwrap(),Some(PathBuf::from("project.screenplay")));
  assert_eq!(startup_project(&[]).unwrap(),None);
 }
}

#[cfg(test)]mod workbench_schema_tests{use super::*;
 #[test]fn feature_tools_are_discoverable_and_writes_require_identity_and_revision(){let tools=ScreenplayMcpServer::tool_router().list_all();let data=serde_json::to_value(tools).unwrap();let tools=data.as_array().unwrap();for name in ["workbench_overview","read_episode","edit_episodes","save_planning","preview_replacement","apply_replacement","compare_drafts"]{assert!(tools.iter().any(|t|t["name"]==name),"missing {name}");}for name in ["edit_episodes","save_planning","apply_replacement"]{let tool=tools.iter().find(|t|t["name"]==name).unwrap();let required=tool["inputSchema"]["required"].as_array().unwrap();assert!(required.iter().any(|v|v=="project_id"));assert!(required.iter().any(|v|v=="expected_revision"));}}
}
