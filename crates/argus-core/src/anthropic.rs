//! Anthropic Messages API provider。
#![allow(dead_code)] // 临时：DTO/转换将在 P1-Task5 接入 complete() 后被使用，届时移除本行

use crate::types::{CompletionRequest, Role};
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Message;

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
}
