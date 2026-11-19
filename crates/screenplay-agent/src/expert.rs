//! 专家（Expert）体系：带明确方法论的审稿者 / 协作写作者。
//!
//! ## Skill 与 Expert 的分工
//!
//! - Skill：一次性的写作动作（续写、润色、扩写…），输入片段、输出片段。
//! - Expert：稳定的人格与方法论，面向「场景/整稿 → 判断与修改方案」，
//!   也能作为对话中的协作写作者视角。
//!
//! ## 成本策略
//!
//! 多专家会审默认串行：每位专家各调用一次模型，最后再由「会审主持」
//! 汇总一次，避免并发把费用放大到不可预期。调用次数 = 专家数 + 1。
//!
//! 自定义专家放在 `~/Library/Application Support/ScreenplayStudio/experts/<id>/EXPERT.md`，
//! frontmatter 支持 id / name / description / dimensions / temperature / max_tokens。

use crate::provider::{ChatMessage, ModelRouter};
use serde::Serialize;
use std::path::Path;
use thiserror::Error;

/// 一个专家定义。
#[derive(Debug, Clone)]
pub struct Expert {
    pub id: String,
    pub name: String,
    pub description: String,
    /// 评审维度（会写进提示词，也是界面上的标签）
    pub dimensions: Vec<String>,
    /// 输出结构（Markdown 小节标题）
    pub output_sections: Vec<String>,
    /// 方法论人格提示词
    pub system_prompt: String,
    pub temperature: f32,
    pub max_tokens: u32,
    /// 内置专家不可被同名自定义专家覆盖
    pub builtin: bool,
}

impl Expert {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        dimensions: &[&str],
        system_prompt: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            dimensions: dimensions.iter().map(|d| d.to_string()).collect(),
            output_sections: default_sections(),
            system_prompt: system_prompt.into(),
            temperature: 0.4,
            max_tokens: 2400,
            builtin: false,
        }
    }

    pub fn builtin(mut self) -> Self {
        self.builtin = true;
        self
    }

    /// 专家在对话中的人格前缀。
    pub fn persona(&self) -> String {
        format!(
            "{}\n\n你的评审维度：{}。\n你的输出结构：{}。\n你只能提出建议，不能声称已经修改或保存稿件。",
            self.system_prompt,
            self.dimensions.join("、"),
            self.output_sections.join("、")
        )
    }
}

fn default_sections() -> Vec<String> {
    vec![
        "总体判断".into(),
        "关键问题".into(),
        "修改建议".into(),
        "需要作者确认".into(),
    ]
}

