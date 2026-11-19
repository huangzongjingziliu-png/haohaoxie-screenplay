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
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;

fn error(message: impl ToString) -> IpcError {
    IpcError::InvalidArgument { message: message.to_string() }
}

/// 用户专家目录（与内置技能目录并列）。
pub(crate) fn experts_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("ScreenplayStudio").join("experts"))
}

/// 停用名单单独存放，不改 EXPERT.md 格式，也不改既有解析规则。
fn disabled_file() -> Option<PathBuf> {
    experts_dir().map(|d| d.join(".disabled.json"))
}

fn read_disabled() -> Vec<String> {
    disabled_file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<Vec<String>>(&text).ok())
        .unwrap_or_default()
}

fn write_disabled(ids: &[String]) -> IpcResult<()> {
    let path = disabled_file().ok_or_else(|| error("无法确定用户数据目录"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| IpcError::Internal { message: e.to_string() })?;
    }
    let text = serde_json::to_string_pretty(ids).map_err(|e| IpcError::Internal { message: e.to_string() })?;
    std::fs::write(path, text).map_err(|e| IpcError::Internal { message: e.to_string() })
}

pub(crate) fn load_all_experts() -> Vec<Expert> {
    let disabled = read_disabled();
    screenplay_agent::load_experts_detailed(experts_dir().as_deref())
        .0
        .into_iter()
        .filter(|e| !disabled.contains(&e.id))
        .collect()
}

#[derive(Serialize)]
pub struct ExpertInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub dimensions: Vec<String>,
    pub builtin: bool,
    pub enabled: bool,
}

fn expert_info(expert: &Expert, enabled: bool) -> ExpertInfo {
    ExpertInfo {
        id: expert.id.clone(),
        name: expert.name.clone(),
        description: expert.description.clone(),
        dimensions: expert.dimensions.clone(),
        builtin: expert.builtin,
        enabled,
    }
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
    let disabled = read_disabled();
    Ok(ExpertCatalog {
        experts: experts
            .iter()
            .map(|e| expert_info(e, !disabled.contains(&e.id)))
            .collect(),
        issues,
    })
}

/// 启用/停用专家。状态存本地，不改动 EXPERT.md。
#[tauri::command]
pub fn agent_set_expert_enabled(expert_id: String, enabled: bool) -> IpcResult<()> {
    let (all, _) = screenplay_agent::load_experts_detailed(experts_dir().as_deref());
    if !all.iter().any(|e| e.id == expert_id) {
        return Err(error(format!("专家不存在：{expert_id}")));
    }
    let mut disabled = read_disabled();
    disabled.retain(|id| id != &expert_id);
    if !enabled {
        disabled.push(expert_id);
    }
    write_disabled(&disabled)
}

/// 在 frontmatter 中写入或替换 name 字段，保留其余内容。
fn set_frontmatter_name(content: &str, name: &str) -> String {
    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
    if let Some(start) = lines.iter().position(|l| l.trim() == "---") {
        if let Some(offset) = lines[start + 1..].iter().position(|l| l.trim() == "---") {
            let end = start + 1 + offset;
            let mut replaced = false;
            for line in lines.iter_mut().take(end).skip(start + 1) {
                if line.trim_start().starts_with("name:") {
                    *line = format!("name: {name}");
                    replaced = true;
                }
            }
            if !replaced {
                lines.insert(end, format!("name: {name}"));
            }
            let mut text = lines.join("\n");
            if content.ends_with('\n') {
                text.push('\n');
            }
            return text;
        }
    }
    format!("---\nname: {name}\n---\n{content}")
}

#[tauri::command]
pub fn agent_rename_expert(expert_id: String, name: String) -> IpcResult<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(error("专家名称不能为空"));
    }
    if name.chars().count() > 60 {
        return Err(error("专家名称最多 60 个字"));
    }
    let dir = experts_dir().ok_or_else(|| error("无法确定用户数据目录"))?;
    let file = dir.join(&expert_id).join("EXPERT.md");
    if !file.is_file() {
        return Err(error("内置专家不支持改名；请改用自定义专家"));
    }
    let content = std::fs::read_to_string(&file).map_err(|e| IpcError::Internal { message: e.to_string() })?;
    let updated = set_frontmatter_name(&content, name);
    std::fs::write(&file, updated).map_err(|e| IpcError::Internal { message: e.to_string() })
}

