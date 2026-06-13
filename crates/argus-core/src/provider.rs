//! Provider 抽象与内置 MockProvider。

use crate::types::{CompletionRequest, CompletionResponse, StopReason, Usage};
use async_trait::async_trait;

/// 模型 Provider 抽象 —— "模型无关"的核心接口。
#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    async fn complete(&self, req: &CompletionRequest) -> anyhow::Result<CompletionResponse>;
}

/// 确定性的 Mock Provider，让 Argus 无需任何 API key 即可演示与测试。
pub struct MockProvider {
    name: String,
}

impl MockProvider {
    pub fn new() -> Self {
        Self {
            name: "mock".to_string(),
        }
    }
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for MockProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn complete(&self, req: &CompletionRequest) -> anyhow::Result<CompletionResponse> {
        let has_tool_result = req.messages.iter().any(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, crate::types::Content::ToolResult { .. }))
        });
        // 取第一条 user 消息作为原始任务：跳过可能前置的 system 消息,也跳过多轮里的 ToolResult。
        let task_text = req
            .messages
            .iter()
            .find(|m| matches!(m.role, crate::types::Role::User))
            .map(|m| m.text())
            .unwrap_or_default();
        let usage = Usage {
            prompt_tokens: req
                .messages
                .iter()
                .map(|m| m.text().split_whitespace().count() as u64)
                .sum(),
            completion_tokens: 4,
        };
        if !req.tools.is_empty() && !has_tool_result {
            let tool = &req.tools[0];
            let input = if tool.name == "write_file" {
                serde_json::json!({"path": "mock.txt", "content": "from mock"})
            } else {
                serde_json::json!({"path": "mock.txt"})
            };
            return Ok(CompletionResponse {
                text: String::new(),
                tool_calls: vec![crate::types::ToolCall {
                    id: "mock-1".into(),
                    name: tool.name.clone(),
                    input,
                }],
                usage,
                stop_reason: StopReason::ToolUse,
            });
        }
        let text = format!("[mock:{}] acknowledged task: {}", req.model, task_text);
        Ok(CompletionResponse {
            text,
            tool_calls: vec![],
            usage,
            stop_reason: StopReason::EndTurn,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Message;

    #[tokio::test]
    async fn mock_provider_echoes_task() {
        let p = MockProvider::new();
        let req = CompletionRequest {
            model: "demo".into(),
            messages: vec![Message::user("build a thing")],
            tools: vec![],
        };
        let resp = p.complete(&req).await.unwrap();
        assert!(resp.text.contains("build a thing"));
        assert!(resp.text.contains("mock:demo"));
        assert!(resp.tool_calls.is_empty());
        assert_eq!(resp.stop_reason, StopReason::EndTurn);
        assert_eq!(p.name(), "mock");
    }

    #[tokio::test]
    async fn mock_provider_handles_empty_messages() {
        let p = MockProvider::new();
        let req = CompletionRequest {
            model: "x".into(),
            messages: vec![],
            tools: vec![],
        };
        let resp = p.complete(&req).await.unwrap();
        assert!(resp.text.contains("mock:x"));
        assert!(resp.tool_calls.is_empty());
        assert_eq!(resp.stop_reason, StopReason::EndTurn);
    }

    #[tokio::test]
    async fn mock_returns_tool_use_when_tools_present() {
        let p = MockProvider::new();
        let req = CompletionRequest {
            model: "demo".into(),
            messages: vec![Message::user("do it")],
            tools: vec![crate::types::ToolSpec {
                name: "read_file".into(),
                description: "read".into(),
                input_schema: serde_json::json!({}),
            }],
        };
        let resp = p.complete(&req).await.unwrap();
        assert_eq!(resp.stop_reason, StopReason::ToolUse);
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].name, "read_file");
    }

    #[tokio::test]
    async fn mock_ignores_system_uses_first_user() {
        let p = MockProvider::new();
        let req = CompletionRequest {
            model: "demo".into(),
            messages: vec![
                Message::system("project rules here"),
                Message::user("build a thing"),
            ],
            tools: vec![],
        };
        let resp = p.complete(&req).await.unwrap();
        assert!(
            resp.text.contains("build a thing"),
            "should use first user msg, got: {}",
            resp.text
        );
        assert!(
            !resp.text.contains("project rules"),
            "should not echo system: {}",
            resp.text
        );
    }
}
