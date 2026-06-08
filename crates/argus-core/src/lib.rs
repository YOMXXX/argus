//! Argus 内核 —— 模型无关的 Provider 抽象与 Agent 主循环。

use serde::{Deserialize, Serialize};

/// 对话角色。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
}

/// 一条对话消息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: Role::System, content: content.into() }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: Role::User, content: content.into() }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: Role::Assistant, content: content.into() }
    }
}

/// 模型补全请求。
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
}

/// token 使用量。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

/// 模型补全响应。
#[derive(Debug, Clone, PartialEq)]
pub struct CompletionResponse {
    pub text: String,
    pub usage: Usage,
}

/// 模型 Provider 抽象 —— "模型无关"的核心接口。
pub trait Provider {
    fn name(&self) -> &str;
    fn complete(&self, req: &CompletionRequest) -> anyhow::Result<CompletionResponse>;
}

/// 确定性的 Mock Provider，让 Argus 无需任何 API key 即可演示与测试。
pub struct MockProvider {
    name: String,
}

impl MockProvider {
    pub fn new() -> Self {
        Self { name: "mock".to_string() }
    }
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for MockProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn complete(&self, req: &CompletionRequest) -> anyhow::Result<CompletionResponse> {
        let last = req.messages.last().map(|m| m.content.clone()).unwrap_or_default();
        let text = format!("[mock:{}] acknowledged task: {}", req.model, last);
        let usage = Usage {
            prompt_tokens: last.split_whitespace().count() as u64,
            completion_tokens: text.split_whitespace().count() as u64,
        };
        Ok(CompletionResponse { text, usage })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_provider_echoes_task() {
        let p = MockProvider::new();
        let req = CompletionRequest {
            model: "demo".into(),
            messages: vec![Message::user("build a thing")],
        };
        let resp = p.complete(&req).unwrap();
        assert!(resp.text.contains("build a thing"));
        assert!(resp.text.contains("mock:demo"));
        assert_eq!(p.name(), "mock");
    }
}