/// 内置专家：用方法论命名，不绑定任何真人。
pub fn builtin_experts() -> Vec<Expert> {
    vec![
        Expert::new(
            "beat-sheet",
            "节拍表专家",
            "用节拍表检查故事推进与情绪曲线，指出漏拍、错拍和节奏塌陷",
            &[
                "开场画面与主题",
                "催化剂与拒绝",
                "中点与伪胜利/伪失败",
                "全盘皆输与灵魂黑夜",
                "终局与终场画面",
            ],
            "你以节拍表方法工作。你相信节拍必须由「事件」承担，而不是由人物解释或旁白宣布。\
             你的判断标准只有一条：观众是否在正确的时间点产生期待、意外与满足。\
             你尤其警惕三种病：开头铺垫过长、中点没有改变主角处境、第三幕靠巧合收尾。\
             发现漏拍时，你要给出「补一个什么事件、放在哪一场、替代哪段解释」的具体方案，而不是泛泛建议加强冲突。",
        )
        .builtin(),
        Expert::new(
            "three-act",
            "三幕结构专家",
            "检查幕结构、情节点与戏剧问题的闭合，判断故事是否站得住",
            &[
                "第一幕建置",
                "情节点一与进入对抗",
                "中点转向",
                "情节点二与最低点",
                "第三幕解决",
            ],
            "你以三幕结构方法工作。你关注的是因果链：每一幕的转折是否改变了主角的目标或代价。\
             你的判断标准：中点必须让主角从被动变主动（或反之），情节点二必须把退路堵死，\
             结局必须回答开场提出的戏剧问题。你反对用情绪强度冒充结构推进。\
             你会明确指出「哪一幕缺转折、缺的是目标变化还是代价变化」，并给出可执行的替代事件。",
        )
        .builtin(),
        Expert::new(
            "character-arc",
            "人物弧光专家",
            "追踪欲望、缺陷与转变，检查人物是否在压力下做出可见的选择",
            &[
                "想要与需要",
                "缺陷与自我欺骗",
                "压力下的选择",
                "关系镜像",
                "转变的可见证据",
            ],
            "你以人物弧光方法工作。你只承认由「选择与代价」证明的转变，不承认台词宣布的成长。\
             你的判断标准：开场与结尾，主角面对相似压力时是否做出不同的选择；配角是否拥有自己的欲望，\
             而不只是主角的帮手或障碍。你会指出弧光断裂的具体场景，并要求用行动、代价或关系变化来补证据。",
        )
        .builtin(),
        Expert::new(
            "scene-craft",
            "场景与冲突专家",
            "逐场检查目标、障碍与转折，判断每场戏是否必要、是否改变了力量关系",
            &[
                "场景目标",
                "障碍与冲突",
                "进入与退出点",
                "信息增量",
                "场景必要性",
            ],
            "你以场景工程方法工作。你要求每场戏都有人想要某样东西并遭到抵抗，\
             场景结束时力量关系必须发生变化，否则这场戏就只是信息搬运。\
             你会明确指出可以删除、合并或后置的场景，并说明删除后损失了什么、如何补回。\
             你反对用「气氛」为没有冲突的场景辩护，但承认必要的呼吸场，前提是它承担了人物或主题功能。",
        )
        .builtin(),
        Expert::new(
            "dialogue-voice",
            "对白与声口专家",
            "检查人物区分度、潜台词与可演性，删掉解释性台词",
            &[
                "人物区分度",
                "潜台词",
                "信息密度",
                "可演性",
                "节奏与停顿",
            ],
            "你以对白方法工作。你判断每句台词是否至少完成一件事：推进、暴露或施压。\
             你最反感人物互相解释彼此已经知道的事，以及用形容词代替可演的动作。\
             你的判断标准：遮住人物名，读者能否分辨是谁在说话；演员能否从台词里找到动作与停顿。\
             你会给出具体改写示范，但保留原意与人物事实，不新增情节。",
        )
        .builtin(),
        Expert::new(
            "series-pacing",
            "短剧节奏专家",
            "按集检查钩子、冲突升级与卡点，判断单集是否留得住观众",
            &[
                "前三十秒钩子",
                "冲突升级频率",
                "反转与卡点",
                "集尾悬念",
                "单集信息量",
            ],
            "你以竖屏短剧的节奏方法工作。你要求每集在前三十秒建立欲望与阻力，\
             至少有一次明确的力量反转，集尾留下未完成的问题。\
             你的判断标准：观众在任意一分钟划走，是否还有理由回来。\
             你会指出「哪一段在空转、哪一次反转为反转而反转、集尾钩子是否靠下一集解释才成立」，\
             并给出压缩、前置或改卡点的具体方案。",
        )
        .builtin(),
    ]
}

/// 专家评审模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpertMode {
    /// 评审：给判断、问题与修改建议
    Review,
    /// 改写建议：给原文 → 建议改写的对照方案
    Rewrite,
}

impl ExpertMode {
    pub fn parse(value: &str) -> Self {
        match value {
            "rewrite" => Self::Rewrite,
            _ => Self::Review,
        }
    }
}

/// 单个专家的返回。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpertReply {
    pub expert_id: String,
    pub name: String,
    pub ok: bool,
    pub text: String,
    pub error: Option<String>,
}

/// 会审结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpertPanel {
    pub reviews: Vec<ExpertReply>,
    pub synthesis: Option<String>,
    pub synthesis_error: Option<String>,
}

