//! Screenplay AST — 剧本结构化文档模型。
//!
//! 这是整个软件唯一权威数据源。GUI / Agent / CLI / MCP 全部通过
//! `ScreenplayCommandService` 修改本文档，任何一方都不允许绕过。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

/// 剧本元素类型。顺序即 FDX 标准顺序。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src/ipc/types/")]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub enum BlockType {
    SceneHeading,
    Action,
    Character,
    Parenthetical,
    Dialogue,
    DualDialogue,
    Transition,
    Shot,
    Note,
    Section,
    Synopsis,
    General,
    PageBreak,
}

impl BlockType {
    pub fn as_str(&self) -> &'static str {
        match self {
            BlockType::SceneHeading => "scene_heading",
            BlockType::Action => "action",
            BlockType::Character => "character",
            BlockType::Parenthetical => "parenthetical",
            BlockType::Dialogue => "dialogue",
            BlockType::DualDialogue => "dual_dialogue",
            BlockType::Transition => "transition",
            BlockType::Shot => "shot",
            BlockType::Note => "note",
            BlockType::Section => "section",
            BlockType::Synopsis => "synopsis",
            BlockType::General => "general",
            BlockType::PageBreak => "page_break",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "scene_heading" => BlockType::SceneHeading,
            "action" => BlockType::Action,
            "character" => BlockType::Character,
            "parenthetical" => BlockType::Parenthetical,
            "dialogue" => BlockType::Dialogue,
            "dual_dialogue" => BlockType::DualDialogue,
            "transition" => BlockType::Transition,
            "shot" => BlockType::Shot,
            "note" => BlockType::Note,
            "section" => BlockType::Section,
            "synopsis" => BlockType::Synopsis,
            "general" => BlockType::General,
            "page_break" => BlockType::PageBreak,
            _ => return None,
        })
    }

    /// FDX Paragraph Type
    pub fn to_fdx(&self) -> &'static str {
        match self {
            BlockType::SceneHeading => "Scene Heading",
            BlockType::Action => "Action",
            BlockType::Character => "Character",
            BlockType::Parenthetical => "Parenthetical",
            BlockType::Dialogue => "Dialogue",
            BlockType::DualDialogue => "Dialogue",
            BlockType::Transition => "Transition",
            BlockType::Shot => "Shot",
            BlockType::Note => "Note",
            BlockType::Section => "Section",
            BlockType::Synopsis => "Synopsis",
            BlockType::General => "General",
            BlockType::PageBreak => "Page Break",
        }
    }

    pub fn from_fdx(s: &str) -> Option<Self> {
        Some(match s.trim() {
            "Scene Heading" => BlockType::SceneHeading,
            "Action" => BlockType::Action,
            "Character" => BlockType::Character,
            "Parenthetical" => BlockType::Parenthetical,
            "Dialogue" => BlockType::Dialogue,
            "Transition" => BlockType::Transition,
            "Shot" => BlockType::Shot,
            "Note" => BlockType::Note,
            "Section" => BlockType::Section,
            "Synopsis" => BlockType::Synopsis,
            "General" => BlockType::General,
            "Page Break" => BlockType::PageBreak,
            _ => return None,
        })
    }
}

/// 内/外景
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export, export_to = "../../../src/ipc/types/")]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub enum IntExt {
    Int,
    Ext,
    IntExt,
    Unknown,
}

impl IntExt {
    pub fn to_zh(&self) -> &'static str {
        match self {
            IntExt::Int => "内",
            IntExt::Ext => "外",
            IntExt::IntExt => "内/外",
            IntExt::Unknown => "",
        }
    }
    pub fn to_en(&self) -> &'static str {
        match self {
            IntExt::Int => "INT",
            IntExt::Ext => "EXT",
            IntExt::IntExt => "INT/EXT",
            IntExt::Unknown => "",
        }
    }
}

/// 场景标题的结构化字段（支持场景统计、地点统计、FDX 导出）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct SceneHeadingData {
    pub scene_number: Option<String>,
    pub int_ext: IntExt,
    pub location: String,
    pub time_of_day: String,
    /// `zh` 或 `en`，决定呈现为「外. 镇北堡 - 黄昏」还是「EXT. ZHENBEIBU - DUSK」
    pub display_language: String,
}

impl Default for SceneHeadingData {
    fn default() -> Self {
        Self {
            scene_number: None,
            int_ext: IntExt::Unknown,
            location: String::new(),
            time_of_day: String::new(),
            display_language: "zh".into(),
        }
    }
}

