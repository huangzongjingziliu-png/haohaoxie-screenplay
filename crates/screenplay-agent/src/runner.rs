//! AgentRunner —— 调度模型 + Skill + CommandService 的执行引擎。
//!
//! 说明书第 16 条：Agent 对剧本的所有修改都必须走 CommandService，
//! 且必须在事务中执行，失败整体回滚。

use crate::provider::{ChatMessage, ModelRouter};
use crate::skill::SkillRegistry;
use crate::tool::{self, ToolCall};
use anyhow::Result;
use screenplay_store::command::{CommandError, PendingOp};
use screenplay_store::db::Origin;
use screenplay_store::CommandService;
use std::sync::Arc;

/// Agent 执行模式（说明书 E-04：三模式）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentMode {
    /// 仅返回建议，不修改文档
    Suggest,
    /// 返回建议，不执行；确认后的应用必须重新检查 revision。
    SemiAuto,
    /// 自动应用
    Auto,
}

pub struct AgentRunner {
    pub service: Arc<CommandService>,
    pub router: ModelRouter,
    pub skills: SkillRegistry,
    /// 云 AI 是否启用（说明书 E-09：项目可禁用云 AI）
    pub cloud_ai_enabled: bool,
}

impl AgentRunner {
    pub fn new(
        service: Arc<CommandService>,
        router: ModelRouter,
        skills: SkillRegistry,
    ) -> Self {
        Self {
            service,
            router,
            skills,
            cloud_ai_enabled: true,
        }
    }

    /// 构造时指定云 AI 开关
    pub fn with_cloud_ai(mut self, enabled: bool) -> Self {
        self.cloud_ai_enabled = enabled;
        self
    }

