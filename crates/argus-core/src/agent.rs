//! Agent 主循环。

use crate::provider::Provider;
use crate::tool::Tool;
use crate::types::{CompletionRequest, Content, Message, Role, ToolSpec};
use argus_trace::{EventKind, TraceWriter};

/// 多轮 Agent：think→model→(工具调用→执行→喂回)*→完成，每步落 Trace。
pub struct Agent<'a> {
    provider: &'a dyn Provider,
    model: String,
    trace: &'a mut TraceWriter,
    tools: Vec<Box<dyn Tool>>,
    max_turns: usize,
}

impl<'a> Agent<'a> {
    pub fn new(provider: &'a dyn Provider, model: impl Into<String>, trace: &'a mut TraceWriter) -> Self {
        Self { provider, model: model.into(), trace, tools: Vec::new(), max_turns: 8 }
    }

    pub fn with_tools(mut self, tools: Vec<Box<dyn Tool>>) -> Self {
        self.tools = tools;
        self
    }

    fn tool_specs(&self) -> Vec<ToolSpec> {
        self.tools.iter().map(|t| ToolSpec {
            name: t.name().to_string(),
            description: t.description().to_string(),
            input_schema: t.input_schema(),
        }).collect()
    }

    /// 运行一次任务，多轮调用工具直到完成，返回最终文本；全过程写入 Trace。
    pub async fn run(&mut self, task: &str) -> anyhow::Result<String> {
        self.trace.record(EventKind::TaskStarted { task: task.to_string() })?;
        self.trace.record(EventKind::Thought { text: format!("Received task: {task}") })?;

        let mut messages = vec![Message::user(task)];
        let specs = self.tool_specs();

        for _turn in 0..self.max_turns {
            let prompt_tokens: u64 = messages.iter()
                .map(|m| m.text().split_whitespace().count() as u64).sum();
            self.trace.record(EventKind::ModelRequest { model: self.model.clone(), prompt_tokens })?;

            let req = CompletionRequest {
                model: self.model.clone(),
                messages: messages.clone(),
                tools: specs.clone(),
            };
            let resp = self.provider.complete(&req).await?;
            self.trace.record(EventKind::ModelResponse {
                model: self.model.clone(),
                prompt_tokens: resp.usage.prompt_tokens,
                completion_tokens: resp.usage.completion_tokens,
                text: resp.text.clone(),
            })?;

            if resp.tool_calls.is_empty() {
                return Ok(resp.text);
            }

            // 记录 assistant 的 text + tool_use 到历史
            let mut assistant_blocks = Vec::new();
            if !resp.text.is_empty() {
                assistant_blocks.push(Content::Text { text: resp.text.clone() });
            }
            for call in &resp.tool_calls {
                assistant_blocks.push(Content::ToolUse {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    input: call.input.clone(),
                });
            }
            messages.push(Message { role: Role::Assistant, content: assistant_blocks });

            // 执行工具，收集 tool_result
            let mut result_blocks = Vec::new();
            for call in &resp.tool_calls {
                self.trace.record(EventKind::ToolCall { name: call.name.clone(), args: call.input.to_string() })?;
                let (output, is_error) = match self.tools.iter().find(|t| t.name() == call.name) {
                    Some(tool) => match tool.execute(&call.input).await {
                        Ok(out) => (out, false),
                        Err(e) => (format!("error: {e}"), true),
                    },
                    None => (format!("error: unknown tool '{}'", call.name), true),
                };
                self.trace.record(EventKind::ToolResult { name: call.name.clone(), ok: !is_error, output: output.clone() })?;
                result_blocks.push(Content::ToolResult { tool_use_id: call.id.clone(), content: output, is_error });
            }
            messages.push(Message { role: Role::User, content: result_blocks });
        }
        anyhow::bail!("max turns ({}) exceeded", self.max_turns)
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
        // 不带工具 → 单轮：TaskStarted/Thought/ModelRequest/ModelResponse
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
        assert!(matches!(events[3].kind, EventKind::ModelResponse { .. }));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn agent_runs_tool_then_finishes() {
        // 带工具 → 多轮：Mock 第一轮调 write_file，执行后第二轮 EndTurn
        let dir = std::env::temp_dir().join(format!("argus-agent-tool-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("trace.jsonl");
        let provider = MockProvider::new();
        {
            let mut trace = TraceWriter::create(&path).unwrap();
            let mut agent = Agent::new(&provider, "demo", &mut trace)
                .with_tools(vec![Box::new(crate::tool::WriteFile::new(&dir))]);
            let out = agent.run("write a file").await.unwrap();
            assert!(out.contains("acknowledged") || out.contains("done"));
        }
        let events = read_trace(&path).unwrap();
        assert!(events.iter().any(|e| matches!(e.kind, EventKind::ToolCall { .. })));
        assert!(events.iter().any(|e| matches!(e.kind, EventKind::ToolResult { ok: true, .. })));
        assert!(dir.join("mock.txt").exists(), "tool should have written mock.txt");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