/// 给单个专家的消息。
pub fn review_messages(
    expert: &Expert,
    context: &str,
    memory: &str,
    mode: ExpertMode,
    instruction: Option<&str>,
) -> Vec<ChatMessage> {
    let task = match mode {
        ExpertMode::Review => {
            "请按你的方法论评审下面的剧本。只评审给定文本，引用场景号或原句作为证据，不虚构剧情。\
             区分事实问题、判断问题与口味问题；口味问题必须标明。每条建议说明改哪里、怎么改、为什么。"
        }
        ExpertMode::Rewrite => {
            "请按你的方法论给出改写方案：逐场列出「原文 → 建议改写」，保留人物与情节事实，\
             说明改动依据。不要输出整稿，不要新增未发生的剧情。只评审给定文本并引用原句。"
        }
    };
    let extra = instruction.map(str::trim).filter(|s| !s.is_empty()).unwrap_or("");
    let user = format!(
        "{task}\n\n{}\n输出结构（Markdown 小节标题，可增补但不要省略）：\n{}\n\n{}项目约束与记忆：\n{}\n\n待评审文本：\n{}",
        expert.description,
        expert
            .output_sections
            .iter()
            .map(|s| format!("## {s}"))
            .collect::<Vec<_>>()
            .join("\n"),
        if extra.is_empty() {
            String::new()
        } else {
            format!("作者补充要求：{extra}\n\n")
        },
        if memory.trim().is_empty() {
            "（无）"
        } else {
            memory
        },
        context
    );
    vec![
        ChatMessage { role: "system".into(), content: expert.persona() },
        ChatMessage { role: "user".into(), content: user },
    ]
}

/// 会审主持的汇总消息。
pub fn synthesis_messages(replies: &[ExpertReply], mode: ExpertMode) -> Vec<ChatMessage> {
    let joined = replies
        .iter()
        .filter(|r| r.ok)
        .map(|r| format!("### {}\n{}", r.name, r.text.trim()))
        .collect::<Vec<_>>()
        .join("\n\n");
    let mode_hint = match mode {
        ExpertMode::Review => "这是一次评审会审。",
        ExpertMode::Rewrite => "这是一次改写方案会审。",
    };
    let user = format!(
        "{mode_hint}下面是多位专家对同一份剧本的独立意见。请汇总，不要新增专家没有提出的问题，不要抹平分歧。\n\
         输出结构：\n## 共识\n（2-5 条，注明依据来自哪些专家）\n\
         ## 分歧\n（逐条列出冲突点与各方立场；没有分歧就写「无实质分歧」）\n\
         ## 修改优先级\n（P0 必改 / P1 建议 / P2 可选，每条注明依据专家）\n\
         ## 一句话结论\n\n专家意见：\n{joined}"
    );
    vec![
        ChatMessage {
            role: "system".into(),
            content: "你是剧本会审主持。你只做归纳与对比，不替专家补充新观点，也不裁决审美口味。".into(),
        },
        ChatMessage { role: "user".into(), content: user },
    ]
}

