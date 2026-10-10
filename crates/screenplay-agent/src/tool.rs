//! 结构化 Tool Calling。
//!
//! 说明书第 16/61 条：Agent 对剧本的修改必须走 CommandService，且必须可校验、可回滚。
//!
//! 旧实现 `freeform_edit` 让模型返回「第一行写索引、后面写文本」，
//! 模型输出格式稍变就解析失败（脆弱）。本模块改用**结构化 Tool Calling**：
//!
//! 1. 每个 Tool 有名字、描述、JSON 参数 schema
//! 2. 模型返回 `{ "tool": "...", "args": {...} }` 形式的调用
//! 3. `validate()` 在执行前校验参数（block_id 是否存在、类型是否合法等）
//! 4. 校验通过后在单个 `transaction` 中执行，失败整体回滚
//!
//! 这样 Agent 的每一次修改都是**可审计、可校验、可回滚**的结构化操作，
//! 而不是对模型自由文本的脆弱解析。

use screenplay_core::ast::BlockType;
use screenplay_store::command::PendingOp;
use serde::{Deserialize, Serialize};

/// 一个工具调用的参数（已反序列化）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "tool", content = "args")]
pub enum ToolCall {
    #[serde(rename = "replace_block")]
    ReplaceBlock { block_id: String, text: String },
    #[serde(rename = "insert_block")]
    InsertBlock {
        after_id: Option<String>,
        block_type: String,
        text: String,
    },
    #[serde(rename = "delete_block")]
    DeleteBlock { block_id: String },
    #[serde(rename = "set_block_type")]
    SetBlockType { block_id: String, block_type: String },
    #[serde(rename = "merge_block")]
    MergeBlock { first_id: String, second_id: String },
}

/// 工具定义：供模型了解可用操作
#[derive(Debug, Clone, Serialize)]
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: serde_json::Value,
}

/// 所有内置工具的定义（发给模型看的「菜单」）
pub fn tool_definitions() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "replace_block",
            description: "替换指定 block 的文本内容",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "block_id": {"type": "string", "description": "要修改的 block ID"},
                    "text": {"type": "string", "description": "新的文本内容"}
                },
                "required": ["block_id", "text"]
            }),
        },
        ToolDef {
            name: "insert_block",
            description: "在指定位置插入一个新 block",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "after_id": {"type": ["string", "null"], "description": "插入到哪个 block 之后；为 null 时插到末尾"},
                    "block_type": {"type": "string", "enum": ["scene_heading", "action", "character", "parenthetical", "dialogue", "transition", "shot", "general"], "description": "block 类型"},
                    "text": {"type": "string", "description": "block 文本"}
                },
                "required": ["block_type", "text"]
            }),
        },
        ToolDef {
            name: "delete_block",
            description: "删除指定 block",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "block_id": {"type": "string", "description": "要删除的 block ID"}
                },
                "required": ["block_id"]
            }),
        },
        ToolDef {
            name: "set_block_type",
            description: "修改指定 block 的类型（如 action → dialogue）",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "block_id": {"type": "string", "description": "要修改的 block ID"},
                    "block_type": {"type": "string", "enum": ["scene_heading", "action", "character", "parenthetical", "dialogue", "transition", "shot", "general"]}
                },
                "required": ["block_id", "block_type"]
            }),
        },
        ToolDef {
            name: "merge_block",
            description: "将两个相邻 block 合并（第二个并入第一个，文本拼接）",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "first_id": {"type": "string", "description": "第一个 block ID（保留）"},
                    "second_id": {"type": "string", "description": "第二个 block ID（被合并，必须紧跟第一个之后）"}
                },
                "required": ["first_id", "second_id"]
            }),
        },
    ]
}

/// 把工具定义渲染成给模型看的文本说明
pub fn render_tool_menu() -> String {
    let mut s = String::from("你可以使用以下工具来修改剧本（返回 JSON 数组，每个元素是一个工具调用）：\n\n");
    for t in tool_definitions() {
        s.push_str(&format!(
            "- {}: {}\n  参数: {}\n",
            t.name,
            t.description,
            serde_json::to_string_pretty(&t.parameters).unwrap_or_default()
        ));
    }
    s.push_str("\n返回格式示例：\n[{\"tool\": \"replace_block\", \"args\": {\"block_id\": \"abc\", \"text\": \"新内容\"}}]\n");
    s.push_str("只返回 JSON，不要解释。");
    s
}

