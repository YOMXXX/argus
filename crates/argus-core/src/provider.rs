//! Provider 抽象与内置 MockProvider。

use crate::types::{CompletionRequest, CompletionResponse, Usage};

/// 模型 Provider 抽象 —— "模型无关"的核心接口。（后续 task 将 async 化）
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
            // 近似值：mock 仅统计最后一条消息的词数，足够演示
            prompt_tokens: last.split_whitespace().count() as u64,
            completion_tokens: text.split_whitespace().count() as u64,
        };
        Ok(CompletionResponse { text, usage })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Message;

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

    #[test]
    fn mock_provider_handles_empty_messages() {
        let p = MockProvider::new();
        let req = CompletionRequest { model: "x".into(), messages: vec![] };
        let resp = p.complete(&req).unwrap();
        assert!(resp.text.contains("mock:x"));
    }
}
