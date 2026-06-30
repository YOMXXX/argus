//! Argus 内核 —— 模型无关的 Provider 抽象与 Agent 主循环。

use argus_trace::{EventKind, TraceEvent};

pub mod agent;
pub(crate) mod anthropic;
pub mod approver;
pub mod command;
pub mod cost;
pub mod eval;
pub mod mcp;
pub(crate) mod openai;
pub mod policy;
pub mod provider;
pub mod router;
pub mod tool;
pub mod types;
pub mod verifier;

pub use agent::Agent;
pub use anthropic::AnthropicProvider;
pub use approver::{Approver, AutoApprover};
pub use command::{CommandOutput, CommandRunner, ExecutionLimits};
pub use cost::estimate_cost;
pub use eval::{
    run_suite, run_suite_with_options, AttemptResult, CaseResult, EvalCase, EvalIsolation,
    EvalRunOptions, EvalSuite, SuiteReport,
};
pub use mcp::{mcp_connect, McpClient, McpTool, McpToolDef};
pub use openai::OpenAiProvider;
pub use policy::{OperationKind, PolicyAction, PolicyDecision, PolicyMode, SandboxPolicy};
pub use provider::{MockProvider, Provider};
pub use router::{run_with_escalation, RouteReport};
pub use tool::{ListFiles, ReadFile, RunShell, SearchText, Tool, WriteFile};
pub use types::{
    CompletionRequest, CompletionResponse, Content, Message, Role, StopReason, ToolCall, ToolSpec,
    Usage,
};
pub use verifier::{CommandVerifier, Verifier, VerifyResult};

/// 从一段 trace 中提取原始任务文本（读第一个 TaskStarted 事件）。
pub fn task_from_trace(events: &[TraceEvent]) -> Option<String> {
    events.iter().find_map(|e| match &e.kind {
        EventKind::TaskStarted { task } => Some(task.clone()),
        _ => None,
    })
}

/// 从 trace 构造 fork 任务。未指定 step 时保持旧行为；指定 step 时附带截至该
/// step 的紧凑上下文，供新模型从中间状态继续推理。
pub fn task_from_trace_at(events: &[TraceEvent], step: Option<u64>) -> anyhow::Result<String> {
    let task = task_from_trace(events).ok_or_else(|| {
        anyhow::anyhow!("trace has no TaskStarted event; re-run the task to enable fork")
    })?;
    let Some(step) = step else {
        return Ok(task);
    };
    if !events.iter().any(|e| e.step == step) {
        anyhow::bail!("trace has no step {step}; choose an existing step");
    }

    let context = events
        .iter()
        .filter(|e| e.step <= step)
        .map(summarize_trace_event_for_fork)
        .collect::<Vec<_>>()
        .join("\n");

    Ok(format!(
        "Original task:\n{task}\n\nFork context through step {step}:\n{context}\n\nContinue from this trace context. Re-evaluate the next action using the selected provider/model."
    ))
}

fn summarize_trace_event_for_fork(event: &TraceEvent) -> String {
    match &event.kind {
        EventKind::TaskStarted { task } => {
            format!("[{}] TASK {}", event.step, truncate_for_fork(task))
        }
        EventKind::Thought { text } => {
            format!("[{}] THOUGHT {}", event.step, truncate_for_fork(text))
        }
        EventKind::ModelRequest {
            model,
            prompt_tokens,
        } => format!(
            "[{}] MODEL_REQUEST model={model} prompt_tokens={prompt_tokens}",
            event.step
        ),
        EventKind::ModelResponse {
            model,
            prompt_tokens,
            completion_tokens,
            text,
        } => format!(
            "[{}] MODEL_RESPONSE model={model} tokens={prompt_tokens}+{completion_tokens} text={}",
            event.step,
            truncate_for_fork(text)
        ),
        EventKind::ToolCall { name, args } => {
            format!(
                "[{}] TOOL_CALL {name} {}",
                event.step,
                truncate_for_fork(args)
            )
        }
        EventKind::ToolResult { name, ok, output } => {
            format!(
                "[{}] TOOL_RESULT {name} ok={ok} {}",
                event.step,
                truncate_for_fork(output)
            )
        }
        EventKind::PolicyDecision {
            tool_name,
            operation,
            decision,
            reason,
        } => format!(
            "[{}] POLICY tool={tool_name} operation={operation} decision={decision} {}",
            event.step,
            truncate_for_fork(reason)
        ),
        EventKind::Diff { path, patch } => {
            format!("[{}] DIFF {path} {}", event.step, truncate_for_fork(patch))
        }
        EventKind::VerificationGate { passed, detail } => {
            format!(
                "[{}] VERIFICATION passed={passed} {}",
                event.step,
                truncate_for_fork(detail)
            )
        }
        EventKind::RouteDecision {
            from_model,
            to_model,
            reason,
        } => format!(
            "[{}] ROUTE {from_model} -> {to_model} {}",
            event.step,
            truncate_for_fork(reason)
        ),
        EventKind::Note { text } => {
            format!("[{}] NOTE {}", event.step, truncate_for_fork(text))
        }
    }
}

fn truncate_for_fork(text: &str) -> String {
    const MAX_CHARS: usize = 300;
    let mut out = String::new();
    for (i, ch) in text.chars().enumerate() {
        if i >= MAX_CHARS {
            out.push_str("...");
            return out;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_from_trace_at_includes_context_through_step() {
        let events = vec![
            TraceEvent {
                step: 0,
                ts_ms: 0,
                kind: EventKind::TaskStarted {
                    task: "build X".into(),
                },
            },
            TraceEvent {
                step: 1,
                ts_ms: 0,
                kind: EventKind::Thought {
                    text: "look around".into(),
                },
            },
            TraceEvent {
                step: 2,
                ts_ms: 0,
                kind: EventKind::ToolResult {
                    name: "read_file".into(),
                    ok: true,
                    output: "file body".into(),
                },
            },
        ];

        let task = task_from_trace_at(&events, Some(1)).unwrap();

        assert!(task.contains("Original task:\nbuild X"), "{task}");
        assert!(task.contains("Fork context through step 1"), "{task}");
        assert!(task.contains("[1] THOUGHT"), "{task}");
        assert!(!task.contains("file body"), "{task}");
    }

    #[test]
    fn task_from_trace_at_rejects_missing_step() {
        let events = vec![TraceEvent {
            step: 0,
            ts_ms: 0,
            kind: EventKind::TaskStarted {
                task: "build X".into(),
            },
        }];

        let err = task_from_trace_at(&events, Some(9)).unwrap_err();

        assert!(format!("{err}").contains("step 9"), "{err}");
    }
}
