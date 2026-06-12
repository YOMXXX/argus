//! OpenAI 兼容 Chat Completions API provider(OpenAI / OpenRouter / 本地 Ollama 等)。

use crate::provider::Provider;
use crate::types::{CompletionRequest, CompletionResponse, Content, Role, StopReason, ToolCall, Usage};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    #[serde(default)]
    choices: Vec<OaChoice>,
    #[serde(default)]
    usage: OaUsage,
}

#[derive(Debug, Deserialize)]
struct OaChoice {
    message: OaMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OaMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OaToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OaToolCall {
    #[serde(default)]
    id: String,
    function: OaFunction,
}

#[derive(Debug, Deserialize)]
struct OaFunction {
    name: String,
    #[serde(default)]
    arguments: String,
}

#[derive(Debug, Default, Deserialize)]
struct OaUsage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
}

/// 把内核请求转为 OpenAI Chat Completions 请求体。
/// 内核 Content blocks → OpenAI message 结构:
/// - Text → message.content
/// - ToolUse → assistant message.tool_calls[{id,type:"function",function:{name,arguments(JSON 字符串)}}]
/// - ToolResult → 独立的 {role:"tool", tool_call_id, content} 消息
fn to_openai_body(req: &CompletionRequest) -> Value {
    let mut messages = Vec::new();
    for m in &req.messages {
        let role = match m.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        };
        let mut text_parts = Vec::new();
        let mut tool_calls = Vec::new();
        let mut had_tool_result = false;
        for c in &m.content {
            match c {
                Content::Text { text } => text_parts.push(text.clone()),
                Content::ToolUse { id, name, input } => tool_calls.push(json!({
                    "id": id,
                    "type": "function",
                    "function": {"name": name, "arguments": input.to_string()}
                })),
                Content::ToolResult { tool_use_id, content, .. } => {
                    had_tool_result = true;
                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": tool_use_id,
                        "content": content
                    }));
                }
            }
        }
        // 含 tool_result 的消息只产出上面的 role:"tool" 项(OpenAI 要求独立)
        if had_tool_result {
            continue;
        }
        let text = text_parts.join("");
        let mut msg = json!({ "role": role });
        if tool_calls.is_empty() {
            msg["content"] = json!(text);
        } else {
            msg["content"] = if text.is_empty() { Value::Null } else { json!(text) };
            msg["tool_calls"] = json!(tool_calls);
        }
        messages.push(msg);
    }
    let mut body = json!({ "model": req.model, "messages": messages });
    if !req.tools.is_empty() {
        let tools: Vec<Value> = req.tools.iter().map(|t| json!({
            "type": "function",
            "function": { "name": t.name, "description": t.description, "parameters": t.input_schema }
        })).collect();
        body["tools"] = json!(tools);
    }
    body
}

/// 接入 OpenAI 兼容 Chat Completions API 的 Provider(非流式,支持工具)。
pub struct OpenAiProvider {
    api_key: String,
    base_url: String,
    http: reqwest::Client,
}

impl OpenAiProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::with_base_url(api_key, "https://api.openai.com/v1")
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
impl Provider for OpenAiProvider {
    fn name(&self) -> &str { "openai" }

