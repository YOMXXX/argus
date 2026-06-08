//! Argus 黑匣子 Trace —— 开放 JSONL 事件日志。

use serde::{Deserialize, Serialize};

/// 一条 Trace 事件 —— Argus 黑匣子的原子单位。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceEvent {
    /// 单调递增的步骤序号，时间旅行 fork 的锚点。
    pub step: u64,
    /// Unix 毫秒时间戳。
    pub ts_ms: u64,
    /// 事件内容。
    pub kind: EventKind,
}

/// Trace 事件类型，覆盖 agent 主循环每一种可观测动作。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    Thought { text: String },
    ModelRequest { model: String, prompt_tokens: u64 },
    ModelResponse { model: String, completion_tokens: u64, text: String },
    ToolCall {
        name: String,
        /// 工具参数，JSON 编码的字符串。
        args: String,
    },
    ToolResult { name: String, ok: bool, output: String },
    Diff { path: String, patch: String },
    VerificationGate { passed: bool, detail: String },
    Note { text: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_serializes_roundtrip() {
        let event = TraceEvent {
            step: 3,
            ts_ms: 1234,
            kind: EventKind::Thought { text: "hello".into() },
        };
        let json = serde_json::to_string(&event).unwrap();
        let back: TraceEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);
    }

    #[test]
    fn model_response_roundtrip() {
        let event = TraceEvent {
            step: 7,
            ts_ms: 5678,
            kind: EventKind::ModelResponse {
                model: "claude".into(),
                completion_tokens: 42,
                text: "done".into(),
            },
        };
        let json = serde_json::to_string(&event).unwrap();
        let back: TraceEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);
    }

    #[test]
    fn verification_gate_roundtrip() {
        let event = TraceEvent {
            step: 9,
            ts_ms: 9012,
            kind: EventKind::VerificationGate {
                passed: false,
                detail: "tests failed".into(),
            },
        };
        let json = serde_json::to_string(&event).unwrap();
        let back: TraceEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);
    }

    #[test]
    fn tool_call_with_json_args_roundtrip() {
        let event = TraceEvent {
            step: 11,
            ts_ms: 3456,
            kind: EventKind::ToolCall {
                name: "shell".into(),
                args: r#"{"cmd":"ls"}"#.into(),
            },
        };
        let json = serde_json::to_string(&event).unwrap();
        let back: TraceEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);
    }
}
