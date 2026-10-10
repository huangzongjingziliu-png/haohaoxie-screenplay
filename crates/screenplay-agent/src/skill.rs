//! Skill 注册中心。
//!
//! Skill 是可复用的 AI 能力单元（续写、润色、扩写、转场等），
//! 每个 Skill 定义系统提示词、温度和输入参数。
//!
//! ## SKILL.md 格式（说明书 E-07）
//!
//! 每个 Skill 是一个目录，内含 `SKILL.md`：
//!
//! ```markdown
//! ---
//! id: continue
//! name: 续写
//! description: 根据上下文续写一段剧本
//! temperature: 0.7
//! max_tokens: 2000
//! ---
//!
//! 你是专业编剧。请基于以下剧本上下文，续写接下来的内容。
//! 直接输出剧本内容，不要解释。
//!
//! {input}
//! ```
//!
//! frontmatter 用 YAML 风格的 `key: value`（只支持单层），
//! body 就是 system_prompt 模板，可包含 `{input}` 占位符。
//! 目录下还可放 `scripts/`、`references/`、`assets/` 等辅助资源。

use std::collections::HashMap;
use std::path::Path;

/// 一个 Skill 定义
#[derive(Debug, Clone)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    /// 系统提示词模板，可包含 `{input}` 占位符
    pub system_prompt: String,
    /// 模型温度
    pub temperature: f32,
    /// 最大输出 token 数（仅作提示用，不强制）
    pub max_tokens: u32,
    /// 是否为内置 skill（内置不可卸载）
    pub builtin: bool,
}

impl Skill {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        system_prompt: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            system_prompt: system_prompt.into(),
            temperature: 0.7,
            max_tokens: 2000,
            builtin: false,
        }
    }

    /// 标记为内置 skill（不可卸载）
    pub fn builtin(mut self) -> Self {
        self.builtin = true;
        self
    }

    /// 渲染系统提示词，替换 `{input}`
    pub fn render(&self, input: &str) -> String {
        self.system_prompt.replace("{input}", input)
    }
}

/// 内置技能工厂
pub fn builtin_skills() -> Vec<Skill> {
    vec![
        Skill::new(
            "continue",
            "续写",
            "根据上下文续写一段剧本动作或对白",
            "你是专业编剧。请基于以下剧本上下文，续写接下来的内容，保持场景与人物语气一致。直接输出剧本内容，不要解释。\n\n{input}",
        ).builtin(),
        Skill::new(
            "polish",
            "润色",
            "在不改变原意的前提下润色对白与动作描写",
            "你是专业剧本编辑。请润色以下剧本片段，使对白更自然、动作描写更生动，但不改变情节和人物设定。直接输出润色后的剧本，不要解释。\n\n{input}",
        ).builtin(),
        Skill::new(
            "expand",
            "扩写",
            "将简略的动作描写扩写为更详细的场景",
            "你是专业编剧。请将以下简略的剧本片段扩写，增加环境描写、人物动作和细节，但保持原有情节走向。直接输出扩写后的剧本。\n\n{input}",
        ).builtin(),
        Skill::new(
            "dialogue",
            "对白优化",
            "优化对白的节奏与口语化程度",
            "你是专业对白编辑。请优化以下对白，使其更符合角色身份、更有潜台词和节奏感。直接输出修改后的对白，不要解释。\n\n{input}",
        ).builtin(),
    ]
}

/// Skill 注册中心
#[derive(Debug, Default)]
pub struct SkillRegistry {
    skills: HashMap<String, Skill>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        let mut r = Self { skills: HashMap::new() };
        for s in builtin_skills() {
            r.register(s);
        }
        r
    }

    pub fn register(&mut self, skill: Skill) {
        if self.skills.get(&skill.id).is_some_and(|existing| existing.builtin) && !skill.builtin {
            return;
        }
        self.skills.insert(skill.id.clone(), skill);
    }

    pub fn get(&self, id: &str) -> Option<&Skill> {
        self.skills.get(id)
    }

    pub fn list(&self) -> Vec<&Skill> {
        self.skills.values().collect()
    }

    /// 卸载（移除）一个 skill。内置 skill 不可卸载。
    pub fn unload(&mut self, id: &str) -> bool {
        if let Some(s) = self.skills.get(id) {
            if s.builtin {
                return false;
            }
        }
        self.skills.remove(id).is_some()
    }

    /// 重新加载：清空非内置 skill，重新从目录加载。
    pub fn reload_from_dir(&mut self, dir: &Path) -> Result<usize, SkillLoadError> {
        self.skills.retain(|_, s| s.builtin);
        let before = self.skills.len();
        self.load_from_dir(dir)?;
        Ok(self.skills.len() - before)
    }

    /// 从一个目录加载所有 Skill（每个子目录含 SKILL.md）。
    /// 加载失败的 skill 会被跳过并记录到返回的 warnings 中。
    pub fn load_from_dir(&mut self, dir: &Path) -> Result<(), SkillLoadError> {
        if !dir.exists() {
            return Err(SkillLoadError::DirNotFound(dir.to_path_buf()));
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let skill_md = path.join("SKILL.md");
            if !skill_md.exists() {
                continue;
            }
            let content = std::fs::read_to_string(&skill_md)?;
            match parse_skill_md(&content) {
                Ok(skill) => {
                    self.register(skill);
                }
                Err(e) => {
                    eprintln!("[skill] 跳过 {}: {}", skill_md.display(), e);
                }
            }
        }
        Ok(())
    }
}

/// Skill IDs are single directory names, never filesystem paths.
pub fn is_valid_skill_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128
        && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// 解析 SKILL.md 内容（frontmatter + body）
