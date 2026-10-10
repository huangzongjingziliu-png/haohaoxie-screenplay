//! screenplay-agent —— Agent 运行时：模型 Provider 路由、Skill 注册、CommandService 事务执行。

pub mod provider;
pub mod runner;
pub mod skill;
pub mod tool;
pub mod summary;

pub use provider::{ChatMessage, ModelProvider, ModelRouter, OpenAiProvider};
pub use runner::{AgentError, AgentMode, AgentRunner, AgentToolOutcome};
pub use skill::{Skill, SkillRegistry, builtin_skills, parse_skill_md};
pub use tool::{ToolCall, ToolError, tool_definitions};

pub mod transform;