/// 依次执行多专家会审，再做一次汇总。
///
/// - 串行执行，避免并发放大成本；
/// - 单个专家失败不影响其他专家；
/// - 少于两位成功时不生成汇总。
pub async fn run_panel(
    router: &ModelRouter,
    experts: &[Expert],
    context: &str,
    memory: &str,
    mode: ExpertMode,
    instruction: Option<&str>,
) -> ExpertPanel {
    let mut reviews = Vec::new();
    for expert in experts.iter().take(4) {
        let messages = review_messages(expert, context, memory, mode, instruction);
        let reply = match router
            .chat_with_max_tokens(None, &messages, expert.temperature, expert.max_tokens)
            .await
        {
            Ok(text) if !text.trim().is_empty() => ExpertReply {
                expert_id: expert.id.clone(),
                name: expert.name.clone(),
                ok: true,
                text,
                error: None,
            },
            Ok(_) => ExpertReply {
                expert_id: expert.id.clone(),
                name: expert.name.clone(),
                ok: false,
                text: String::new(),
                error: Some("模型返回空结果".into()),
            },
            Err(error) => ExpertReply {
                expert_id: expert.id.clone(),
                name: expert.name.clone(),
                ok: false,
                text: String::new(),
                error: Some(friendly_error(&error.to_string())),
            },
        };
        reviews.push(reply);
    }
    let successes: Vec<&ExpertReply> = reviews.iter().filter(|r| r.ok).collect();
    if successes.len() < 2 {
        // 选了多位专家却没能凑够两位时，说清为什么没有综合意见。
        let synthesis_error = if experts.len() > 1 {
            Some(if successes.is_empty() {
                "没有专家成功返回结果，因此没有综合意见。请检查模型设置后重试。".into()
            } else {
                "只有 1 位专家成功返回，至少需要 2 位才能比较汇总，因此没有综合意见。".into()
            })
        } else {
            None
        };
        return ExpertPanel { reviews, synthesis: None, synthesis_error };
    }
    let messages = synthesis_messages(&reviews, mode);
    match router.chat(None, &messages, 0.3).await {
        Ok(text) if !text.trim().is_empty() => {
            ExpertPanel { reviews, synthesis: Some(text), synthesis_error: None }
        }
        Ok(_) => ExpertPanel {
            reviews,
            synthesis: None,
            synthesis_error: Some("汇总返回空结果".into()),
        },
        Err(error) => ExpertPanel {
            reviews,
            synthesis: None,
            synthesis_error: Some(friendly_error(&error.to_string())),
        },
    }
}

/// 把 provider 原始错误收敛成用户可读的提示，避免把端点、模型名带进界面。
fn friendly_error(raw: &str) -> String {
    let lower = raw.to_lowercase();
    if lower.contains("timeout") || lower.contains("timed out") {
        "请求超时，请稍后重试。".into()
    } else if lower.contains("401") || lower.contains("403") || lower.contains("unauthorized") || lower.contains("api key") {
        "鉴权失败，请检查设置里的 API Key。".into()
    } else if lower.contains("404") {
        "接口地址或模型名不可用，请检查模型设置。".into()
    } else if lower.contains("429") {
        "请求过于频繁或额度不足，请稍后再试。".into()
    } else {
        "模型调用失败，请检查模型设置后重试。".into()
    }
}

/// 解析 EXPERT.md（frontmatter + 正文人格提示词）。
pub fn parse_expert_md(content: &str) -> Result<Expert, ExpertLoadError> {
    let (frontmatter, body) = crate::skill::split_frontmatter(content)
        .map_err(|e| ExpertLoadError::Format(e.to_string()))?;
    let mut id = String::new();
    let mut name = String::new();
    let mut description = String::new();
    let mut dimensions = Vec::new();
    let mut temperature = 0.4f32;
    let mut max_tokens = 2400u32;
    for line in frontmatter.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else { continue };
        let value = value.trim().trim_matches('"').trim_matches('\'');
        match key.trim() {
            "id" => id = value.to_string(),
            "name" => name = value.to_string(),
            "description" => description = value.to_string(),
            "dimensions" => {
                dimensions = value
                    .split([',', '，', '、'])
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            }
            "temperature" => {
                temperature = value
                    .parse::<f32>()
                    .map_err(|_| ExpertLoadError::InvalidField("temperature".into()))?
            }
            "max_tokens" => {
                max_tokens = value
                    .parse::<u32>()
                    .map_err(|_| ExpertLoadError::InvalidField("max_tokens".into()))?
            }
            _ => {}
        }
    }
    if id.is_empty() {
        return Err(ExpertLoadError::MissingField("id".into()));
    }
    if !crate::skill::is_valid_skill_id(&id) {
        return Err(ExpertLoadError::InvalidField("id 必须是安全的目录名".into()));
    }
    let prompt = body.trim();
    if prompt.is_empty() {
        return Err(ExpertLoadError::MissingField("正文人格提示词".into()));
    }
    if !temperature.is_finite() || !(0.0..=2.0).contains(&temperature) {
        return Err(ExpertLoadError::InvalidField("temperature".into()));
    }
    if max_tokens == 0 || max_tokens > 16000 {
        return Err(ExpertLoadError::InvalidField("max_tokens".into()));
    }
    Ok(Expert {
        name: if name.is_empty() { id.clone() } else { name },
        description,
        dimensions: if dimensions.is_empty() {
            vec!["总体判断".into(), "关键问题".into(), "修改建议".into()]
        } else {
            dimensions
        },
        output_sections: default_sections(),
        system_prompt: prompt.to_string(),
        temperature,
        max_tokens,
        builtin: false,
        id,
    })
}