pub fn parse_skill_md(content: &str) -> Result<Skill, SkillLoadError> {
    let (frontmatter, body) = split_frontmatter(content)?;

    let mut id = String::new();
    let mut name = String::new();
    let mut description = String::new();
    let mut temperature = 0.7f32;
    let mut max_tokens = 2000u32;

    for line in frontmatter.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"').trim_matches('\'');
        match key {
            "id" => id = value.to_string(),
            "name" => name = value.to_string(),
            "description" => description = value.to_string(),
            "temperature" => {
                temperature = value
                    .parse::<f32>()
                    .map_err(|_| SkillLoadError::InvalidField("temperature".into()))?;
            }
            "max_tokens" => {
                max_tokens = value
                    .parse::<u32>()
                    .map_err(|_| SkillLoadError::InvalidField("max_tokens".into()))?;
            }
            _ => {} // 忽略未知字段
        }
    }

    if id.is_empty() {
        return Err(SkillLoadError::MissingField("id".into()));
    }

    if !is_valid_skill_id(&id) {
        return Err(SkillLoadError::InvalidField("id must be a safe directory name".into()));
    }
    let name = if name.is_empty() { id.clone() } else { name };

    Ok(Skill {
        id,
        name,
        description,
        system_prompt: body.trim().to_string(),
        temperature,
        max_tokens,
        builtin: false,
    })
}

/// 分离 frontmatter（`---` 之间）和 body
fn split_frontmatter(content: &str) -> Result<(String, String), SkillLoadError> {
    let content = content.trim_start();
    let rest = content
        .strip_prefix("---")
        .ok_or(SkillLoadError::NoFrontmatter)?;
    let rest = rest.trim_start_matches('\n');
    let end = rest
        .find("\n---")
        .ok_or(SkillLoadError::NoFrontmatter)?;
    let frontmatter = rest[..end].to_string();
    let body = rest[end + 4..].trim_start_matches('\n').to_string();
    Ok((frontmatter, body))
}

#[derive(Debug, thiserror::Error)]
pub enum SkillLoadError {
    #[error("目录不存在: {0}")]
    DirNotFound(std::path::PathBuf),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("缺少 frontmatter")]
    NoFrontmatter,
    #[error("缺少必填字段: {0}")]
    MissingField(String),
    #[error("字段值非法: {0}")]
    InvalidField(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t01_parse_basic_skill() {
        let md = r#"---
id: continue
name: 续写
description: 续写剧本
temperature: 0.8
max_tokens: 1500
---

你是专业编剧。
{input}
"#;
        let s = parse_skill_md(md).unwrap();
        assert_eq!(s.id, "continue");
        assert_eq!(s.name, "续写");
        assert_eq!(s.description, "续写剧本");
        assert!((s.temperature - 0.8).abs() < 0.001);
        assert_eq!(s.max_tokens, 1500);
        assert!(s.system_prompt.contains("{input}"));
    }

    #[test]
    fn t02_parse_defaults_optional_fields() {
        let md = r#"---
id: minimal
---

just body
"#;
        let s = parse_skill_md(md).unwrap();
        assert_eq!(s.id, "minimal");
        assert_eq!(s.name, "minimal"); // 默认用 id
        assert!((s.temperature - 0.7).abs() < 0.001);
        assert_eq!(s.max_tokens, 2000);
    }

    #[test]
    fn t03_missing_id_rejected() {
        let md = r#"---
name: no id
---

body
"#;
        let err = parse_skill_md(md).unwrap_err();
        assert!(matches!(err, SkillLoadError::MissingField(_)));
    }

    #[test]
    fn t04_no_frontmatter_rejected() {
        let md = "just body no frontmatter";
        let err = parse_skill_md(md).unwrap_err();
        assert!(matches!(err, SkillLoadError::NoFrontmatter));
    }

    #[test]
    fn t05_invalid_temperature_rejected() {
        let md = r#"---
id: x
temperature: not_a_number
---

body
"#;
        let err = parse_skill_md(md).unwrap_err();
        assert!(matches!(err, SkillLoadError::InvalidField(_)));
    }

    #[test]
    fn t06_load_from_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
id: custom
name: 自定义
description: test
---

custom prompt {input}
"#,
        )
        .unwrap();

        let mut reg = SkillRegistry::new();
        reg.load_from_dir(tmp.path()).unwrap();
        let s = reg.get("custom").unwrap();
        assert_eq!(s.name, "自定义");
    }

    #[test]
    fn rejects_path_ids_and_builtin_overrides() {
        for id in ["../outside", "/tmp/test", "..", "a/b", "a\\b"] {
            assert!(parse_skill_md(&format!("---\nid: {id}\n---\nprompt")).is_err());
        }
        let mut registry = SkillRegistry::new();
        registry.register(Skill::new("polish", "malicious", "", "override"));
        assert!(registry.get("polish").unwrap().builtin);
        assert_ne!(registry.get("polish").unwrap().name, "malicious");
    }

    #[test]
    fn t07_skill_render_replaces_input() {
        let s = Skill::new("t", "T", "d", "hello {input} world");
        assert_eq!(s.render("foo"), "hello foo world");
    }

    #[test]
    fn t08_load_workspace_skills_dir() {
        // 加载仓库根目录的 skills/，验证 4 个内置 skill 都能正确解析
        let manifest = env!("CARGO_MANIFEST_DIR");
        let skills_dir = std::path::Path::new(manifest)
            .join("..")
            .join("..")
            .join("skills");
        assert!(skills_dir.exists(), "required skills fixture directory is missing");
        let mut reg = SkillRegistry::new();
        reg.load_from_dir(&skills_dir).unwrap();
        // 至少应有 4 个内置 skill
        assert!(reg.get("continue").is_some());
        assert!(reg.get("polish").is_some());
        assert!(reg.get("expand").is_some());
        assert!(reg.get("dialogue").is_some());
    }
}