    /// 对指定 block 应用某个 skill。
    /// 模型返回的文本整体替换该 block。
    pub async fn apply_skill(
        &self,
        skill_id: &str,
        block_id: &str,
        user_input: &str,
    ) -> Result<screenplay_store::CommandResult, AgentError> {
        if !self.cloud_ai_enabled && self.router.default_model != "ollama" {
            return Err(AgentError::CloudAiDisabled);
        }
        let snapshot = self.service.get_document();
        let expected_revision = snapshot.document.revision;
        let skill = self
            .skills
            .get(skill_id)
            .ok_or_else(|| AgentError::SkillNotFound(skill_id.to_string()))?;

        let block = snapshot.document.block(block_id)
            .ok_or_else(|| AgentError::Command(CommandError::NotFound { what: "block".into(), id: block_id.into() }))?;

        let context = format!("【当前 block】\n{}\n\n【用户补充】\n{}", block.text, user_input);
        let system = skill.render(&context);

        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: system,
            },
            ChatMessage {
                role: "user".into(),
                content: user_input.to_string(),
            },
        ];

        let response = self
            .router
            .chat(None, &messages, skill.temperature)
            .await
            .map_err(AgentError::Model)?;

        // 清理模型可能的多余解释
        let cleaned = clean_model_output(&response);

        // 通过事务替换 block（说明书第 16/61 条）
        let ops = vec![PendingOp::ReplaceBlock {
            block_id: block_id.to_string(),
            text: cleaned.clone(),
        }];
        let result = self
            .service
            .transaction(Origin::Agent, &format!("skill:{skill_id}"), ops, Some(expected_revision), |_doc| Ok(()))
            .map_err(AgentError::Command)?;

        Ok(result)
    }

    /// 自由对话：用户给出指令，模型返回对 block 的修改建议并应用。
    /// 这里取文档最后几个 block 作为上下文。
    pub async fn freeform_edit(
        &self,
        instruction: &str,
        context_block_count: usize,
    ) -> Result<screenplay_store::CommandResult, AgentError> {
        if !self.cloud_ai_enabled && self.router.default_model != "ollama" {
            return Err(AgentError::CloudAiDisabled);
        }
        let doc = self.service.get_document();
        let expected_revision = doc.document.revision;
        let blocks = &doc.document.blocks;
        let start = blocks.len().saturating_sub(context_block_count);
        let context: String = blocks[start..]
            .iter()
            .map(|b| format!("[{}] {}", b.block_type.as_str(), b.text))
            .collect::<Vec<_>>()
            .join("\n");

        let system = format!(
            "你是专业编剧助理。以下是当前剧本的最后若干 block：\n\n{context}\n\n\
             请根据用户指令，返回修改建议。格式：第一行写要修改的 block 索引（从 0 开始，相对最后 N 个），\n\
             第二行起写新的文本内容。只输出这两部分，不要解释。"
        );
        let messages = vec![
            ChatMessage { role: "system".into(), content: system },
            ChatMessage { role: "user".into(), content: instruction.to_string() },
        ];
        let response = self
            .router
            .chat(None, &messages, 0.7)
            .await
            .map_err(AgentError::Model)?;

        let (idx_str, new_text) = response
            .split_once('\n')
            .ok_or_else(|| AgentError::InvalidResponse("无法解析模型输出".into()))?;
        let rel_idx: usize = idx_str
            .trim()
            .parse()
            .map_err(|_| AgentError::InvalidResponse("block 索引不是数字".into()))?;
        let abs_idx = start + rel_idx;
        let target = blocks
            .get(abs_idx)
            .ok_or_else(|| AgentError::InvalidResponse("block 索引越界".into()))?;

        let cleaned = clean_model_output(new_text);
        let ops = vec![PendingOp::ReplaceBlock {
            block_id: target.id.clone(),
            text: cleaned,
        }];
        let result = self
            .service
            .transaction(Origin::Agent, "freeform_edit", ops, Some(expected_revision), |_doc| Ok(()))
            .map_err(AgentError::Command)?;
        Ok(result)
    }

    /// 结构化 Tool Calling（说明书 E-03）。
    ///
    /// 与 `freeform_edit` 的区别：不再让模型返回「索引+文本」的脆弱文本，
    /// 而是给模型一份工具菜单（replace_block / insert_block / delete_block /
    /// set_block_type），模型返回 JSON 工具调用，经 `validate_and_convert`
    /// 校验后在单个事务中执行。
    ///
    /// - `expected_revision`：模型调用期间用户可能编辑了文档，
    ///   若 revision 不匹配则拒绝（E-05：人工编辑不得被覆盖）。
    /// - `mode`：Suggest 只返回解析后的工具调用不执行；Auto 直接执行。
    /// - 云 AI 禁用时直接返回 `CloudAiDisabled`（E-09）。
    pub async fn run_with_tools(
        &self,
        instruction: &str,
        expected_revision: Option<u64>,
        mode: AgentMode,
        project_rules: &str,
    ) -> Result<AgentToolOutcome, AgentError> {
        if !self.cloud_ai_enabled {
            return Err(AgentError::CloudAiDisabled);
        }

        let doc_result = self.service.get_document();
        let doc = &doc_result.document;

        let expected_revision = Some(expected_revision.unwrap_or(doc.revision));
        let ctx = tool::render_document_context(doc);
        let rules = tool::render_project_rules(project_rules);
        let menu = tool::render_tool_menu();

        let system = format!(
            "{rules}你是专业编剧助理。{ctx}\n\n{menu}"
        );
        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: system,
            },
            ChatMessage {
                role: "user".into(),
                content: instruction.to_string(),
            },
        ];

        let response = self
            .router
            .chat(None, &messages, 0.0)
            .await
            .map_err(AgentError::Model)?;

        let calls = tool::parse_tool_calls(&response)
            .map_err(|e| AgentError::Tool(e))?;

        // 校验所有工具调用（任一失败则整体失败，不执行任何修改）
        let ops = tool::validate_all(&calls, doc)
            .map_err(|e| AgentError::Tool(e))?;

        if mode != AgentMode::Auto {
            return Ok(AgentToolOutcome::Suggested { calls });
        }

        // 执行：在单个事务中应用所有 ops，携带 expected_revision 做并发保护
        let result = self
            .service
            .transaction(
                Origin::Agent,
                "agent_tool_calls",
                ops,
                expected_revision,
                |_doc| Ok(()),
            )
            .map_err(AgentError::Command)?;

        Ok(AgentToolOutcome::Applied {
            calls,
            revision: result.revision,
        })
    }
}