/// 内置专家 + 用户目录专家（同名时保留内置）。
pub fn load_experts(dir: Option<&Path>) -> Vec<Expert> {
    load_experts_detailed(dir).0
}

/// 同 [`load_experts`]，同时返回被跳过的文件及原因，供界面提示用户修正。
pub fn load_experts_detailed(dir: Option<&Path>) -> (Vec<Expert>, Vec<String>) {
    let mut experts = builtin_experts();
    let mut issues = Vec::new();
    let Some(dir) = dir else { return (experts, issues) };
    let Ok(entries) = std::fs::read_dir(dir) else { return (experts, issues) };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let file = path.join("EXPERT.md");
        let Ok(content) = std::fs::read_to_string(&file) else { continue };
        match parse_expert_md(&content) {
            Ok(expert) => {
                if experts.iter().any(|e| e.id == expert.id) {
                    issues.push(format!(
                        "{}：id「{}」与已有专家重复，已忽略。",
                        path.display(),
                        expert.id
                    ));
                } else {
                    experts.push(expert);
                }
            }
            Err(error) => issues.push(format!("{}：{error}", file.display())),
        }
    }
    (experts, issues)
}

#[derive(Debug, Error)]
pub enum ExpertLoadError {
    #[error("格式错误: {0}")]
    Format(String),
    #[error("缺少字段: {0}")]
    MissingField(String),
    #[error("字段非法: {0}")]
    InvalidField(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use anyhow::Result;
    use std::sync::Mutex;

    struct MockProvider {
        seen: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        reply: String,
    }

    #[async_trait]
    impl crate::provider::ModelProvider for MockProvider {
        fn name(&self) -> &str {
            "mock"
        }
        async fn chat(&self, messages: &[ChatMessage], _temperature: f32) -> Result<String> {
            let first = messages.first().map(|m| m.content.clone()).unwrap_or_default();
            self.seen.lock().unwrap().push(first);
            Ok(self.reply.clone())
        }
    }

    #[test]
    fn builtin_experts_are_unique_and_method_based() {
        let experts = builtin_experts();
        assert!(experts.len() >= 5);
        let mut ids: Vec<&str> = experts.iter().map(|e| e.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), experts.len());
        for expert in &experts {
            assert!(expert.builtin);
            assert!(!expert.dimensions.is_empty());
            assert!(!expert.system_prompt.is_empty());
        }
    }

    #[test]
    fn custom_expert_parses_and_cannot_override_builtin() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("my-expert");
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(
            dir.join("EXPERT.md"),
            "---\nid: my-expert\nname: 我的专家\ndescription: 自定义\n\
             dimensions: 结构，节奏、人物\ntemperature: 0.5\nmax_tokens: 2000\n---\n\
             你只关心一件事：这场戏有没有让主角付出代价。",
        )
        .unwrap();
        let experts = load_experts(Some(tmp.path()));
        let custom = experts.iter().find(|e| e.id == "my-expert").unwrap();
        assert_eq!(custom.name, "我的专家");
        assert_eq!(custom.dimensions.len(), 3);
        assert!(!custom.builtin);

