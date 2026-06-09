//! Anthropic Messages API provider（含 tools 与 content blocks）。

use crate::provider::Provider;
use crate::types::{CompletionRequest, CompletionResponse, Content, Role, StopReason, ToolCall, Usage};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: u32 = 4096;
const API_KEY_HEADER: &str = "x-api-key";
const VERSION_HEADER: &str = "anthropic-version";

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u64,
    output_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<Value>,
    usage: AnthropicUsage,
    #[serde(default)]
    stop_reason: Option<String>,
}

/// 把内核请求转为 Anthropic Messages API 请求体（含 tools 与 content blocks）。
/// System 消息提取到 top-level `system`；user/assistant 消息内容映射为 content block 数组。
fn to_anthropic_body(req: &CompletionRequest, max_tokens: u32) -> Value {
    let mut system_parts = Vec::new();
    let mut messages = Vec::new();
    for m in &req.messages {
        if matches!(m.role, Role::System) {
            system_parts.push(m.text());
            continue;
        }
        let role = if matches!(m.role, Role::Assistant) { "assistant" } else { "user" };
        let blocks: Vec<Value> = m.content.iter().map(|c| match c {
            Content::Text { text } => json!({"type": "text", "text": text}),
            Content::ToolUse { id, name, input } => json!({"type": "tool_use", "id": id, "name": name, "input": input}),
            Content::ToolResult { tool_use_id, content, is_error } =>
                json!({"type": "tool_result", "tool_use_id": tool_use_id, "content": content, "is_error": is_error}),
        }).collect();
        messages.push(json!({"role": role, "content": blocks}));
    }
    let mut body = json!({"model": req.model, "max_tokens": max_tokens, "messages": messages});
    let system: String = system_parts.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n\n");
    if !system.is_empty() {
        body["system"] = json!(system);
    }
    if !req.tools.is_empty() {
        let tools: Vec<Value> = req.tools.iter().map(|t| json!({
            "name": t.name, "description": t.description, "input_schema": t.input_schema
        })).collect();
        body["tools"] = json!(tools);
    }
    body
}

