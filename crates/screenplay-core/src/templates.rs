//! 新工程模板：纯数据描述初始结构，模板只影响新建时生成的文档。
//!
//! 模板以 JSON 文件保存（内置模板随应用编译，用户模板从数据目录读取），
//! 新增模板不需要修改代码。

use crate::ast::{BlockType, ScreenplayBlock, ScreenplayDocument, ScreenplaySettings};
use crate::heading_format;
use serde::{Deserialize, Serialize};

/// 模板中的一个剧本元素。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateBlock {
    #[serde(rename = "type")]
    pub block_type: BlockType,
    pub text: String,
}

/// 分集模板中的一集。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateEpisode {
    pub title: String,
    #[serde(default)]
    pub blocks: Vec<TemplateBlock>,
}

/// 一套新工程模板。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectTemplate {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub blocks: Vec<TemplateBlock>,
    #[serde(default)]
    pub episodes: Option<Vec<TemplateEpisode>>,
}

impl ProjectTemplate {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty()
            || self.id.len() > 64
            || !self
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err("模板 id 只能包含字母、数字、连字符和下划线，且不超过 64 个字符".into());
        }
        if self.name.trim().is_empty() || self.name.chars().count() > 40 {
            return Err(format!("模板 {} 的名称不能为空，且不超过 40 字", self.id));
        }
        if self.description.chars().count() > 200 {
            return Err(format!("模板 {} 的说明不能超过 200 字", self.id));
        }
        if self.episodes.is_some() && !self.blocks.is_empty() {
            return Err(format!("模板 {} 不能同时包含 blocks 与 episodes", self.id));
        }
        if let Some(episodes) = &self.episodes {
            if episodes.is_empty() {
                return Err(format!("模板 {} 至少需要一集", self.id));
            }
            for (i, episode) in episodes.iter().enumerate() {
                if episode.title.trim().is_empty() {
                    return Err(format!("模板 {} 的第 {} 集缺少标题", self.id, i + 1));
                }
            }
        }
        Ok(())
    }
}

/// 解析模板 JSON 并校验结构。
pub fn parse(json: &str) -> Result<ProjectTemplate, String> {
    let template: ProjectTemplate =
        serde_json::from_str(json).map_err(|e| format!("模板 JSON 解析失败：{e}"))?;
    template.validate()?;
    Ok(template)
}

/// 按模板生成新工程文档。
pub fn build_document(title: &str, template: &ProjectTemplate) -> Result<ScreenplayDocument, String> {
    template.validate()?;
    let mut doc = ScreenplayDocument::blank(title);
    let settings = doc.settings.clone();
    match &template.episodes {
        Some(episodes) => {
            doc.blocks = build_blocks(&episodes[0].blocks, &settings);
            crate::series::edit(&mut doc, "enable", "", &episodes[0].title, &[])?;
            for episode in &episodes[1..] {
                doc.rebuild_derived();
                crate::series::edit(&mut doc, "add", "", &episode.title, &[])?;
                doc.blocks = build_blocks(&episode.blocks, &settings);
            }
            doc.rebuild_derived();
            let first = doc
                .series
                .as_ref()
                .map(|s| s.episodes[0].id.clone())
                .ok_or("模板分集创建失败")?;
            crate::series::edit(&mut doc, "switch", &first, "", &[])?;
        }
        None => doc.blocks = build_blocks(&template.blocks, &settings),
    }
    doc.rebuild_derived();
    Ok(doc)
}

fn build_blocks(specs: &[TemplateBlock], settings: &ScreenplaySettings) -> Vec<ScreenplayBlock> {
    specs.iter().map(|spec| build_block(spec, settings)).collect()
}

fn build_block(spec: &TemplateBlock, settings: &ScreenplaySettings) -> ScreenplayBlock {
    let mut block = ScreenplayBlock::new(spec.block_type, spec.text.clone());
    if spec.block_type == BlockType::SceneHeading {
        if let Some((display, data)) = heading_format::format_heading(&spec.text, settings) {
            block.text = display;
            block.scene_heading = Some(data);
        }
    }
    block
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUILTINS: &[&str] = &[
        include_str!("../../../src-tauri/templates/blank.json"),
        include_str!("../../../src-tauri/templates/three-act-beats.json"),
        include_str!("../../../src-tauri/templates/short-drama-series.json"),
    ];

    #[test]
    fn builtins_parse_and_build_expected_structures() {
        let blank = parse(BUILTINS[0]).unwrap();
        assert_eq!(blank.id, "blank");
        let doc = build_document("空白", &blank).unwrap();
        assert!(doc.blocks.is_empty());
        assert!(doc.series.is_none());

        let three_act = parse(BUILTINS[1]).unwrap();
        let doc = build_document("长片", &three_act).unwrap();
        assert!(doc.series.is_none());
        assert_eq!(
            doc.blocks
                .iter()
                .filter(|b| b.block_type == BlockType::SceneHeading)
                .count(),
            15
        );
        assert_eq!(
            doc.blocks
                .iter()
                .filter(|b| b.block_type == BlockType::Section)
                .count(),
            3
        );
        let heading = doc
            .blocks
            .iter()
            .find(|b| b.block_type == BlockType::SceneHeading)
            .unwrap();
        assert!(heading.scene_heading.is_some(), "场景标题应解析成结构化数据");
        assert!(doc.scenes.len() == 15);

        let series = parse(BUILTINS[2]).unwrap();
        let doc = build_document("短剧", &series).unwrap();
        let series = doc.series.as_ref().unwrap();
        assert_eq!(series.episodes.len(), 3);
        assert_eq!(series.episodes[0].id, series.active_id);
        assert_eq!(
            doc.blocks
                .iter()
                .filter(|b| b.block_type == BlockType::SceneHeading)
                .count(),
            4
        );
        let drafts = crate::series::export_drafts(&doc).unwrap();
        assert_eq!(drafts.len(), 3);
        for (_, draft) in drafts {
            assert_eq!(
                draft
                    .blocks
                    .iter()
                    .filter(|b| b.block_type == BlockType::SceneHeading)
                    .count(),
                4
            );
        }
    }

    #[test]
    fn generated_blocks_are_plain_editable_blocks() {
        let template = parse(BUILTINS[1]).unwrap();
        let doc = build_document("可编辑", &template).unwrap();
        for block in &doc.blocks {
            assert!(!block.id.is_empty());
            assert_eq!(block.revision, 0);
        }
        assert!(doc.blocks.iter().all(|b| !b.metadata.locked));
    }

    #[test]
    fn invalid_templates_are_rejected() {
        assert!(parse("{}").is_err());
        assert!(parse(r#"{"id":"bad id","name":"坏"}"#).is_err());
        assert!(parse(r#"{"id":"x","name":"坏","blocks":[],"episodes":[]}"#).is_err());
        assert!(parse(r#"{"id":"x","name":"坏","blocks":[{"type":"unknown","text":"x"}]}"#).is_err());
    }
}
