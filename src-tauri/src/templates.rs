//! 新工程模板的加载。
//!
//! 内置模板随应用编译；用户模板放在数据目录的 `templates/` 下，
//! 直接放入 JSON 文件即可新增模板，不需要改代码。

use screenplay_core::templates::{self, ProjectTemplate};
use std::path::{Path, PathBuf};

/// 内置模板。顺序即新建对话框中的展示顺序。
const BUILTIN: &[&str] = &[
    include_str!("../templates/blank.json"),
    include_str!("../templates/three-act-beats.json"),
    include_str!("../templates/short-drama-series.json"),
];

const MAX_TEMPLATE_BYTES: u64 = 512 * 1024;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub builtin: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplatesPayload {
    /// 用户模板目录，前端用来提示如何新增模板。
    pub dir: String,
    pub templates: Vec<TemplateSummary>,
}

pub fn user_template_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("ScreenplayStudio").join("templates"))
}

fn builtin_templates() -> Vec<ProjectTemplate> {
    BUILTIN
        .iter()
        .filter_map(|json| match templates::parse(json) {
            Ok(template) => Some(template),
            Err(error) => {
                crate::diag::log(&format!("内置模板解析失败: {error}"));
                None
            }
        })
        .collect()
}

fn load_user_templates(dir: &Path) -> Vec<ProjectTemplate> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
        })
        .collect();
    files.sort();
    files
        .into_iter()
        .filter_map(|path| {
            let too_large = std::fs::metadata(&path)
                .map(|m| m.len() > MAX_TEMPLATE_BYTES)
                .unwrap_or(false);
            if too_large {
                crate::diag::log(&format!("模板文件过大，已跳过: {}", path.display()));
                return None;
            }
            let json = match std::fs::read_to_string(&path) {
                Ok(json) => json,
                Err(error) => {
                    crate::diag::log(&format!("读取模板失败 {}: {error}", path.display()));
                    return None;
                }
            };
            match templates::parse(&json) {
                Ok(template) => Some(template),
                Err(error) => {
                    crate::diag::log(&format!("模板解析失败 {}: {error}", path.display()));
                    None
                }
            }
        })
        .collect()
}

/// 内置模板 + 用户模板；用户模板 id 与内置相同时覆盖内置。
pub fn all_templates(dir: Option<&Path>) -> Vec<(ProjectTemplate, bool)> {
    let mut result: Vec<(ProjectTemplate, bool)> =
        builtin_templates().into_iter().map(|t| (t, true)).collect();
    if let Some(dir) = dir {
        for user in load_user_templates(dir) {
            match result.iter_mut().find(|(t, _)| t.id == user.id) {
                Some(slot) => *slot = (user, false),
                None => result.push((user, false)),
            }
        }
    }
    result
}

pub fn find(id: &str) -> Option<ProjectTemplate> {
    let dir = user_template_dir();
    all_templates(dir.as_deref())
        .into_iter()
        .find(|(template, _)| template.id == id)
        .map(|(template, _)| template)
}

#[tauri::command]
pub fn list_project_templates() -> TemplatesPayload {
    let dir = user_template_dir();
    if let Some(dir) = dir.as_deref() {
        // 建好目录，用户才能直接把 JSON 模板放进去。
        let _ = std::fs::create_dir_all(dir);
    }
    let templates = all_templates(dir.as_deref())
        .into_iter()
        .map(|(template, builtin)| TemplateSummary {
            id: template.id,
            name: template.name,
            description: template.description,
            builtin,
        })
        .collect();
    TemplatesPayload {
        dir: dir
            .map(|p| p.display().to_string())
            .unwrap_or_else(String::new),
        templates,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_load_in_dialog_order() {
        let templates = all_templates(None);
        let ids: Vec<&str> = templates.iter().map(|(t, _)| t.id.as_str()).collect();
        assert_eq!(ids, vec!["blank", "three-act-beats", "short-drama-series"]);
        assert!(templates.iter().all(|(_, builtin)| *builtin));
    }

    #[test]
    fn user_templates_are_scanned_and_can_override_builtins() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("custom.json"),
            r#"{"id":"custom","name":"自定义","description":"用户模板","blocks":[{"type":"action","text":"你好"}]}"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("blank.json"),
            r#"{"id":"blank","name":"我的空白","description":"覆盖内置","blocks":[]}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("broken.json"), "{ not json").unwrap();
        let templates = all_templates(Some(dir.path()));
        assert_eq!(templates.len(), 4);
        assert_eq!(templates[0].0.name, "我的空白");
        assert!(!templates[0].1);
        assert_eq!(templates[3].0.id, "custom");
    }
}
