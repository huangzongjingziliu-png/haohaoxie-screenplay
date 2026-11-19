//! 专家（Expert）会审：多位带方法论的审稿人依次评审，再由“会审主持”汇总。
//!
//! 设计要点见 `docs/assistant/harness-experts.md`：
//! - 专家与技能分工：技能是片段级写作动作，专家是稳定的方法论人格；
//! - 串行执行，调用次数 = 专家数 + 1，避免并发放大成本；
//! - 自定义专家放在用户目录的 `experts/<id>/EXPERT.md`，不覆盖内置专家。

use crate::{build_agent, AppState, IpcError, IpcResult};
use screenplay_agent::{Expert, ExpertMode, ExpertPanel};
use screenplay_core::ast::ScreenplayDocument;
use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

fn error(message: impl ToString) -> IpcError {
    IpcError::InvalidArgument { message: message.to_string() }
}

/// 用户专家目录（与内置技能目录并列）。
pub(crate) fn experts_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("ScreenplayStudio").join("experts"))
}

pub(crate) fn load_all_experts() -> Vec<Expert> {
    screenplay_agent::load_experts_detailed(experts_dir().as_deref()).0
}

#[derive(Serialize)]
pub struct ExpertInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub dimensions: Vec<String>,
    pub builtin: bool,
}

/// 专家列表 + 自定义文件被跳过时的原因（路径与错误），供界面提示用户修正。
#[derive(Serialize)]
pub struct ExpertCatalog {
    pub experts: Vec<ExpertInfo>,
    pub issues: Vec<String>,
}

#[tauri::command]
pub fn agent_list_experts() -> IpcResult<ExpertCatalog> {
    let (experts, issues) = screenplay_agent::load_experts_detailed(experts_dir().as_deref());
    Ok(ExpertCatalog {
        experts: experts
            .into_iter()
            .map(|e| ExpertInfo {
                id: e.id,
                name: e.name,
                description: e.description,
                dimensions: e.dimensions,
                builtin: e.builtin,
            })
            .collect(),
        issues,
    })
}

/// 当前场景附近的正文（与对话上下文保持同一取法）。
pub(crate) fn scene_excerpt(doc: &ScreenplayDocument, block_id: Option<&str>) -> String {
    let index = doc
        .blocks
        .iter()
        .position(|b| block_id == Some(b.id.as_str()))
        .unwrap_or(0);
    let start = doc.blocks[..index.saturating_add(1).min(doc.blocks.len())]
        .iter()
        .rposition(|b| b.block_type == screenplay_core::ast::BlockType::SceneHeading)
        .unwrap_or(0);
    doc.blocks
        .iter()
        .skip(start)
        .take(100)
        .map(|b| format!("[{}] {}", b.block_type.as_str(), b.text))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn scene_catalog(doc: &ScreenplayDocument) -> String {
    doc.scenes
        .iter()
        .map(|s| {
            format!(
                "{} · {:?} {} · {}",
                s.number.as_deref().unwrap_or(""),
                s.int_ext,
                s.location,
                s.time_of_day
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 整稿正文，超过上限时截断并明确标注，避免模型误以为稿子只有这么长。
fn whole_script(doc: &ScreenplayDocument) -> String {
    const LIMIT: usize = 30_000;
    let mut text = doc
        .blocks
        .iter()
        .map(|b| format!("[{}] {}", b.block_type.as_str(), b.text))
        .collect::<Vec<_>>()
        .join("\n");
    if text.chars().count() > LIMIT {
        text = text.chars().take(LIMIT).collect::<String>();
        text.push_str("\n\n（正文超出单次审阅上限，以下内容未提供：请改为按场景分别会审。）");
    }
    text
}

#[tauri::command]
pub async fn agent_expert_panel(
    state: State<'_, AppState>,
    project_id: String,
    expert_ids: Vec<String>,
    mode: String,
    scope: String,
    block_id: Option<String>,
    instruction: Option<String>,
) -> IpcResult<ExpertPanel> {
    crate::agent_text::ready(&state).await?;
    let doc = state.project.document()?;
    if doc.id != project_id {
        return Err(error("项目已切换，请重新发起会审"));
    }
    let mut ids = Vec::new();
    for id in expert_ids {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    if ids.is_empty() {
        return Err(error("请至少选择一位专家"));
    }
    if ids.len() > 4 {
        return Err(error("一次最多选择四位专家，避免耗时和费用过大"));
    }
    let all = load_all_experts();
    let mut picked = Vec::new();
    for id in &ids {
        let expert = all
            .iter()
            .find(|e| &e.id == id)
            .ok_or_else(|| error(format!("专家不存在：{id}")))?;
        picked.push(expert.clone());
    }

    let system_memory = std::fs::read_to_string(
        dirs::data_dir()
            .unwrap_or_default()
            .join("ScreenplayStudio")
            .join("memories")
            .join("system.md"),
    )
    .unwrap_or_default();
    let notes = if doc.settings.project_memory.author_notes.is_empty()
        && doc.settings.project_memory.source_fingerprint.is_empty()
    {
        std::fs::read_to_string(
            dirs::data_dir()
                .unwrap_or_default()
                .join("ScreenplayStudio")
                .join("memories")
                .join("projects")
                .join(format!("{project_id}.md")),
        )
        .unwrap_or_default()
    } else {
        doc.settings.project_memory.author_notes.clone()
    };
    let memory = format!(
        "系统偏好记忆：{}\n作者确认与锁定笔记：{}\n自动整理：{}",
        if system_memory.trim().is_empty() { "（无）" } else { &system_memory },
        if notes.trim().is_empty() { "（无）" } else { &notes },
        if doc.settings.project_memory.markdown.trim().is_empty() {
            "（无）"
        } else {
            &doc.settings.project_memory.markdown
        }
    );
    let context = if scope == "whole" {
        format!("场景目录：\n{}\n\n正文：\n{}", scene_catalog(&doc), whole_script(&doc))
    } else {
        format!(
            "场景目录：\n{}\n\n当前场景附近正文：\n{}",
            scene_catalog(&doc),
            scene_excerpt(&doc, block_id.as_deref())
        )
    };
    let agent = build_agent(&state)?;
    Ok(screenplay_agent::run_panel(
        &agent.router,
        &picked,
        &context,
        &memory,
        ExpertMode::parse(&mode),
        instruction.as_deref(),
    )
    .await)
}
