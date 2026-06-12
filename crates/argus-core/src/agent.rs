//! Agent 主循环。

use crate::approver::Approver;
use crate::provider::Provider;
use crate::tool::Tool;
use crate::types::{CompletionRequest, Content, Message, Role, ToolSpec};
use crate::verifier::Verifier;
use argus_trace::{EventKind, TraceWriter};

/// 多轮 Agent：think→model→(工具调用→执行→喂回)*→完成，每步落 Trace。
pub struct Agent<'a> {
    provider: &'a dyn Provider,
    model: String,
    trace: &'a mut TraceWriter,
    tools: Vec<Box<dyn Tool>>,
    max_turns: usize,
    approver: Option<Box<dyn Approver>>,
    verifier: Option<Box<dyn Verifier>>,
    max_verify_attempts: usize,
    system: Option<String>,
}

impl<'a> Agent<'a> {
    pub fn new(provider: &'a dyn Provider, model: impl Into<String>, trace: &'a mut TraceWriter) -> Self {
        Self {
            provider,
            model: model.into(),
            trace,
            tools: Vec::new(),
            max_turns: 8,
            approver: None,
            verifier: None,
            max_verify_attempts: 3,
            system: None,
        }
    }

    pub fn with_tools(mut self, tools: Vec<Box<dyn Tool>>) -> Self {
        self.tools = tools;
        self
    }

    pub fn with_approver(mut self, approver: Box<dyn Approver>) -> Self {
        self.approver = Some(approver);
        self
    }

    pub fn with_verifier(mut self, verifier: Box<dyn Verifier>) -> Self {
        self.verifier = Some(verifier);
        self
    }

