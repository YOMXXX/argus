//! Provider 抽象与内置 MockProvider。

use crate::types::{CompletionRequest, CompletionResponse, Usage};
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
        Self { name: "mock".to_string() }
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
        let last = req.messages.last().map(|m| m.content.clone()).unwrap_or_default();
        let text = format!("[mock:{}] acknowledged task: {}", req.model, last);
        let usage = Usage {
            // 统计所有消息词数之和，与 Agent 的请求估算口径一致
            prompt_tokens: req
                .messages
                .iter()
                .map(|m| m.content.split_whitespace().count() as u64)
                .sum(),
            completion_tokens: text.split_whitespace().count() as u64,
        };
        Ok(CompletionResponse { text, usage })
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
        };
        let resp = p.complete(&req).await.unwrap();
        assert!(resp.text.contains("build a thing"));
        assert!(resp.text.contains("mock:demo"));
        assert_eq!(p.name(), "mock");
    }

    #[tokio::test]
    async fn mock_provider_handles_empty_messages() {
        let p = MockProvider::new();
        let req = CompletionRequest { model: "x".into(), messages: vec![] };
        let resp = p.complete(&req).await.unwrap();
        assert!(resp.text.contains("mock:x"));
    }
}