    async fn complete(&self, req: &CompletionRequest) -> anyhow::Result<CompletionResponse> {
        let body = to_openai_body(req);
        let url = format!("{}/chat/completions", self.base_url);
        let resp = self
            .http
            .post(url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI API error {status}: {body_text}");
        }
        let parsed: OpenAiResponse = resp.json().await?;
        let choice = parsed
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("OpenAI response had no choices"))?;
        let text = choice.message.content.unwrap_or_default();
        let mut tool_calls = Vec::new();
        if let Some(tcs) = choice.message.tool_calls {
            for tc in tcs {
                let input = serde_json::from_str(&tc.function.arguments).unwrap_or_else(|_| json!({}));
                tool_calls.push(ToolCall { id: tc.id, name: tc.function.name, input });
            }
        }
        let stop_reason = match choice.finish_reason.as_deref() {
            Some("tool_calls") => StopReason::ToolUse,
            Some("stop") => StopReason::EndTurn,
            _ => StopReason::Other,
        };
        Ok(CompletionResponse {
            text,
            tool_calls,
            usage: Usage {
                prompt_tokens: parsed.usage.prompt_tokens,
                completion_tokens: parsed.usage.completion_tokens,
            },
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
    fn body_maps_roles_and_tools() {
        let req = CompletionRequest {
            model: "gpt-4o-mini".into(),
            messages: vec![Message::system("be terse"), Message::user("hi")],
            tools: vec![ToolSpec { name: "read_file".into(), description: "r".into(), input_schema: json!({"type":"object"}) }],
        };
        let body = to_openai_body(&req);
        assert_eq!(body["messages"][0]["role"], json!("system"));
        assert_eq!(body["messages"][0]["content"], json!("be terse"));
        assert_eq!(body["messages"][1]["role"], json!("user"));
        assert_eq!(body["messages"][1]["content"], json!("hi"));
        assert_eq!(body["tools"][0]["type"], json!("function"));
        assert_eq!(body["tools"][0]["function"]["name"], json!("read_file"));
    }

    #[test]
    fn body_maps_tool_use_and_tool_result() {
        let req = CompletionRequest {
            model: "m".into(),
            messages: vec![
                Message { role: Role::Assistant, content: vec![
                    Content::Text { text: "calling".into() },
                    Content::ToolUse { id: "tc1".into(), name: "read_file".into(), input: json!({"path":"a.txt"}) },
                ] },
                Message { role: Role::User, content: vec![
                    Content::ToolResult { tool_use_id: "tc1".into(), content: "file body".into(), is_error: false },
                ] },
            ],
            tools: vec![],
        };
        let body = to_openai_body(&req);
        // assistant 消息带 tool_calls
        assert_eq!(body["messages"][0]["role"], json!("assistant"));
        assert_eq!(body["messages"][0]["tool_calls"][0]["id"], json!("tc1"));
        assert_eq!(body["messages"][0]["tool_calls"][0]["function"]["name"], json!("read_file"));
        let args = body["messages"][0]["tool_calls"][0]["function"]["arguments"].as_str().unwrap();
        assert!(args.contains("a.txt"), "arguments should be JSON string: {args}");
        // tool result 作为独立 role:"tool" 消息
        assert_eq!(body["messages"][1]["role"], json!("tool"));
        assert_eq!(body["messages"][1]["tool_call_id"], json!("tc1"));
        assert_eq!(body["messages"][1]["content"], json!("file body"));
    }

    #[tokio::test]
    async fn complete_parses_text_stop() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{"message": {"content": "hello"}, "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 5, "completion_tokens": 2}
            }))).mount(&server).await;
        let p = OpenAiProvider::with_base_url("k", server.uri());
        let req = CompletionRequest { model: "m".into(), messages: vec![Message::user("hi")], tools: vec![] };
        let resp = p.complete(&req).await.unwrap();
        assert_eq!(resp.text, "hello");
        assert!(resp.tool_calls.is_empty());
        assert_eq!(resp.stop_reason, StopReason::EndTurn);
        assert_eq!(resp.usage.prompt_tokens, 5);
        assert_eq!(resp.usage.completion_tokens, 2);
    }

    #[tokio::test]
    async fn complete_parses_tool_calls() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/chat/completions")).and(header("authorization", "Bearer k"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{"message": {"content": null, "tool_calls": [
                    {"id": "call_1", "type": "function", "function": {"name": "read_file", "arguments": "{\"path\":\"a.txt\"}"}}
                ]}, "finish_reason": "tool_calls"}],
                "usage": {"prompt_tokens": 7, "completion_tokens": 3}
            }))).mount(&server).await;
        let p = OpenAiProvider::with_base_url("k", server.uri());
        let req = CompletionRequest {
            model: "m".into(),
            messages: vec![Message::user("read a.txt")],
            tools: vec![ToolSpec { name: "read_file".into(), description: "r".into(), input_schema: json!({}) }],
        };
        let resp = p.complete(&req).await.unwrap();
        assert_eq!(resp.stop_reason, StopReason::ToolUse);
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].id, "call_1");
        assert_eq!(resp.tool_calls[0].name, "read_file");
        assert_eq!(resp.tool_calls[0].input["path"], json!("a.txt"));
    }

    #[tokio::test]
    async fn complete_surfaces_api_error_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({
                "error": {"message": "invalid api key"}
            }))).mount(&server).await;
        let p = OpenAiProvider::with_base_url("k", server.uri());
        let req = CompletionRequest { model: "m".into(), messages: vec![Message::user("hi")], tools: vec![] };
        let err = p.complete(&req).await.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("401"), "err: {msg}");
        assert!(msg.contains("invalid api key"), "err: {msg}");
    }

    #[tokio::test]
    #[ignore = "requires OPENAI_API_KEY and network; run with --ignored"]
    async fn real_openai_smoke() {
        let key = std::env::var("OPENAI_API_KEY").expect("set OPENAI_API_KEY to run");
        let provider = OpenAiProvider::new(key);
        let req = CompletionRequest {
            model: "gpt-4o-mini".into(),
            messages: vec![Message::user("Reply with exactly: pong")],
            tools: vec![],
        };
        let resp = provider.complete(&req).await.unwrap();
        assert!(!resp.text.is_empty());
    }
}