/// 右侧自由文本中的来源范围 —— 增量双向同步的关键
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct SourceRange {
    pub from: usize,
    pub to: usize,
    /// 源语法标记，例如 `inline-dialogue`（人物+括注+对白合并为一行）
    pub syntax: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct BlockMetadata {
    /// 修订标记颜色
    pub revision_color: Option<String>,
    /// 大纲颜色
    pub color: Option<String>,
    /// 场景状态
    pub status: Option<String>,
    /// dual dialogue 配对 id
    pub dual_with: Option<String>,
    /// 锁定（Agent 不可修改）
    pub locked: bool,
}

impl Default for BlockMetadata {
    fn default() -> Self {
        Self {
            revision_color: None,
            color: None,
            status: None,
            dual_with: None,
            locked: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct ScreenplayBlock {
    /// 全局唯一 UUID（非顺序，便于稳定同步）
    pub id: String,
    pub block_type: BlockType,
    pub text: String,
    pub scene_id: Option<String>,
    pub character_id: Option<String>,
    pub source: Option<SourceRange>,
    /// 识别置信度 0.0 - 1.0
    pub confidence: f32,
    /// 该 block 最后一次被修改时的文档 revision
    #[ts(type = "number")]
    pub revision: u64,
    pub scene_heading: Option<SceneHeadingData>,
    #[serde(default)]
    pub metadata: BlockMetadata,
}

impl ScreenplayBlock {
    pub fn new(block_type: BlockType, text: impl Into<String>) -> Self {
        Self {
            id: new_id("B"),
            block_type,
            text: text.into(),
            scene_id: None,
            character_id: None,
            source: None,
            confidence: 1.0,
            revision: 0,
            scene_heading: None,
            metadata: BlockMetadata::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct CharacterEntity {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct LocationEntity {
    pub id: String,
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct Scene {
    pub id: String,
    pub number: Option<String>,
    pub int_ext: IntExt,
    pub location: String,
    pub time_of_day: String,
    pub synopsis: Option<String>,
    pub note: Option<String>,
    pub color: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct TitlePage {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub written_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub based_on: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copyright: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct PaperSettings {
    pub template: String,
    pub page_width_inch: f64,
    pub page_height_inch: f64,
    pub margin_top_inch: f64,
    pub margin_bottom_inch: f64,
    pub margin_left_inch: f64,
    pub margin_right_inch: f64,
}

impl Default for PaperSettings {
    fn default() -> Self {
        Self {
            template: "us-letter".into(),
            page_width_inch: 8.5,
            page_height_inch: 11.0,
            margin_top_inch: 1.0,
            margin_bottom_inch: 1.0,
            margin_left_inch: 1.5,
            margin_right_inch: 1.0,
        }
    }
}

impl PaperSettings {
    pub fn a4() -> Self {
        Self {
            template: "a4".into(),
            page_width_inch: 8.27,
            page_height_inch: 11.69,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct ElementStyle {
    pub font_family: String,
    pub font_size: f64,
    pub bold: bool,
    pub italic: bool,
    pub color: String,
}

/// Story board presentation lives in the project; scene headings remain the source of order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct SceneCard {
    pub color: String,
    pub width: f64,
    pub height: f64,
    pub summary: String,
    pub summary_source: String,
    #[serde(default)]
    pub summary_manual: bool,
    #[serde(default)]
    pub comments: Vec<String>,
}
impl Default for SceneCard {
    fn default() -> Self { Self { color: String::new(), width: 0.0, height: 0.0, summary: String::new(), summary_source: String::new(), summary_manual: false, comments: Vec::new() } }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct SceneGroup { pub id:String, pub name:String, pub scene_ids:Vec<String> }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct BoardLink { pub id:String, pub from:String, pub to:String, pub label:String, pub arrow:String }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct SceneBoard {
    pub color: String,
    pub summaries_enabled: bool,
    #[serde(default)]
    pub groups: Vec<SceneGroup>,
    #[serde(default)]
    pub links: Vec<BoardLink>,
    #[serde(default)]
    pub hide_links: bool,
    pub cards: BTreeMap<String, SceneCard>,
}
impl Default for SceneBoard {
    fn default() -> Self { Self { color: "#FFF2A8".into(), summaries_enabled: false, groups: Vec::new(), links: Vec::new(), hide_links: false, cards: BTreeMap::new() } }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct ProjectMemory {
    pub markdown: String,
    pub author_notes: String,
    pub history: Vec<String>,
    pub source_fingerprint: String,
    pub updated_at: String,
    #[serde(default = "memory_enabled")]
    pub auto_enabled: bool,
}
fn memory_enabled() -> bool { true }
fn default_project_memory() -> ProjectMemory { ProjectMemory { auto_enabled: true, ..Default::default() } }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct ScreenplaySettings {
    #[serde(default = "default_project_memory")]
    pub project_memory: ProjectMemory,
    #[serde(default)]
    pub scene_board: SceneBoard,
    #[serde(default = "default_typography")]
    pub typography: String,
    #[serde(default)]
    pub element_styles: std::collections::BTreeMap<String, ElementStyle>,
    #[serde(default)]
    pub style_preset_name: String,
    #[serde(flatten)]
    #[ts(flatten)]
    pub paper: PaperSettings,
    pub font_family: String,
    pub font_size: f64,
    pub line_height: f64,
    pub show_page_numbers: bool,
    pub show_scene_numbers: bool,
    pub show_more_contd: bool,
    pub include_title_page: bool,
    pub language: String,
    pub theme: String,
    pub auto_save: bool,
    #[serde(default = "default_save_interval")]
    #[ts(type = "number")]
    pub auto_save_interval_secs: u64,
    pub auto_version_interval_secs: u64,
    #[serde(default)]
    pub ocr_languages: Vec<String>,
}

fn default_save_interval() -> u64 { 30 }

fn default_typography() -> String { "hollywood".into() }

impl Default for ScreenplaySettings {
    fn default() -> Self {
        Self {
            project_memory: default_project_memory(),
            scene_board: SceneBoard::default(),
            paper: PaperSettings::default(),
            typography: default_typography(),
            element_styles: Default::default(),
            style_preset_name: String::new(),
            font_family: "\"Courier Prime\", Courier, \"PingFang SC\", monospace".into(),
            font_size: 12.0,
            line_height: 1.0,
            show_page_numbers: true,
            show_scene_numbers: false,
            show_more_contd: true,
            include_title_page: true,
            language: "zh".into(),
            theme: "light".into(),
            auto_save: true,
            auto_save_interval_secs: 30,
            auto_version_interval_secs: 300,
            ocr_languages: vec!["zh-Hans".into(), "en-US".into()],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../src/ipc/types/")]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub enum SourceType {
    Pdf,
    Image,
    Docx,
    Doc,
    Rtf,
    Fdx,
    Fountain,
    Txt,
    Markdown,
    Clipboard,
    Blank,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct ImportInfo {
    pub source_type: SourceType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_name: Option<String>,
    pub imported_at: String,
    /// 未经 parser 修改的原文，始终可恢复
    pub original_text: String,
    #[serde(default)]
    pub removed_noise: Vec<String>,
    /// 导出时是否要求逐字保留原文
    pub preserve_original: bool,
}

/// Story Bible —— 供 Agent 检索，不进入正式剧本
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct StoryBible {
    #[serde(default)]
    pub character_bios: BTreeMap<String, String>,
    #[serde(default)]
    pub world: String,
    #[serde(default)]
    pub era: String,
    #[serde(default)]
    pub relationships: String,
    #[serde(default)]
    pub principles: String,
    #[serde(default)]
    pub references: String,
}

/// PROJECT_RULES.md —— Agent 任何修改必须优先遵守
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct ProjectRules {
    pub enabled: bool,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct ScreenplayDocument {
    pub id: String,
    pub format_version: u32,
    /// 单调递增修订号，用于 Revision Conflict 检测
    #[ts(type = "number")]
    pub revision: u64,
    pub metadata: Metadata,
    pub title_page: TitlePage,
    pub blocks: Vec<ScreenplayBlock>,
    pub characters: Vec<CharacterEntity>,
    pub locations: Vec<LocationEntity>,
    pub scenes: Vec<Scene>,
    pub settings: ScreenplaySettings,
    #[serde(default)]
    pub story_bible: StoryBible,
    #[serde(default)]
    pub project_rules: ProjectRules,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub import_info: Option<ImportInfo>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct Metadata {
    pub title: String,
    pub author: String,
    pub language: String,
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// 生成带前缀的唯一 ID
pub fn new_id(prefix: &str) -> String {
    format!("{}-{}", prefix, uuid::Uuid::new_v4().simple())
}

impl ScreenplayDocument {
    pub fn blank(title: &str) -> Self {
        let now = now_iso();
        Self {
            id: new_id("doc"),
            format_version: 1,
            revision: 0,
            metadata: Metadata {
                title: title.to_string(),
                author: String::new(),
                language: "zh".into(),
            },
            title_page: TitlePage {
                title: title.to_string(),
                ..Default::default()
            },
            blocks: Vec::new(),
            characters: Vec::new(),
            locations: Vec::new(),
            scenes: Vec::new(),
            settings: ScreenplaySettings::default(),
            story_bible: StoryBible::default(),
            project_rules: ProjectRules::default(),
            import_info: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    pub fn block(&self, id: &str) -> Option<&ScreenplayBlock> {
        self.blocks.iter().find(|b| b.id == id)
    }

    pub fn block_mut(&mut self, id: &str) -> Option<&mut ScreenplayBlock> {
        self.blocks.iter_mut().find(|b| b.id == id)
    }

    pub fn block_index(&self, id: &str) -> Option<usize> {
        self.blocks.iter().position(|b| b.id == id)
    }

    pub fn scene(&self, id: &str) -> Option<&Scene> {
        self.scenes.iter().find(|s| s.id == id)
    }

    /// 全部文本量（用于导入完整性校验）
    pub fn total_text_len(&self) -> usize {
        self.blocks.iter().map(|b| b.text.chars().count()).sum()
    }

    pub fn character_by_name(&self, name: &str) -> Option<&CharacterEntity> {
        self.characters.iter().find(|c| c.name == name)
    }

    /// 校验不变量：Block ID 唯一、引用有效
    pub fn validate(&self) -> Result<(), String> {
        let mut seen = std::collections::HashSet::new();
        for b in &self.blocks {
            if !seen.insert(b.id.clone()) {
                return Err(format!("Duplicate block id: {}", b.id));
            }
        }
        let scene_ids: std::collections::HashSet<_> = self.scenes.iter().map(|s| &s.id).collect();
        for b in &self.blocks {
            if let Some(sid) = &b.scene_id {
                if !scene_ids.contains(sid) {
                    return Err(format!("Block {} references unknown scene {}", b.id, sid));
                }
            }
        }
        let char_ids: std::collections::HashSet<_> = self.characters.iter().map(|c| &c.id).collect();
        for b in &self.blocks {
            if let Some(cid) = &b.character_id {
                if !char_ids.contains(cid) {
                    return Err(format!("Block {} references unknown character {}", b.id, cid));
                }
            }
        }
        Ok(())
    }

    /// 重建 scenes / characters / locations 派生数据
    pub fn rebuild_derived(&mut self) {
        let mut scenes: Vec<Scene> = Vec::new();
        let mut current: Option<String> = None;
        let mut n = 0usize;
        for b in self.blocks.iter_mut() {
            if b.block_type == BlockType::SceneHeading {
                n += 1;
                let sid = b.id.clone();
                let data = b.scene_heading.clone().unwrap_or_default();
                scenes.push(Scene {
                    id: sid.clone(),
                    number: data.scene_number.clone().or(Some(n.to_string())),
                    int_ext: data.int_ext,
                    location: data.location.clone(),
                    time_of_day: data.time_of_day.clone(),
                    synopsis: None,
                    note: None,
                    color: b.metadata.color.clone(),
                    status: b.metadata.status.clone(),
                });
                b.scene_id = Some(sid);
            } else {
                b.scene_id = current.clone();
            }
            current = b.scene_id.clone();
        }
        self.scenes = scenes;

        let mut chars: Vec<CharacterEntity> = Vec::new();
        let mut by_name: BTreeMap<String, usize> = BTreeMap::new();
        let mut last_char_id: Option<String> = None;
        for b in self.blocks.iter_mut() {
            if b.block_type == BlockType::Character {
                let name = crate::typography::character_parts(&b.text).0.to_string();
                if name.is_empty() {
                    continue;
                }
                if let Some(&idx) = by_name.get(&name) {
                    b.character_id = Some(chars[idx].id.clone());
                } else {
                    let id = new_id("char");
                    by_name.insert(name.clone(), chars.len());
                    chars.push(CharacterEntity {
                        id: id.clone(),
                        name,
                        aliases: Vec::new(),
                    });
                    b.character_id = Some(id);
                }
                last_char_id = b.character_id.clone();
            } else if b.block_type == BlockType::Dialogue
                || b.block_type == BlockType::Parenthetical
            {
                // 对白/括注继承前一个 character
                b.character_id = last_char_id.clone();
            }
        }
        // 合并 aliases
        for b in self.blocks.iter() {
            if b.block_type == BlockType::Character {
                if let Some(cid) = &b.character_id {
                    if let Some(c) = chars.iter_mut().find(|c| c.id == *cid) {
                        let t = crate::typography::character_parts(&b.text).0;
                        if t != c.name && !c.aliases.contains(&t.to_string()) {
                            c.aliases.push(t.to_string());
                        }
                    }
                }
            }
        }
        self.characters = chars;

        let mut locs: BTreeMap<String, usize> = BTreeMap::new();
        for s in &self.scenes {
            if !s.location.is_empty() {
                *locs.entry(s.location.clone()).or_insert(0) += 1;
            }
        }
        self.locations = locs
            .into_iter()
            .enumerate()
            .map(|(i, (name, count))| LocationEntity {
                id: format!("loc-{}", i),
                name,
                count,
            })
            .collect();
    }

    pub fn stats(&self) -> DocumentStats {
        let dialogues = self
            .blocks
            .iter()
            .filter(|b| b.block_type == BlockType::Dialogue)
            .count();
        let actions = self
            .blocks
            .iter()
            .filter(|b| b.block_type == BlockType::Action)
            .count();
        let words: usize = self
            .blocks
            .iter()
            .map(|b| b.text.chars().count())
            .sum();
        // 页数由权威分页引擎计算。
        //
        // 原实现硬编码为 0（注释写「由 PaginationEngine 填充」但从未实现），
        // 导致 GUI 状态栏、CLI `info`、MCP `document_stats` 全部显示 0 页。
        // 这里直接调用 pagination::paginate —— 统计与纸面/PDF 用同一份分页，
        // 不存在「状态栏 0 页、纸面 12 页」的可能（说明书 D-08）。
        let pages = crate::pagination::paginate(self).total_pages;
        DocumentStats {
            pages,
            scenes: self.scenes.len(),
            characters: self.characters.len(),
            dialogues,
            actions,
            words,
            estimated_minutes: words.div_ceil(350),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/ipc/types/")]
pub struct DocumentStats {
    pub pages: usize,
    pub scenes: usize,
    pub characters: usize,
    pub dialogues: usize,
    pub actions: usize,
    pub words: usize,
    pub estimated_minutes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_type_roundtrip_str() {
        for bt in [
            BlockType::SceneHeading,
            BlockType::Action,
            BlockType::Character,
            BlockType::Parenthetical,
            BlockType::Dialogue,
            BlockType::DualDialogue,
            BlockType::Transition,
            BlockType::Shot,
            BlockType::Note,
            BlockType::Section,
            BlockType::Synopsis,
            BlockType::General,
            BlockType::PageBreak,
        ] {
            assert_eq!(BlockType::from_str(bt.as_str()), Some(bt));
        }
    }

    #[test]
    fn block_type_roundtrip_fdx() {
        for bt in [
            BlockType::SceneHeading,
            BlockType::Action,
            BlockType::Character,
            BlockType::Parenthetical,
            BlockType::Dialogue,
            BlockType::Transition,
            BlockType::Shot,
            BlockType::Note,
            BlockType::Section,
            BlockType::Synopsis,
            BlockType::General,
            BlockType::PageBreak,
        ] {
            assert_eq!(BlockType::from_fdx(bt.to_fdx()), Some(bt));
        }
    }

    #[test]
    fn ids_are_unique() {
        let mut doc = ScreenplayDocument::blank("测试");
        for _ in 0..1000 {
            doc.blocks.push(ScreenplayBlock::new(BlockType::Action, "x"));
        }
        doc.rebuild_derived();
        assert!(doc.validate().is_ok());
    }

    #[test]
    fn scene_and_character_refs_valid_after_rebuild() {
        let mut doc = ScreenplayDocument::blank("测试");
        doc.blocks
            .push(ScreenplayBlock::new(BlockType::SceneHeading, "内. 酒肆 - 夜"));
        doc.blocks
            .push(ScreenplayBlock::new(BlockType::Character, "剑客"));
        doc.blocks
            .push(ScreenplayBlock::new(BlockType::Dialogue, "投军。"));
        doc.rebuild_derived();
        assert_eq!(doc.scenes.len(), 1);
        assert_eq!(doc.characters.len(), 1);
        assert!(doc.validate().is_ok());
        // dialogue 继承 character
        assert!(doc.blocks[2].character_id.is_some());
    }
}
