//! Argus 黑匣子 Trace —— 开放 JSONL 事件日志。

use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

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

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// append-only 的 Trace 写入器，输出开放 JSONL。
pub struct TraceWriter {
    file: File,
    next_step: u64,
}

impl TraceWriter {
    /// 在 `path` 创建/打开 trace（append 模式，父目录由调用方负责创建）。
    /// 若文件已存在且含事件，next_step 对齐到已有事件数，保证 step 单调递增续接。
    pub fn create<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let next_step = if path.as_ref().exists() {
            BufReader::new(File::open(path.as_ref())?)
                .lines()
                .filter(|l| l.as_ref().map(|s| !s.trim().is_empty()).unwrap_or(false))
                .count() as u64
        } else {
            0
        };
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self { file, next_step })
    }

    /// 记录一个事件：自动分配 step 与时间戳，写入一行 JSON。
    pub fn record(&mut self, kind: EventKind) -> anyhow::Result<TraceEvent> {
        let event = TraceEvent { step: self.next_step, ts_ms: now_ms(), kind };
        self.next_step += 1;
        let line = serde_json::to_string(&event)?;
        writeln!(self.file, "{line}")?;
        self.file.flush()?;
        Ok(event)
    }

    /// 已记录的事件数（u64，与 step 序号同类型）。
    pub fn len(&self) -> u64 {
        self.next_step
    }

    pub fn is_empty(&self) -> bool {
        self.next_step == 0
    }
}

/// 从 JSONL 文件读回完整 trace。
/// 遇到无法解析的行会立即返回 Err；如需容错跳过损坏行，请在上层处理。
pub fn read_trace<P: AsRef<Path>>(path: P) -> anyhow::Result<Vec<TraceEvent>> {
    let reader = BufReader::new(File::open(path)?);
    let mut events = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        events.push(serde_json::from_str::<TraceEvent>(&line)?);
    }
    Ok(events)
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

    fn tmp_path(tag: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("argus-trace-test-{}-{}.jsonl", std::process::id(), tag));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn writer_assigns_increasing_steps_and_reads_back() {
        let path = tmp_path("rw");
        let mut w = TraceWriter::create(&path).unwrap();
        let e0 = w.record(EventKind::Thought { text: "a".into() }).unwrap();
        let e1 = w.record(EventKind::Note { text: "b".into() }).unwrap();
        assert_eq!(e0.step, 0);
        assert_eq!(e1.step, 1);
        assert_eq!(w.len(), 2);
        drop(w);

        let events = read_trace(&path).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, EventKind::Thought { text: "a".into() });
        assert_eq!(events[1].kind, EventKind::Note { text: "b".into() });
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn writer_resumes_step_on_existing_trace() {
        let path = tmp_path("resume");
        {
            let mut w = TraceWriter::create(&path).unwrap();
            w.record(EventKind::Note { text: "first".into() }).unwrap(); // step 0
        }
        let mut w = TraceWriter::create(&path).unwrap();
        let e = w.record(EventKind::Note { text: "second".into() }).unwrap();
        assert_eq!(e.step, 1);
        let events = read_trace(&path).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].step, 0);
        assert_eq!(events[1].step, 1);
        let _ = std::fs::remove_file(&path);
    }
}
