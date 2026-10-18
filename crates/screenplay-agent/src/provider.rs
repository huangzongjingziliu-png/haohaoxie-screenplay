//! 模型 Provider 抽象。
//!
//! 说明书第 14 条：所有模型调用统一走 ModelRouter，禁止散落直连。
//! 支持 OpenAI 兼容接口（base_url + api_key）。

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// 一条聊天消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // system / user / assistant
    pub content: String,
}

/// 模型 Provider 接口
#[async_trait::async_trait]
pub trait ModelProvider: Send + Sync {
    fn name(&self) -> &str;
    fn cache_identity(&self) -> Option<String> { None }
    /// 发起聊天补全请求
    async fn chat(&self, messages: &[ChatMessage], temperature: f32) -> Result<String>;
}

/// OpenAI 兼容 Provider
pub struct OpenAiProvider {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub client: reqwest::Client,
}

impl OpenAiProvider {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: model.into(),
            client: reqwest::Client::builder().connect_timeout(std::time::Duration::from_secs(10)).timeout(std::time::Duration::from_secs(90)).build().expect("valid HTTP client settings"),
        }
    }

    /// 便捷构造：OpenAI 官方端点
    pub fn openai(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self::new("https://api.openai.com/v1", api_key, model)
    }

    /// 便捷构造：任意 OpenAI-compatible 端点（DeepSeek / Qwen / vLLM / LM Studio 等）
    pub fn compatible(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self::new(base_url, api_key, model)
    }

    /// 便捷构造：本地 Ollama（默认 http://localhost:11434/v1）
    ///
    /// Ollama 从 v0.1.14 起提供 OpenAI-compatible `/v1/chat/completions` 端点，
    /// api_key 可填任意非空字符串。
    pub fn ollama(model: impl Into<String>) -> Self {
        Self::new("http://localhost:11434/v1", "ollama", model)
    }

    /// 返回该 provider 实际请求的完整 URL（用于测试 / 调试）
    pub fn endpoint_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    fn request_body<'a>(&'a self, messages: &'a [ChatMessage], temperature: f32) -> ChatRequest<'a> {
        let anthropic = reqwest::Url::parse(&self.base_url).ok().and_then(|address|address.host_str().map(str::to_owned)).as_deref()==Some("api.anthropic.com");
        ChatRequest { model: &self.model, messages, temperature: if anthropic {None} else {Some(temperature)}, max_tokens:4096 }
    }
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    max_tokens: u32,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    #[serde(default)]
    finish_reason: Option<String>,
    message: Msg,
}

#[derive(Deserialize)]
struct Msg {
    content: String,
}

#[async_trait::async_trait]
impl ModelProvider for OpenAiProvider {
    fn name(&self) -> &str {
        &self.model
    }

    fn cache_identity(&self) -> Option<String> {
        use sha2::{Digest,Sha256};
        Some(format!("{:x}",Sha256::digest(format!("{}\n{}\n{}",self.base_url,self.model,self.api_key).as_bytes())))
    }
    async fn chat(&self, messages: &[ChatMessage], temperature: f32) -> Result<String> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let body = self.request_body(messages, temperature);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("模型请求发送失败")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default().replace(&self.api_key, "[redacted]");
            let text: String = text.chars().take(2000).collect();
            if status.as_u16()==429 && text.contains("free users") && text.contains("rate limit") {
                anyhow::bail!("免费模型请求额度已达上限。请等待额度恢复或在编剧助理中更换模型。当前剧本未改变。");
            }
            anyhow::bail!("模型返回错误 {status}: {text}");
        }
        let data: ChatResponse = resp.json().await.context("解析模型响应失败")?;
        data.choices
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("模型未返回任何选择"))
            .and_then(|c|{if matches!(c.finish_reason.as_deref(),Some("length")|Some("content_filter")){anyhow::bail!("模型输出不完整，原稿未改变。请缩小范围或更换模型。");}Ok(c.message.content)})
    }
}

/// 模型路由器：按名称选择 provider
pub struct ModelRouter {
    pub providers: Vec<Box<dyn ModelProvider>>,
    pub default_model: String,
}

impl ModelRouter {
    pub fn new(default_model: impl Into<String>) -> Self {
        Self {
            providers: Vec::new(),
            default_model: default_model.into(),
        }
    }

    pub fn register(&mut self, p: Box<dyn ModelProvider>) {
        self.providers.push(p);
    }

    /// 选择 provider：优先匹配 model 名称，否则用默认
    pub fn select(&self, model: Option<&str>) -> Option<&dyn ModelProvider> {
        let target = model.unwrap_or(&self.default_model);
        self.providers
            .iter()
            .find(|p| p.name() == target)
            .or_else(|| self.providers.first())
            .map(|p| p.as_ref())
    }

    pub async fn chat(
        &self,
        model: Option<&str>,
        messages: &[ChatMessage],
        temperature: f32,
    ) -> Result<String> {
        let p = self
            .select(model)
            .ok_or_else(|| anyhow::anyhow!("没有可用的模型 provider"))?;
        p.chat(messages, temperature).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_compatibility_omits_unsupported_sampling_override() {
        let messages = vec![ChatMessage {role:"user".into(),content:"Hello".into()}];
        let provider = OpenAiProvider::compatible("https://api.anthropic.com/v1", "test-key", "claude-model");
        let body = serde_json::to_value(provider.request_body(&messages,0.2)).unwrap();
        assert!(body.get("temperature").is_none());
        assert_eq!(body["model"],"claude-model");
        assert_eq!(body["messages"][0]["content"],"Hello");
        let provider = OpenAiProvider::compatible("https://api.deepseek.com", "test-key", "chat-model");
        assert!(serde_json::to_value(provider.request_body(&messages,0.2)).unwrap().get("temperature").is_some());
    }

    #[test]
    fn t01_ollama_endpoint() {
        let p = OpenAiProvider::ollama("llama3");
        assert_eq!(
            p.endpoint_url(),
            "http://localhost:11434/v1/chat/completions"
        );
        assert_eq!(p.api_key, "ollama");
        assert_eq!(p.model, "llama3");
    }

    #[test]
    fn t02_openai_endpoint() {
        let p = OpenAiProvider::openai("sk-xxx", "gpt-4o-mini");
        assert_eq!(
            p.endpoint_url(),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn t03_compatible_endpoint() {
        let p = OpenAiProvider::compatible(
            "https://api.deepseek.com/v1",
            "sk-xxx",
            "deepseek-chat",
        );
        assert_eq!(
            p.endpoint_url(),
            "https://api.deepseek.com/v1/chat/completions"
        );
    }

    #[test]
    fn t04_base_url_trailing_slash_stripped() {
        let p = OpenAiProvider::new("http://x/v1/", "k", "m");
        assert_eq!(p.endpoint_url(), "http://x/v1/chat/completions");
    }

    #[test]
    fn t05_router_select_falls_back_to_first() {
        let mut r = ModelRouter::new("default");
        r.register(Box::new(OpenAiProvider::ollama("llama3")));
        let p = r.select(Some("nonexistent")).unwrap();
        assert_eq!(p.name(), "llama3");
    }
}