/// 解析模型返回的工具调用列表
pub fn parse_tool_calls(raw: &str) -> Result<Vec<ToolCall>, ToolError> {
    let raw = raw.trim();
    // 去掉可能的代码围栏
    let raw = raw
        .strip_prefix("```json")
        .or_else(|| raw.strip_prefix("```"))
        .unwrap_or(raw);
    let raw = raw.strip_suffix("```").unwrap_or(raw).trim();

    let calls: Vec<ToolCall> = serde_json::from_str(raw)
        .map_err(|e| ToolError::Parse(e.to_string()))?;
    Ok(calls)
}

/// 把字符串 block_type 转成枚举
pub fn parse_block_type(s: &str) -> Result<BlockType, ToolError> {
    match s {
        "scene_heading" => Ok(BlockType::SceneHeading),
        "action" => Ok(BlockType::Action),
        "character" => Ok(BlockType::Character),
        "parenthetical" => Ok(BlockType::Parenthetical),
        "dialogue" => Ok(BlockType::Dialogue),
        "transition" => Ok(BlockType::Transition),
        "shot" => Ok(BlockType::Shot),
        "general" => Ok(BlockType::General),
        other => Err(ToolError::InvalidBlockType(other.to_string())),
    }
}

/// 校验一个工具调用并转成 PendingOp。
///
/// 校验内容：
/// - block_id 非空
/// - text 非空（replace/insert）
/// - block_type 合法
/// - block_id 存在于文档（replace/delete/set_type）
pub fn validate_and_convert(
    call: &ToolCall,
    doc: &screenplay_core::ast::ScreenplayDocument,
) -> Result<PendingOp, ToolError> {
    match call {
        ToolCall::ReplaceBlock { block_id, text } => {
            if block_id.is_empty() {
                return Err(ToolError::Validation("block_id 不能为空".into()));
            }
            if text.is_empty() {
                return Err(ToolError::Validation("text 不能为空".into()));
            }
            if doc.block(block_id).is_none() {
                return Err(ToolError::BlockNotFound(block_id.clone()));
            }
            Ok(PendingOp::ReplaceBlock {
                block_id: block_id.clone(),
                text: text.clone(),
            })
        }
        ToolCall::InsertBlock {
            after_id,
            block_type,
            text,
        } => {
            if text.is_empty() {
                return Err(ToolError::Validation("text 不能为空".into()));
            }
            let bt = parse_block_type(block_type)?;
            // after_id 若指定则必须存在
            if let Some(aid) = after_id {
                if !aid.is_empty() && doc.block(aid).is_none() {
                    return Err(ToolError::BlockNotFound(aid.clone()));
                }
            }
            Ok(PendingOp::InsertBlock {
                block_id: format!("agent-{}", uuid::Uuid::new_v4()),
                after_id: after_id.clone(),
                block_type: bt,
                text: text.clone(),
            })
        }
        ToolCall::DeleteBlock { block_id } => {
            if block_id.is_empty() {
                return Err(ToolError::Validation("block_id 不能为空".into()));
            }
            if doc.block(block_id).is_none() {
                return Err(ToolError::BlockNotFound(block_id.clone()));
            }
            Ok(PendingOp::DeleteBlock {
                block_id: block_id.clone(),
            })
        }
        ToolCall::SetBlockType {
            block_id,
            block_type,
        } => {
            if block_id.is_empty() {
                return Err(ToolError::Validation("block_id 不能为空".into()));
            }
            if doc.block(block_id).is_none() {
                return Err(ToolError::BlockNotFound(block_id.clone()));
            }
            let bt = parse_block_type(block_type)?;
            Ok(PendingOp::SetBlockType {
                block_id: block_id.clone(),
                block_type: bt,
            })
        }
        ToolCall::MergeBlock { first_id, second_id } => {
            if first_id.is_empty() || second_id.is_empty() {
                return Err(ToolError::Validation("first_id 和 second_id 不能为空".into()));
            }
            if first_id == second_id {
                return Err(ToolError::Validation("不能与自身合并".into()));
            }
            if doc.block(first_id).is_none() {
                return Err(ToolError::BlockNotFound(first_id.clone()));
            }
            if doc.block(second_id).is_none() {
                return Err(ToolError::BlockNotFound(second_id.clone()));
            }
            Ok(PendingOp::MergeBlocks {
                first_id: first_id.clone(),
                second_id: second_id.clone(),
            })
        }
    }
}