/// `run_with_tools` 的返回值
#[derive(Debug)]
pub enum AgentToolOutcome {
    /// Suggest 模式：仅返回解析后的工具调用，未修改文档
    Suggested { calls: Vec<ToolCall> },
    /// 已应用到文档
    Applied { calls: Vec<ToolCall>, revision: u64 },
}

/// 清理模型输出：去掉首尾空白和代码围栏
fn clean_model_output(s: &str) -> String {
    let s = s.trim();
    let s = s
        .strip_prefix("```")
        .and_then(|s| s.strip_suffix("```"))
        .map(|s| s.trim())
        .unwrap_or(s);
    s.to_string()
}

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("未找到 skill: {0}")]
    SkillNotFound(String),
    #[error("模型调用失败: {0}")]
    Model(#[from] anyhow::Error),
    #[error("命令服务错误: {0}")]
    Command(#[from] CommandError),
    #[error("模型响应无法解析: {0}")]
    InvalidResponse(String),
    #[error("工具调用校验失败: {0}")]
    Tool(#[from] crate::tool::ToolError),
    #[error("云 AI 已被项目禁用")]
    CloudAiDisabled,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{ChatMessage, ModelProvider};
    use screenplay_core::ast::{BlockType, ScreenplayBlock};
    use screenplay_store::db::{open_db, Origin, Store};
    use screenplay_store::CommandService;
    use std::sync::Arc;

    struct EchoProvider;

    #[async_trait::async_trait]
    impl ModelProvider for EchoProvider {
        fn name(&self) -> &str {
            "echo"
        }
        async fn chat(&self, _messages: &[ChatMessage], _t: f32) -> Result<String> {
            Ok("ECHOED TEXT".to_string())
        }
    }

    fn make_runner() -> AgentRunner {
        let conn = open_db(std::path::Path::new(":memory:")).unwrap();
        let store = Arc::new(Store::new(conn, screenplay_core::ast::ScreenplayDocument::blank("t")));
        let service = Arc::new(CommandService::new(store));
        // 插入一个 block
        let r = service
            .insert_block(Origin::Cli, None, BlockType::Action, "hello", None)
            .unwrap();
        let bid = r.affected[0].clone();

        let mut router = ModelRouter::new("echo");
        router.register(Box::new(EchoProvider));
        let skills = SkillRegistry::new();
        AgentRunner::new(service, router, skills)
    }

    #[tokio::test]
    async fn skill_disabled_cloud_cannot_write() {
        let runner = make_runner().with_cloud_ai(false);
        let doc = runner.service.get_document().document;
        let result = runner.apply_skill("polish", &doc.blocks[0].id, "change").await;
        assert!(matches!(result, Err(AgentError::CloudAiDisabled)));
        assert_eq!(runner.service.get_document().document, doc);
    }

    struct ConcurrentEditProvider { service: Arc<CommandService>, block_id: String }
    #[async_trait::async_trait]
    impl ModelProvider for ConcurrentEditProvider {
        fn name(&self) -> &str { "concurrent" }
        async fn chat(&self, _: &[ChatMessage], _: f32) -> Result<String> {
            self.service.replace_block(Origin::GuiLeft, &self.block_id, "人的最新文字", None)?;
            Ok("过期模型结果".into())
        }
    }
    #[tokio::test]
    async fn skill_cannot_overwrite_edits_made_during_model_call() {
        let mut runner = make_runner();
        let id = runner.service.get_document().document.blocks[0].id.clone();
        let mut router = ModelRouter::new("concurrent");
        router.register(Box::new(ConcurrentEditProvider { service: runner.service.clone(), block_id: id.clone() }));
        runner.router = router;
        let result = runner.apply_skill("polish", &id, "change").await;
        assert!(matches!(result, Err(AgentError::Command(CommandError::RevisionConflict { .. }))));
        assert_eq!(runner.service.get_document().document.blocks[0].text, "人的最新文字");
    }

    #[tokio::test]
    async fn t01_apply_skill_replaces_block() {
        let runner = make_runner();
        let bid = runner.service.get_document().document.blocks[0].id.clone();
        let r = runner.apply_skill("polish", &bid, "make it better").await.unwrap();
        assert_eq!(r.document.blocks[0].text, "ECHOED TEXT");
    }

    #[tokio::test]
    async fn t02_skill_not_found() {
        let runner = make_runner();
        let bid = runner.service.get_document().document.blocks[0].id.clone();
        let err = runner.apply_skill("nope", &bid, "x").await.unwrap_err();
        assert!(matches!(err, AgentError::SkillNotFound(_)));
    }

    #[test]
    fn t03_clean_output_strips_fences() {
        assert_eq!(clean_model_output("```\nfoo\n```"), "foo");
        assert_eq!(clean_model_output("  bar  "), "bar");
    }

    // === run_with_tools 端到端测试 ===

    /// Mock provider：返回预设的工具调用 JSON（通过共享可变字符串设置）
    struct ToolCallProvider {
        response: Arc<std::sync::Mutex<String>>,
    }

    #[async_trait::async_trait]
    impl ModelProvider for ToolCallProvider {
        fn name(&self) -> &str {
            "tool-mock"
        }
        async fn chat(&self, _messages: &[ChatMessage], _t: f32) -> Result<String> {
            Ok(self.response.lock().unwrap().clone())
        }
    }

    fn make_runner_with_tool_response() -> (AgentRunner, Arc<std::sync::Mutex<String>>) {
        let conn = open_db(std::path::Path::new(":memory:")).unwrap();
        let store = Arc::new(Store::new(conn, screenplay_core::ast::ScreenplayDocument::blank("t")));
        let service = Arc::new(CommandService::new(store));
        service
            .insert_block(Origin::Cli, None, BlockType::Action, "hello", None)
            .unwrap();

        let response = Arc::new(std::sync::Mutex::new(String::new()));
        let mut router = ModelRouter::new("tool-mock");
        router.register(Box::new(ToolCallProvider {
            response: response.clone(),
        }));
        let skills = SkillRegistry::new();
        (AgentRunner::new(service, router, skills), response)
    }

    fn set_response(r: &Arc<std::sync::Mutex<String>>, s: &str) {
        *r.lock().unwrap() = s.to_string();
    }

    #[tokio::test]
    async fn t04_run_with_tools_applies_replace() {
        let (runner, resp) = make_runner_with_tool_response();
        let bid = runner.service.get_document().document.blocks[0].id.clone();
        set_response(
            &resp,
            &format!(
                r#"[{{"tool":"replace_block","args":{{"block_id":"{bid}","text":"replaced!"}}}}]"#
            ),
        );

        let outcome = runner
            .run_with_tools("do it", None, AgentMode::Auto, "")
            .await
            .unwrap();
        match outcome {
            AgentToolOutcome::Applied { calls, revision } => {
                assert_eq!(calls.len(), 1);
                assert!(revision >= 1);
            }
            _ => panic!("expected Applied"),
        }
        let doc = runner.service.get_document().document;
        assert_eq!(doc.blocks[0].text, "replaced!");
    }

    #[tokio::test]
    async fn t05_suggest_mode_does_not_modify() {
        let (runner, resp) = make_runner_with_tool_response();
        let bid = runner.service.get_document().document.blocks[0].id.clone();
        set_response(
            &resp,
            &format!(
                r#"[{{"tool":"replace_block","args":{{"block_id":"{bid}","text":"should not apply"}}}}]"#
            ),
        );
        let before = runner.service.get_document().document.blocks[0].text.clone();

        let outcome = runner
            .run_with_tools("do it", None, AgentMode::Suggest, "")
            .await
            .unwrap();
        assert!(matches!(outcome, AgentToolOutcome::Suggested { .. }));
        let after = runner.service.get_document().document.blocks[0].text.clone();
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn t06_invalid_block_rolls_back() {
        // 模型返回一个不存在的 block_id → 校验失败 → 文档不变
        let (runner, resp) = make_runner_with_tool_response();
        set_response(
            &resp,
            r#"[{"tool":"replace_block","args":{"block_id":"ghost","text":"x"}}]"#,
        );
        let before = runner.service.get_document().document.blocks[0].text.clone();

        let err = runner
            .run_with_tools("do it", None, AgentMode::Auto, "")
            .await
            .unwrap_err();
        assert!(matches!(err, AgentError::Tool(_)));
        let after = runner.service.get_document().document.blocks[0].text.clone();
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn semi_auto_returns_a_proposal_without_editing() {
        let (runner, response) = make_runner_with_tool_response();
        let before = runner.service.get_document().document;
        set_response(&response, &format!(r#"[{{"tool":"replace_block","args":{{"block_id":"{}","text":"new"}}}}]"#, before.blocks[0].id));
        assert!(matches!(runner.run_with_tools("edit", None, AgentMode::SemiAuto, "").await.unwrap(), AgentToolOutcome::Suggested { .. }));
        assert_eq!(runner.service.get_document().document, before);
    }

    struct EditingProvider {
        service: Arc<CommandService>,
        block_id: String,
    }

    #[async_trait::async_trait]
    impl ModelProvider for EditingProvider {
        fn name(&self) -> &str { "editing-provider" }
        async fn chat(&self, _: &[ChatMessage], _: f32) -> Result<String> {
            self.service.replace_block(Origin::Cli, &self.block_id, "human edit during model call", None)?;
            Ok(format!(r#"[{{"tool":"replace_block","args":{{"block_id":"{}","text":"stale AI text"}}}}]"#, self.block_id))
        }
    }

    #[tokio::test]
    async fn omitted_revision_still_protects_edits_during_model_call() {
        let (mut runner, _) = make_runner_with_tool_response();
        let block_id = runner.service.get_document().document.blocks[0].id.clone();
        let mut router = ModelRouter::new("editing-provider");
        router.register(Box::new(EditingProvider { service: runner.service.clone(), block_id: block_id.clone() }));
        runner.router = router;
        let error = runner.run_with_tools("edit", None, AgentMode::Auto, "").await.unwrap_err();
        assert!(matches!(error, AgentError::Command(CommandError::RevisionConflict { .. })));
        assert_eq!(runner.service.get_block(&block_id).unwrap().text, "human edit during model call");
    }

    #[tokio::test]
    async fn t07_revision_conflict_rejected() {
        let (runner, resp) = make_runner_with_tool_response();
        let bid = runner.service.get_document().document.blocks[0].id.clone();
        set_response(
            &resp,
            &format!(
                r#"[{{"tool":"replace_block","args":{{"block_id":"{bid}","text":"new"}}}}]"#
            ),
        );

        // 先做一次编辑使 revision 增长
        runner
            .service
            .replace_block(Origin::Cli, &bid, "manual edit", None)
            .unwrap();

        // expected_revision = 1（旧值），应被拒绝
        let err = runner
            .run_with_tools("do it", Some(1), AgentMode::Auto, "")
            .await
            .unwrap_err();
        assert!(matches!(err, AgentError::Command(CommandError::RevisionConflict { .. })));
        assert_eq!(
            runner.service.get_document().document.blocks[0].text,
            "manual edit"
        );
    }

    #[tokio::test]
    async fn t08_cloud_ai_disabled_blocked() {
        let (runner, _resp) = make_runner_with_tool_response();
        let runner = runner.with_cloud_ai(false);
        let err = runner
            .run_with_tools("do it", None, AgentMode::Auto, "")
            .await
            .unwrap_err();
        assert!(matches!(err, AgentError::CloudAiDisabled));
    }

    #[tokio::test]
    async fn t09_project_rules_injected() {
        let (runner, resp) = make_runner_with_tool_response();
        set_response(&resp, "[]");
        let outcome = runner
            .run_with_tools("x", None, AgentMode::Auto, "规则：不得使用脏话")
            .await
            .unwrap();
        assert!(matches!(outcome, AgentToolOutcome::Applied { .. }));
    }
}