/// 打开专家目录；传入 expert_id 时打开该自定义专家的文件夹。
#[tauri::command]
pub fn agent_open_experts_dir(expert_id: Option<String>) -> IpcResult<()> {
    let root = experts_dir().ok_or_else(|| error("无法确定用户数据目录"))?;
    std::fs::create_dir_all(&root).map_err(|e| IpcError::Internal { message: e.to_string() })?;
    let target = match expert_id {
        Some(id) => {
            let dir = root.join(&id);
            if !dir.is_dir() {
                return Err(error("该专家没有本地文件夹"));
            }
            dir
        }
        None => root,
    };
    std::process::Command::new("open")
        .arg(&target)
        .spawn()
        .map_err(|e| IpcError::Internal { message: e.to_string() })?;
    Ok(())
}

/// 导入单个专家：源可以是 EXPERT.md 文件，也可以是包含 EXPERT.md 的文件夹。
#[tauri::command]
pub fn agent_import_expert(source_path: String) -> IpcResult<ExpertInfo> {
    let src = PathBuf::from(&source_path);
    let file = if src.is_dir() { src.join("EXPERT.md") } else { src };
    if !file.is_file() {
        return Err(error("请选择 EXPERT.md 文件或包含它的文件夹"));
    }
    let content = std::fs::read_to_string(&file).map_err(|e| IpcError::Internal { message: e.to_string() })?;
    let expert = screenplay_agent::parse_expert_md(&content).map_err(|e| error(format!("解析失败：{e}")))?;
    if expert.builtin || screenplay_agent::builtin_experts().iter().any(|b| b.id == expert.id) {
        return Err(error(format!("id「{}」与内置专家重复，请修改后再导入", expert.id)));
    }
    let root = experts_dir().ok_or_else(|| error("无法确定用户数据目录"))?;
    std::fs::create_dir_all(&root).map_err(|e| IpcError::Internal { message: e.to_string() })?;
    let dest_dir = root.join(&expert.id);
    if dest_dir.exists() {
        return Err(error(format!("已存在同名专家「{}」，请先移除或改名", expert.id)));
    }
    std::fs::create_dir_all(&dest_dir).map_err(|e| IpcError::Internal { message: e.to_string() })?;
    std::fs::write(dest_dir.join("EXPERT.md"), content)
        .map_err(|e| IpcError::Internal { message: e.to_string() })?;
    let mut disabled = read_disabled();
    disabled.retain(|id| id != &expert.id);
    write_disabled(&disabled)?;
    Ok(expert_info(&expert, true))
}

/// 导出单个自定义专家为 EXPERT.md。
#[tauri::command]
pub fn agent_export_expert(expert_id: String, dest_path: String) -> IpcResult<()> {
    let root = experts_dir().ok_or_else(|| error("无法确定用户数据目录"))?;
    let file = root.join(&expert_id).join("EXPERT.md");
    if !file.is_file() {
        return Err(error("内置专家没有可导出的文件；仅支持导出自定义专家"));
    }
    let content = std::fs::read_to_string(&file).map_err(|e| IpcError::Internal { message: e.to_string() })?;
    std::fs::write(&dest_path, content).map_err(|e| IpcError::Internal { message: e.to_string() })
}