/// 校验并转换一批工具调用。任一失败则整体失败（事务原子性）。
pub fn validate_all(
    calls: &[ToolCall],
    doc: &screenplay_core::ast::ScreenplayDocument,
) -> Result<Vec<PendingOp>, ToolError> {
    calls
        .iter()
        .map(|c| validate_and_convert(c, doc))
        .collect()
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("解析模型工具调用失败: {0}")]
    Parse(String),
    #[error("校验失败: {0}")]
    Validation(String),
    #[error("block 不存在: {0}")]
    BlockNotFound(String),
    #[error("非法 block_type: {0}")]
    InvalidBlockType(String),
}

/// 把文档的 block 列表渲染成给模型的上下文（id + 类型 + 文本预览）
pub fn render_document_context(doc: &screenplay_core::ast::ScreenplayDocument) -> String {
    let mut s = String::from("当前剧本 blocks：\n");
    for b in &doc.blocks {
        let preview: String = b.text.chars().take(60).collect();
        s.push_str(&format!(
            "- id={} type={} text={:?}\n",
            b.id,
            b.block_type.as_str(),
            preview
        ));
    }
    s
}

/// 渲染项目规则（PROJECT_RULES）给模型作为约束
pub fn render_project_rules(rules: &str) -> String {
    if rules.trim().is_empty() {
        return String::new();
    }
    format!("项目规则（必须遵守）：\n{rules}\n\n")
}

/// 用于测试的 mock：把工具调用序列化再反序列化，验证 round-trip
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t01_parse_replace_block() {
        let raw = r#"[{"tool":"replace_block","args":{"block_id":"b1","text":"hi"}}]"#;
        let calls = parse_tool_calls(raw).unwrap();
        assert_eq!(calls.len(), 1);
        match &calls[0] {
            ToolCall::ReplaceBlock { block_id, text } => {
                assert_eq!(block_id, "b1");
                assert_eq!(text, "hi");
            }
            _ => panic!("expected ReplaceBlock"),
        }
    }

    #[test]
    fn t02_parse_with_fences() {
        let raw = "```json\n[{\"tool\":\"delete_block\",\"args\":{\"block_id\":\"b1\"}}]\n```";
        let calls = parse_tool_calls(raw).unwrap();
        assert!(matches!(calls[0], ToolCall::DeleteBlock { .. }));
    }

    #[test]
    fn t03_invalid_block_type_rejected() {
        let err = parse_block_type("nope").unwrap_err();
        assert!(matches!(err, ToolError::InvalidBlockType(_)));
    }

    #[test]
    fn t04_validate_rejects_missing_block() {
        let doc = screenplay_core::ast::ScreenplayDocument::blank("t");
        let call = ToolCall::ReplaceBlock {
            block_id: "ghost".into(),
            text: "x".into(),
        };
        let err = validate_and_convert(&call, &doc).unwrap_err();
        assert!(matches!(err, ToolError::BlockNotFound(_)));
    }

    #[test]
    fn t05_validate_rejects_empty_text() {
        let doc = screenplay_core::ast::ScreenplayDocument::blank("t");
        let call = ToolCall::ReplaceBlock {
            block_id: "x".into(),
            text: "".into(),
        };
        let err = validate_and_convert(&call, &doc).unwrap_err();
        assert!(matches!(err, ToolError::Validation(_)));
    }

    #[test]
    fn t06_validate_insert_generates_uuid() {
        let doc = screenplay_core::ast::ScreenplayDocument::blank("t");
        let call = ToolCall::InsertBlock {
            after_id: None,
            block_type: "action".into(),
            text: "hello".into(),
        };
        let op = validate_and_convert(&call, &doc).unwrap();
        match op {
            PendingOp::InsertBlock { block_id, .. } => {
                assert!(block_id.starts_with("agent-"));
            }
            _ => panic!("expected InsertBlock"),
        }
    }

    #[test]
    fn t07_tool_menu_contains_all_four() {
        let menu = render_tool_menu();
        assert!(menu.contains("replace_block"));
        assert!(menu.contains("insert_block"));
        assert!(menu.contains("delete_block"));
        assert!(menu.contains("set_block_type"));
    }

    #[test]
    fn t08_validate_all_atomic() {
        // 一个调用失败则整体失败
        let doc = screenplay_core::ast::ScreenplayDocument::blank("t");
        let calls = vec![
            ToolCall::InsertBlock {
                after_id: None,
                block_type: "action".into(),
                text: "ok".into(),
            },
            ToolCall::ReplaceBlock {
                block_id: "ghost".into(),
                text: "x".into(),
            },
        ];
        let err = validate_all(&calls, &doc).unwrap_err();
        assert!(matches!(err, ToolError::BlockNotFound(_)));
    }
}