/// 接入 Anthropic Messages API 的 Provider（非流式，支持工具）。
pub struct AnthropicProvider {
    api_key: String,
    base_url: String,
    http: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::with_base_url(api_key, "https://api.anthropic.com")
    }
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
    fn name(&self) -> &str { "anthropic" }

    async fn complete(&self, req: &CompletionRequest) -> anyhow::Result<CompletionResponse> {
        let body = to_anthropic_body(req, DEFAULT_MAX_TOKENS);
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
        let mut text = String::new();
        let mut tool_calls = Vec::new();
        for block in &parsed.content {
            match block.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                        text.push_str(t);
                    }
                }
                Some("tool_use") => {
                    tool_calls.push(ToolCall {
                        id: block.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                        name: block.get("name").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                        input: block.get("input").cloned().unwrap_or_else(|| json!({})),
                    });
                }
                _ => {}
            }
        }
        let stop_reason = match parsed.stop_reason.as_deref() {
            Some("tool_use") => StopReason::ToolUse,
            Some("end_turn") => StopReason::EndTurn,
            _ => StopReason::Other,
        };
        Ok(CompletionResponse {
            text,
            tool_calls,
            usage: Usage { prompt_tokens: parsed.usage.input_tokens, completion_tokens: parsed.usage.output_tokens },
            stop_reason,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Message, ToolSpec};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn body_moves_system_and_includes_tools() {
        let req = CompletionRequest {
            model: "claude-x".into(),
            messages: vec![Message::system("be terse"), Message::user("hi")],
            tools: vec![ToolSpec { name: "read_file".into(), description: "r".into(), input_schema: json!({"type":"object"}) }],
        };
        let body = to_anthropic_body(&req, 1024);
        assert_eq!(body["system"], json!("be terse"));
        assert_eq!(body["max_tokens"], json!(1024));
        assert_eq!(body["messages"][0]["role"], json!("user"));
        assert_eq!(body["messages"][0]["content"][0]["type"], json!("text"));
        assert_eq!(body["tools"][0]["name"], json!("read_file"));
    }

    #[test]
    fn body_omits_system_and_tools_when_absent() {
        let req = CompletionRequest { model: "m".into(), messages: vec![Message::user("hi")], tools: vec![] };
        let body = to_anthropic_body(&req, 16);
        assert!(body.get("system").is_none());
        assert!(body.get("tools").is_none());
    }

    #[test]
    fn body_serializes_tool_result_block() {
        let req = CompletionRequest {
            model: "m".into(),
            messages: vec![Message { role: Role::User, content: vec![Content::ToolResult { tool_use_id: "t1".into(), content: "ok".into(), is_error: false }] }],
            tools: vec![],
        };
        let body = to_anthropic_body(&req, 16);
        assert_eq!(body["messages"][0]["content"][0]["type"], json!("tool_result"));
        assert_eq!(body["messages"][0]["content"][0]["tool_use_id"], json!("t1"));
    }

    #[tokio::test]
    async fn complete_parses_text_end_turn() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "content": [{"type":"text","text":"hello"}],
                "usage": {"input_tokens":5,"output_tokens":2},
                "stop_reason": "end_turn"
            }))).mount(&server).await;
        let p = AnthropicProvider::with_base_url("k", server.uri());
        let req = CompletionRequest { model: "m".into(), messages: vec![Message::user("hi")], tools: vec![] };
        let resp = p.complete(&req).await.unwrap();
        assert_eq!(resp.text, "hello");
        assert!(resp.tool_calls.is_empty());
        assert_eq!(resp.stop_reason, StopReason::EndTurn);
        assert_eq!(resp.usage.prompt_tokens, 5);
        assert_eq!(resp.usage.completion_tokens, 2);
    }

    #[tokio::test]
    async fn complete_parses_tool_use() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/v1/messages")).and(header("x-api-key", "k"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "content": [
                    {"type":"text","text":"let me read"},
                    {"type":"tool_use","id":"tu_1","name":"read_file","input":{"path":"a.txt"}}
                ],
                "usage": {"input_tokens":7,"output_tokens":3},
                "stop_reason": "tool_use"
            }))).mount(&server).await;
        let p = AnthropicProvider::with_base_url("k", server.uri());
        let req = CompletionRequest {
            model: "m".into(),
            messages: vec![Message::user("read a.txt")],
            tools: vec![ToolSpec { name: "read_file".into(), description: "r".into(), input_schema: json!({}) }],
        };
        let resp = p.complete(&req).await.unwrap();
        assert_eq!(resp.stop_reason, StopReason::ToolUse);
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].name, "read_file");
        assert_eq!(resp.tool_calls[0].id, "tu_1");
        assert_eq!(resp.tool_calls[0].input["path"], json!("a.txt"));
        assert_eq!(resp.text, "let me read");
    }

    #[tokio::test]
    async fn complete_surfaces_api_error_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error": {"message":"credit balance is too low"}
            }))).mount(&server).await;
        let p = AnthropicProvider::with_base_url("k", server.uri());
        let req = CompletionRequest { model: "m".into(), messages: vec![Message::user("hi")], tools: vec![] };
        let err = p.complete(&req).await.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("400"), "err: {msg}");
        assert!(msg.contains("credit balance is too low"), "err: {msg}");
    }

    #[tokio::test]
    #[ignore = "requires ANTHROPIC_API_KEY and network; run with --ignored"]
    async fn real_anthropic_smoke() {
        let key = std::env::var("ANTHROPIC_API_KEY").expect("set ANTHROPIC_API_KEY to run");
        let provider = AnthropicProvider::new(key);
        let req = CompletionRequest {
            model: "claude-3-5-haiku-latest".into(),
            messages: vec![Message::user("Reply with exactly: pong")],
            tools: vec![],
        };
        let resp = provider.complete(&req).await.unwrap();
        assert!(!resp.text.is_empty());
        assert!(resp.usage.completion_tokens > 0);
    }
}