/// 把一次会审选择的专家合成一个自定义专家：人格提示词与维度合并后落盘。
#[tauri::command]
pub fn agent_save_panel_expert(
    name: String,
    expert_ids: Vec<String>,
    mode: String,
    scope: String,
) -> IpcResult<ExpertInfo> {
    let name = name.trim();
    if name.is_empty() {
        return Err(error("请先给专家起个名字"));
    }
    if name.chars().count() > 60 {
        return Err(error("专家名称最多 60 个字"));
    }
    let (all, _) = screenplay_agent::load_experts_detailed(experts_dir().as_deref());
    let picked: Vec<&Expert> = expert_ids
        .iter()
        .filter_map(|id| all.iter().find(|e| &e.id == id))
        .collect();
    if picked.is_empty() {
        return Err(error("请至少选择一位专家"));
    }
    let mode_label = if mode == "rewrite" { "改写建议" } else { "评审" };
    let scope_label = if scope == "whole" { "整稿" } else { "当前场景" };
    let mut body = format!(
        "你是由多位专家方法论合成的自定义专家。面对同一份文本时，依次用下列视角检查，并在输出中标注每条意见来自哪个视角。\n\n本次保存自一次{scope_label}会审（{mode_label}）。\n\n"
    );
    let mut dimensions: Vec<String> = Vec::new();
    let mut max_tokens = 2400u32;
    for (index, expert) in picked.iter().enumerate() {
        body.push_str(&format!(
            "## 视角{}：{}\n{}\n\n",
            index + 1,
            expert.name,
            expert.system_prompt.trim()
        ));
        for dimension in &expert.dimensions {
            if !dimensions.contains(dimension) {
                dimensions.push(dimension.clone());
            }
        }
        max_tokens = max_tokens.max(expert.max_tokens);
    }
    dimensions.truncate(8);
    if dimensions.is_empty() {
        dimensions = vec!["总体判断".into(), "关键问题".into(), "修改建议".into()];
    }
    body.push_str(
        "硬规则：只评审给定文本，引用场景号或原句作为证据，不虚构剧情；区分事实问题、判断问题与口味问题；每条建议说明改哪里、怎么改、为什么。",
    );
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let id = format!("panel-{stamp}");
    let root = experts_dir().ok_or_else(|| error("无法确定用户数据目录"))?;
    let dest_dir = root.join(&id);
    std::fs::create_dir_all(&dest_dir).map_err(|e| IpcError::Internal { message: e.to_string() })?;
    let description = format!(
        "由会审保存：{}",
        picked.iter().map(|e| e.name.as_str()).collect::<Vec<_>>().join("、")
    );
    let text = format!(
        "---\nid: {id}\nname: {name}\ndescription: {description}\ndimensions: {}\ntemperature: 0.4\nmax_tokens: {max_tokens}\n---\n{body}\n",
        dimensions.join("、")
    );
    std::fs::write(dest_dir.join("EXPERT.md"), text)
        .map_err(|e| IpcError::Internal { message: e.to_string() })?;
    Ok(expert_info(
        &Expert {
            id,
            name: name.to_string(),
            description,
            dimensions,
            output_sections: vec![
                "总体判断".into(),
                "关键问题".into(),
                "修改建议".into(),
                "需要作者确认".into(),
            ],
            system_prompt: body,
            temperature: 0.4,
            max_tokens,
            builtin: false,
        },
        true,
    ))
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

#[cfg(test)]
mod tests {
    use super::set_frontmatter_name;

    #[test]
    fn rename_replaces_existing_name_and_keeps_rest() {
        let content = "---\nid: my-expert\nname: 旧名字\ndescription: 说明\n---\n你是专家。\n";
        let updated = set_frontmatter_name(content, "新名字");
        assert!(updated.contains("name: 新名字"));
        assert!(!updated.contains("旧名字"));
        assert!(updated.contains("description: 说明"));
        assert!(updated.contains("你是专家。"));
        // 解析规则不变，改名后仍可正常载入
        let expert = screenplay_agent::parse_expert_md(&updated).expect("改名后仍应可解析");
        assert_eq!(expert.name, "新名字");
    }

    #[test]
    fn rename_inserts_name_when_missing() {
        let content = "---\nid: my-expert\n---\n人格提示词。\n";
        let updated = set_frontmatter_name(content, "没有名字");
        assert!(updated.contains("name: 没有名字"));
        let expert = screenplay_agent::parse_expert_md(&updated).unwrap();
        assert_eq!(expert.name, "没有名字");
    }
}
