//! Argus 内核 —— 模型无关的 Provider 抽象与 Agent 主循环。

use serde::{Deserialize, Serialize};
use argus_trace::{EventKind, TraceWriter};

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
            // 近似值：mock 仅统计最后一条消息的词数，足够演示
            prompt_tokens: last.split_whitespace().count() as u64,
            completion_tokens: text.split_whitespace().count() as u64,
        };
        Ok(CompletionResponse { text, usage })
    }
}

/// 最小 Agent：把一次任务跑成一轮 think→model→observe，每一步落入 Trace。
pub struct Agent<'a> {
    provider: &'a dyn Provider,
    model: String,
    trace: &'a mut TraceWriter,
}

impl<'a> Agent<'a> {
    pub fn new(
        provider: &'a dyn Provider,
        model: impl Into<String>,
        trace: &'a mut TraceWriter,
    ) -> Self {
        Self { provider, model: model.into(), trace }
    }

    /// 运行一次任务，返回模型输出；全过程写入 Trace。
    pub fn run(&mut self, task: &str) -> anyhow::Result<String> {
        self.trace.record(EventKind::Thought {
            text: format!("Received task: {task}"),
        })?;

        let req = CompletionRequest {
            model: self.model.clone(),
            messages: vec![Message::user(task)],
        };
        // 发送时的客户端估算（按词数）；真实 token 账单以 provider 返回的 usage 为准。
        let prompt_tokens: u64 = req
            .messages
            .iter()
            .map(|m| m.content.split_whitespace().count() as u64)
            .sum();
        self.trace.record(EventKind::ModelRequest {
            model: self.model.clone(),
            prompt_tokens,
        })?;

        let resp = self.provider.complete(&req)?;

        self.trace.record(EventKind::ModelResponse {
            model: self.model.clone(),
            completion_tokens: resp.usage.completion_tokens,
            text: resp.text.clone(),
        })?;

        Ok(resp.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_trace::read_trace;

    fn tmp_path(tag: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("argus-core-test-{}-{}.jsonl", std::process::id(), tag));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn agent_run_records_three_events_and_returns_text() {
        let path = tmp_path("run");
        let provider = MockProvider::new();
        {
            let mut trace = TraceWriter::create(&path).unwrap();
            let mut agent = Agent::new(&provider, "demo", &mut trace);
            let out = agent.run("hello world").unwrap();
            assert!(out.contains("hello world"));
        }
        let events = read_trace(&path).unwrap();
        assert_eq!(events.len(), 3);
        assert!(matches!(events[0].kind, EventKind::Thought { .. }));
        assert!(matches!(events[1].kind, EventKind::ModelRequest { .. }));
        assert!(matches!(events[2].kind, EventKind::ModelResponse { .. }));
        let _ = std::fs::remove_file(&path);
    }

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