        let override_dir = tmp.path().join("beat-sheet");
        std::fs::create_dir(&override_dir).unwrap();
        std::fs::write(
            override_dir.join("EXPERT.md"),
            "---\nid: beat-sheet\nname: 冒名\n---\n覆盖内置",
        )
        .unwrap();
        let experts = load_experts(Some(tmp.path()));
        assert!(experts.iter().find(|e| e.id == "beat-sheet").unwrap().builtin);
    }

    #[test]
    fn review_prompt_contains_dimensions_and_evidence_rules() {
        let expert = &builtin_experts()[0];
        let messages = review_messages(expert, "场景1 内容", "项目记忆", ExpertMode::Review, Some("只看前五场"));
        assert_eq!(messages.len(), 2);
        assert!(messages[0].content.contains(&expert.dimensions[0]));
        assert!(messages[1].content.contains("场景1 内容"));
        assert!(messages[1].content.contains("只看前五场"));
        assert!(messages[1].content.contains("## 总体判断"));
        assert!(messages[1].content.contains("不虚构剧情"));
    }

    #[tokio::test]
    async fn panel_runs_sequentially_then_synthesizes() {
        let experts = builtin_experts().into_iter().take(3).collect::<Vec<_>>();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let provider = MockProvider { seen: seen.clone(), reply: "## 总体判断\n可以".into() };
        let mut router = ModelRouter::new("mock");
        router.register(Box::new(provider));
        let panel = run_panel(&router, &experts, "正文", "", ExpertMode::Review, None).await;
        assert_eq!(panel.reviews.len(), 3);
        assert!(panel.reviews.iter().all(|r| r.ok));
        assert!(panel.synthesis.is_some());
        // 3 位专家 + 1 次汇总 = 4 次串行调用
        assert_eq!(seen.lock().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn panel_skips_synthesis_when_only_one_succeeds() {
        struct OnceProvider(Mutex<usize>);
        #[async_trait]
        impl crate::provider::ModelProvider for OnceProvider {
            fn name(&self) -> &str {
                "once"
            }
            async fn chat(&self, _messages: &[ChatMessage], _temperature: f32) -> Result<String> {
                let mut count = self.0.lock().unwrap();
                *count += 1;
                if *count == 1 {
                    Ok("ok".into())
                } else {
                    anyhow::bail!("模型错误")
                }
            }
        }
        let experts = builtin_experts().into_iter().take(2).collect::<Vec<_>>();
        let mut router = ModelRouter::new("once");
        router.register(Box::new(OnceProvider(Mutex::new(0))));
        let panel = run_panel(&router, &experts, "正文", "", ExpertMode::Rewrite, None).await;
        assert_eq!(panel.reviews.iter().filter(|r| r.ok).count(), 1);
        assert_eq!(panel.reviews.iter().filter(|r| !r.ok).count(), 1);
        assert!(panel.synthesis.is_none());
        let reason = panel.synthesis_error.expect("应说明为何没有综合意见");
        assert!(reason.contains("至少需要 2 位"), "实际: {reason}");
        // 失败信息不应带出 provider 原始错误（可能含端点或模型名）
        assert_eq!(panel.reviews.iter().filter(|r| !r.ok).next().unwrap().error.as_deref(), Some("模型调用失败，请检查模型设置后重试。"));
    }

    #[tokio::test]
    async fn expert_max_tokens_reaches_provider() {
        struct LimitProvider(std::sync::Arc<Mutex<Vec<u32>>>);
        #[async_trait]
        impl crate::provider::ModelProvider for LimitProvider {
            fn name(&self) -> &str { "limit" }
            async fn chat(&self, _messages: &[ChatMessage], _temperature: f32) -> Result<String> {
                Ok("## 共识\nok".into())
            }
            async fn chat_limited(&self, _messages: &[ChatMessage], _temperature: f32, max_tokens: u32) -> Result<String> {
                self.0.lock().unwrap().push(max_tokens);
                Ok("## 总体判断\nok".into())
            }
        }
        let limits = std::sync::Arc::new(Mutex::new(Vec::new()));
        let mut router = ModelRouter::new("limit");
        router.register(Box::new(LimitProvider(limits.clone())));
        let experts = builtin_experts().into_iter().take(2).collect::<Vec<_>>();
        let panel = run_panel(&router, &experts, "正文", "", ExpertMode::Review, None).await;
        assert!(panel.synthesis.is_some());
        let seen = limits.lock().unwrap();
        assert_eq!(seen.len(), 2, "汇总走普通 chat，不占用专家上限");
        assert!(seen.contains(&experts[0].max_tokens));
        assert!(seen.contains(&experts[1].max_tokens));
    }
}
