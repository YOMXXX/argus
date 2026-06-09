//! Anthropic Messages API provider。

use crate::provider::Provider;
use crate::types::{CompletionRequest, CompletionResponse, Role, Usage};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Anthropic /v1/messages 请求体。
#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

/// Anthropic 响应体（仅取我们需要的字段）。
#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
    usage: AnthropicUsage,
}

#[derive(Debug, Deserialize)]
struct AnthropicContent {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u64,
    output_tokens: u64,
}

/// 把内核的 CompletionRequest 转为 Anthropic 请求体：
/// System 消息合并进 top-level `system`，user/assistant 进 `messages`。
fn to_anthropic_request(req: &CompletionRequest, max_tokens: u32) -> AnthropicRequest {
    let mut system_parts = Vec::new();
    let mut messages = Vec::new();
    for m in &req.messages {
        match m.role {
            Role::System => system_parts.push(m.content.clone()),
            Role::User => messages.push(AnthropicMessage { role: "user".into(), content: m.content.clone() }),
            Role::Assistant => messages.push(AnthropicMessage { role: "assistant".into(), content: m.content.clone() }),
        }
    }
    let system = if system_parts.is_empty() { None } else { Some(system_parts.join("\n\n")) };
    AnthropicRequest { model: req.model.clone(), max_tokens, system, messages }
}

/// 从 Anthropic 响应提取拼接后的文本（拼接所有 text 块）。
fn extract_text(resp: &AnthropicResponse) -> String {
    resp.content
        .iter()
        .filter(|c| c.kind == "text")
        .map(|c| c.text.as_str())
        .collect::<Vec<_>>()
        .join("")
}

const ANTHROPIC_VERSION: &str = "2023-06-01";
const API_KEY_HEADER: &str = "x-api-key";
const VERSION_HEADER: &str = "anthropic-version";
// 已知局限：固定上限；较新模型支持更大 max_tokens，后续可改为 per-request 可配置。
const DEFAULT_MAX_TOKENS: u32 = 4096;

/// 接入 Anthropic Messages API 的 Provider（非流式）。
pub struct AnthropicProvider {
    api_key: String,
    base_url: String,
    http: reqwest::Client,
}

impl AnthropicProvider {
    /// 用 API key 构造，指向官方端点。
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::with_base_url(api_key, "https://api.anthropic.com")
    }

    /// 用自定义 base_url 构造（测试注入 wiremock 用）。
    pub fn with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    async fn complete(&self, req: &CompletionRequest) -> anyhow::Result<CompletionResponse> {
        let body = to_anthropic_request(req, DEFAULT_MAX_TOKENS);
        let url = format!("{}/v1/messages", self.base_url);
        let resp = self
            .http
            .post(url)
            .header(API_KEY_HEADER, &self.api_key)
            .header(VERSION_HEADER, ANTHROPIC_VERSION)
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic API error {status}: {body_text}");
        }
        let parsed: AnthropicResponse = resp.json().await?;
        // 字段名映射：Anthropic input/output_tokens → 内核 prompt/completion_tokens
        Ok(CompletionResponse {
            text: extract_text(&parsed),
            usage: Usage {
                prompt_tokens: parsed.usage.input_tokens,
                completion_tokens: parsed.usage.output_tokens,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Message;
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn request_moves_system_to_toplevel() {
        let req = CompletionRequest {
            model: "claude-x".into(),
            messages: vec![Message::system("be terse"), Message::user("hi")],
        };
        let ar = to_anthropic_request(&req, 1024);
        assert_eq!(ar.system.as_deref(), Some("be terse"));
        assert_eq!(ar.messages.len(), 1);
        assert_eq!(ar.messages[0].role, "user");
        assert_eq!(ar.max_tokens, 1024);
        let json = serde_json::to_string(&ar).unwrap();
        assert!(json.contains("\"system\":\"be terse\""));
        assert!(json.contains("\"max_tokens\":1024"));
    }

    #[test]
    fn request_omits_system_when_absent() {
        let req = CompletionRequest { model: "claude-x".into(), messages: vec![Message::user("hi")] };
        let ar = to_anthropic_request(&req, 16);
        assert!(ar.system.is_none());
        let json = serde_json::to_string(&ar).unwrap();
        assert!(!json.contains("system"));
    }

    #[test]
    fn request_joins_multiple_system_messages() {
        let req = CompletionRequest {
            model: "claude-x".into(),
            messages: vec![Message::system("part1"), Message::system("part2"), Message::user("hi")],
        };
        let ar = to_anthropic_request(&req, 8);
        assert_eq!(ar.system.as_deref(), Some("part1\n\npart2"));
        assert_eq!(ar.messages.len(), 1);
    }

    #[test]
    fn response_extracts_and_concatenates_text() {
        let raw = r#"{"content":[{"type":"text","text":"Hello "},{"type":"text","text":"world"}],"usage":{"input_tokens":11,"output_tokens":3}}"#;
        let resp: AnthropicResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(extract_text(&resp), "Hello world");
        assert_eq!(resp.usage.input_tokens, 11);
        assert_eq!(resp.usage.output_tokens, 3);
    }

    #[tokio::test]
    async fn complete_posts_and_parses_via_wiremock() {
        let server = MockServer::start().await;
        let body = serde_json::json!({
            "content": [{"type": "text", "text": "mocked reply"}],
            "usage": {"input_tokens": 5, "output_tokens": 2}
        });
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "test-key"))
            .and(header("anthropic-version", "2023-06-01"))
            .and(body_json(serde_json::json!({
                "model": "claude-x",
                "max_tokens": 4096,
                "messages": [{"role": "user", "content": "hi"}]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let provider = AnthropicProvider::with_base_url("test-key", server.uri());
        let req = CompletionRequest {
            model: "claude-x".into(),
            messages: vec![crate::types::Message::user("hi")],
        };
        let resp = provider.complete(&req).await.unwrap();
        assert_eq!(resp.text, "mocked reply");
        assert_eq!(resp.usage.prompt_tokens, 5);
        assert_eq!(resp.usage.completion_tokens, 2);
        assert_eq!(provider.name(), "anthropic");
    }

    #[tokio::test]
    async fn complete_surfaces_api_error_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "type": "error",
                "error": {"type": "invalid_request_error", "message": "credit balance is too low"}
            })))
            .mount(&server)
            .await;
        let provider = AnthropicProvider::with_base_url("k", server.uri());
        let req = CompletionRequest { model: "claude-x".into(), messages: vec![crate::types::Message::user("hi")] };
        let err = provider.complete(&req).await.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("400"), "err was: {msg}");
        assert!(msg.contains("credit balance is too low"), "err was: {msg}");
    }
}
