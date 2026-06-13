//! Argus 内核 —— 模型无关的 Provider 抽象与 Agent 主循环。

use argus_trace::{EventKind, TraceEvent};

pub mod agent;
pub(crate) mod anthropic;
pub mod approver;
pub mod cost;
pub mod eval;
pub mod mcp;
pub(crate) mod openai;
pub mod provider;
pub mod router;
pub mod tool;
pub mod types;
pub mod verifier;

pub use agent::Agent;
pub use anthropic::AnthropicProvider;
pub use openai::OpenAiProvider;
pub use approver::{Approver, AutoApprover};
pub use cost::estimate_cost;
pub use router::{run_with_escalation, RouteReport};
pub use eval::{run_suite, CaseResult, EvalCase, EvalSuite, SuiteReport};
pub use provider::{MockProvider, Provider};
pub use tool::{ReadFile, RunShell, Tool, WriteFile};
pub use verifier::{CommandVerifier, VerifyResult, Verifier};
pub use types::{
    CompletionRequest, CompletionResponse, Content, Message, Role, StopReason, ToolCall, ToolSpec,
    Usage,
};

/// 从一段 trace 中提取原始任务文本（读第一个 TaskStarted 事件）。
pub fn task_from_trace(events: &[TraceEvent]) -> Option<String> {
    events.iter().find_map(|e| match &e.kind {
        EventKind::TaskStarted { task } => Some(task.clone()),
        _ => None,
    })
}