    /// 设置 system prompt(如从 AGENTS.md / CLAUDE.md 导入的项目规则)。
    pub fn with_system(mut self, system: impl Into<String>) -> Self {
        self.system = Some(system.into());
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

        let mut messages = Vec::new();
        if let Some(system) = &self.system {
            messages.push(Message::system(system.clone()));
        }
        messages.push(Message::user(task));
        let mut verify_attempts = 0usize;
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
                // model 认为完成 —— 若有验证护栏，先过验证才算完成
                match &self.verifier {
                    None => return Ok(resp.text),
                    Some(verifier) => {
                        let vr = verifier.verify().await;
                        self.trace.record(EventKind::VerificationGate { passed: vr.passed, detail: vr.detail.clone() })?;
                        if vr.passed {
                            return Ok(resp.text);
                        }
                        verify_attempts += 1;
                        if verify_attempts >= self.max_verify_attempts {
                            return Ok(format!(
                                "{}\n\n[verification still failing after {} attempt(s)]\n{}",
                                resp.text, self.max_verify_attempts, vr.detail
                            ));
                        }
                        if !resp.text.is_empty() {
                            messages.push(Message { role: Role::Assistant, content: vec![Content::Text { text: resp.text.clone() }] });
                        }
                        messages.push(Message::user(format!(
                            "Verification failed. Fix the issues and continue.\n{}",
                            vr.detail
                        )));
                        continue;
                    }
                }
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
                let tool = self.tools.iter().find(|t| t.name() == call.name);
                let (output, is_error) = match tool {
                    Some(tool) if tool.requires_approval() => {
                        let approved = match &self.approver {
                            Some(a) => a.approve(tool.name(), &call.input.to_string()),
                            None => false, // 需审批但无审批者：默认拒绝（安全）
                        };
                        if approved {
                            match tool.execute(&call.input).await {
                                Ok(out) => (out, false),
                                Err(e) => (format!("error: {e}"), true),
                            }
                        } else {
                            (format!("denied by user: {}", tool.name()), true)
                        }
                    }
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
    use crate::approver::AutoApprover;
    use crate::provider::{MockProvider, Provider};
    use crate::types::{CompletionRequest, CompletionResponse, StopReason, ToolCall, Usage};
    use crate::verifier::{VerifyResult, Verifier};
    use argus_trace::{read_trace, EventKind, TraceWriter};
    use async_trait::async_trait;

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

    // 第一轮调 run_shell(echo)、收到结果后结束的测试用 provider
    struct ShellOnceProvider;
    #[async_trait]
    impl Provider for ShellOnceProvider {
        fn name(&self) -> &str { "shell-once" }
        async fn complete(&self, req: &CompletionRequest) -> anyhow::Result<CompletionResponse> {
            let has_result = req.messages.iter().any(|m| m.content.iter().any(|c| matches!(c, Content::ToolResult { .. })));
            let usage = Usage { prompt_tokens: 1, completion_tokens: 1 };
            if !has_result {
                return Ok(CompletionResponse {
                    text: String::new(),
                    tool_calls: vec![ToolCall { id: "s1".into(), name: "run_shell".into(), input: serde_json::json!({"command":"echo hi-shell"}) }],
                    usage, stop_reason: StopReason::ToolUse,
                });
            }
            Ok(CompletionResponse { text: "done".into(), tool_calls: vec![], usage, stop_reason: StopReason::EndTurn })
        }
    }

    #[tokio::test]
    async fn shell_runs_when_approved() {
        let dir = std::env::temp_dir().join(format!("argus-shell-ok-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.jsonl");
        let provider = ShellOnceProvider;
        {
            let mut trace = TraceWriter::create(&path).unwrap();
            let mut agent = Agent::new(&provider, "m", &mut trace)
                .with_tools(vec![Box::new(crate::tool::RunShell::new(&dir))])
                .with_approver(Box::new(AutoApprover));
            let out = agent.run("run echo").await.unwrap();
            assert_eq!(out, "done");
        }
        let events = read_trace(&path).unwrap();
        assert!(events.iter().any(|e| matches!(&e.kind, EventKind::ToolResult { ok: true, output, .. } if output.contains("hi-shell"))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn shell_denied_without_approver() {
        let dir = std::env::temp_dir().join(format!("argus-shell-deny-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.jsonl");
        let provider = ShellOnceProvider;
        {
            let mut trace = TraceWriter::create(&path).unwrap();
            let mut agent = Agent::new(&provider, "m", &mut trace)
                .with_tools(vec![Box::new(crate::tool::RunShell::new(&dir))]);
            let _ = agent.run("run echo").await.unwrap();
        }
        let events = read_trace(&path).unwrap();
        assert!(events.iter().any(|e| matches!(&e.kind, EventKind::ToolResult { ok: false, output, .. } if output.contains("denied"))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    struct PassVerifier;
    #[async_trait]
    impl Verifier for PassVerifier {
        async fn verify(&self) -> VerifyResult { VerifyResult { passed: true, detail: "ok".into() } }
    }
    struct AlwaysFailVerifier;
    #[async_trait]
    impl Verifier for AlwaysFailVerifier {
        async fn verify(&self) -> VerifyResult { VerifyResult { passed: false, detail: "nope".into() } }
    }
    struct FailThenPassVerifier { calls: std::sync::Mutex<u32> }
    #[async_trait]
    impl Verifier for FailThenPassVerifier {
        async fn verify(&self) -> VerifyResult {
            let mut c = self.calls.lock().unwrap();
            *c += 1;
            if *c == 1 { VerifyResult { passed: false, detail: "first fail".into() } }
            else { VerifyResult { passed: true, detail: "ok".into() } }
        }
    }

    #[tokio::test]
    async fn gate_passes_returns_done() {
        let path = tmp_path("gate-pass");
        let provider = MockProvider::new();
        {
            let mut trace = TraceWriter::create(&path).unwrap();
            let mut agent = Agent::new(&provider, "m", &mut trace).with_verifier(Box::new(PassVerifier));
            let out = agent.run("do it").await.unwrap();
            assert!(out.contains("acknowledged"));
        }
        let events = read_trace(&path).unwrap();
        assert!(events.iter().any(|e| matches!(&e.kind, EventKind::VerificationGate { passed: true, .. })));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn gate_fail_then_pass_reprompts() {
        let path = tmp_path("gate-retry");
        let provider = MockProvider::new();
        {
            let mut trace = TraceWriter::create(&path).unwrap();
            let mut agent = Agent::new(&provider, "m", &mut trace)
                .with_verifier(Box::new(FailThenPassVerifier { calls: std::sync::Mutex::new(0) }));
            let _ = agent.run("do it").await.unwrap();
        }
        let events = read_trace(&path).unwrap();
        assert!(events.iter().any(|e| matches!(&e.kind, EventKind::VerificationGate { passed: false, .. })));
        assert!(events.iter().any(|e| matches!(&e.kind, EventKind::VerificationGate { passed: true, .. })));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn gate_circuit_breaks_after_max_attempts() {
        let path = tmp_path("gate-break");
        let provider = MockProvider::new();
        {
            let mut trace = TraceWriter::create(&path).unwrap();
            let mut agent = Agent::new(&provider, "m", &mut trace).with_verifier(Box::new(AlwaysFailVerifier));
            let out = agent.run("do it").await.unwrap();
            assert!(out.contains("still failing"), "out: {out}");
        }
        let events = read_trace(&path).unwrap();
        let fails = events.iter().filter(|e| matches!(&e.kind, EventKind::VerificationGate { passed: false, .. })).count();
        assert_eq!(fails, 3, "should verify max_verify_attempts(3) times");
        let _ = std::fs::remove_file(&path);
    }

    struct CaptureSystemProvider {
        saw_system: std::sync::Mutex<Option<String>>,
    }
    #[async_trait]
    impl Provider for CaptureSystemProvider {
        fn name(&self) -> &str { "capture" }
        async fn complete(&self, req: &CompletionRequest) -> anyhow::Result<CompletionResponse> {
            let sys = req.messages.iter()
                .find(|m| matches!(m.role, Role::System))
                .map(|m| m.text());
            *self.saw_system.lock().unwrap() = sys;
            Ok(CompletionResponse {
                text: "ok".into(),
                tool_calls: vec![],
                usage: Usage { prompt_tokens: 1, completion_tokens: 1 },
                stop_reason: StopReason::EndTurn,
            })
        }
    }

    #[tokio::test]
    async fn with_system_injects_system_message() {
        let path = tmp_path("system");
        let provider = CaptureSystemProvider { saw_system: std::sync::Mutex::new(None) };
        {
            let mut trace = TraceWriter::create(&path).unwrap();
            let mut agent = Agent::new(&provider, "m", &mut trace).with_system("always be terse");
            let _ = agent.run("do it").await.unwrap();
        }
        let seen = provider.saw_system.lock().unwrap().clone();
        assert_eq!(seen.as_deref(), Some("always be terse"));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn without_system_no_system_message() {
        let path = tmp_path("nosystem");
        let provider = CaptureSystemProvider { saw_system: std::sync::Mutex::new(None) };
        {
            let mut trace = TraceWriter::create(&path).unwrap();
            let mut agent = Agent::new(&provider, "m", &mut trace);
            let _ = agent.run("do it").await.unwrap();
        }
        assert!(provider.saw_system.lock().unwrap().is_none());
        let _ = std::fs::remove_file(&path);
    }
}
