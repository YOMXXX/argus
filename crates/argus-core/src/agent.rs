//! Agent 主循环。

use crate::provider::Provider;
use crate::types::{CompletionRequest, Message};
use argus_trace::{EventKind, TraceWriter};

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
    pub async fn run(&mut self, task: &str) -> anyhow::Result<String> {
        self.trace.record(EventKind::TaskStarted { task: task.to_string() })?;
        self.trace.record(EventKind::Thought {
            text: format!("Received task: {task}"),
        })?;

        let req = CompletionRequest {
            model: self.model.clone(),
            messages: vec![Message::user(task)],
            tools: vec![],
        };
        // 发送时的客户端估算（按词数）；真实 token 账单以 provider 返回的 usage 为准。
        let prompt_tokens: u64 = req
            .messages
            .iter()
            .map(|m| m.text().split_whitespace().count() as u64)
            .sum();
        self.trace.record(EventKind::ModelRequest {
            model: self.model.clone(),
            prompt_tokens,
        })?;

        let resp = self.provider.complete(&req).await?;

        self.trace.record(EventKind::ModelResponse {
            model: self.model.clone(),
            prompt_tokens: resp.usage.prompt_tokens,
            completion_tokens: resp.usage.completion_tokens,
            text: resp.text.clone(),
        })?;

        Ok(resp.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::MockProvider;
    use argus_trace::{read_trace, EventKind, TraceWriter};

    fn tmp_path(tag: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("argus-core-test-{}-{}.jsonl", std::process::id(), tag));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[tokio::test]
    async fn agent_run_records_events_and_returns_text() {
        let path = tmp_path("run");
        let provider = MockProvider::new();
        {
            let mut trace = TraceWriter::create(&path).unwrap();
            let mut agent = Agent::new(&provider, "demo", &mut trace);
            let out = agent.run("hello world").await.unwrap();
            assert!(out.contains("hello world"));
        }
        let events = read_trace(&path).unwrap();
        assert_eq!(events.len(), 4);
        assert!(matches!(events[0].kind, EventKind::TaskStarted { .. }));
        assert!(matches!(events[1].kind, EventKind::Thought { .. }));
        assert!(matches!(events[2].kind, EventKind::ModelRequest { .. }));
        match &events[3].kind {
            EventKind::ModelResponse { prompt_tokens, completion_tokens, .. } => {
                assert!(*prompt_tokens > 0);
                assert!(*completion_tokens > 0);
            }
            other => panic!("expected ModelResponse, got {other:?}"),
        }
        let _ = std::fs::remove_file(&path);
    }
}
