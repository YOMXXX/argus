use argus_trace::{EventKind, TraceEvent};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

const PREVIEW_EVENT_LIMIT: usize = 12;
const SUMMARY_LIMIT: usize = 180;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TracePreview {
    pub target: String,
    pub headline: String,
    pub lines: Vec<String>,
}

impl TracePreview {
    pub fn empty() -> Self {
        Self {
            target: ".argus/trace.jsonl".into(),
            headline: "(no task trace yet)".into(),
            lines: Vec::new(),
        }
    }
}

pub fn load_trace_preview(root: &Path, trace_path: Option<&Path>) -> TracePreview {
    let Some(trace_path) = trace_path else {
        return TracePreview::empty();
    };

    let target = trace_path.display().to_string();
    let full_path = resolve_trace_path(root, trace_path);
    if !full_path.exists() {
        return TracePreview {
            target,
            headline: "(trace file not found)".into(),
            lines: Vec::new(),
        };
    }

    let events = match read_trace_preview_events(&full_path) {
        Ok(events) => events,
        Err(err) => {
            return TracePreview {
                target,
                headline: format!("could not read trace: {err}"),
                lines: Vec::new(),
            };
        }
    };

    if events.is_empty() {
        return TracePreview {
            target,
            headline: "(empty trace)".into(),
            lines: Vec::new(),
        };
    }

    let skipped = events.len().saturating_sub(PREVIEW_EVENT_LIMIT);
    let headline = if skipped == 0 {
        format!("{} events", events.len())
    } else {
        format!(
            "{} events, showing last {}",
            events.len(),
            PREVIEW_EVENT_LIMIT
        )
    };
    let lines = events
        .iter()
        .skip(skipped)
        .map(|event| format!("[{:>3}] {}", event.step, summarize_kind(&event.kind)))
        .collect();

    TracePreview {
        target,
        headline,
        lines,
    }
}

fn read_trace_preview_events(path: &Path) -> anyhow::Result<Vec<TraceEvent>> {
    let reader = BufReader::new(std::fs::File::open(path)?);
    let mut events = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<TraceEvent>(&line) {
            Ok(event) => events.push(event),
            Err(err) if err.is_eof() => break,
            Err(err) => return Err(err.into()),
        }
    }
    Ok(events)
}

fn resolve_trace_path(root: &Path, trace_path: &Path) -> PathBuf {
    if trace_path.is_absolute() {
        trace_path.to_path_buf()
    } else {
        root.join(trace_path)
    }
}

fn summarize_kind(kind: &EventKind) -> String {
    let summary = match kind {
        EventKind::TaskStarted { task } => format!("TASK     {task}"),
        EventKind::Thought { text } => format!("THOUGHT  {text}"),
        EventKind::ModelRequest {
            model,
            prompt_tokens,
        } => {
            format!("MODEL -> {model} ({prompt_tokens} prompt tokens)")
        }
        EventKind::ModelResponse {
            model,
            prompt_tokens,
            completion_tokens,
            text,
        } => {
            format!("MODEL <- {model} ({prompt_tokens}+{completion_tokens} tokens): {text}")
        }
        EventKind::ToolCall { name, args } => format!("TOOL ->  {name}({args})"),
        EventKind::ToolResult { name, ok, output } => {
            format!("TOOL <-  {name} ok={ok}: {output}")
        }
        EventKind::PolicyDecision {
            tool_name,
            operation,
            decision,
            reason,
        } => {
            format!("POLICY   {tool_name} operation={operation} decision={decision}: {reason}")
        }
        EventKind::Diff { path, .. } => format!("DIFF     {path}"),
        EventKind::VerificationGate { passed, detail } => {
            format!("GATE     passed={passed}: {detail}")
        }
        EventKind::RouteDecision {
            from_model,
            to_model,
            reason,
        } => {
            format!("ROUTE    {from_model} -> {to_model}: {reason}")
        }
        EventKind::Note { text } => format!("NOTE     {text}"),
    };
    truncate_chars(&summary, SUMMARY_LIMIT)
}

fn truncate_chars(text: &str, max: usize) -> String {
    let mut out = text.chars().take(max).collect::<String>();
    if text.chars().count() > max {
        out.push_str("...");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_trace::{EventKind, TraceWriter};
    use std::io::Write;

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "arguscode-trace-view-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn load_trace_preview_summarizes_recent_events() {
        let dir = temp_dir("summary");
        let trace_path = dir.join(".argus/tasks/task-1.trace.jsonl");
        std::fs::create_dir_all(trace_path.parent().unwrap()).unwrap();
        let mut writer = TraceWriter::create(&trace_path).unwrap();
        writer
            .record(EventKind::TaskStarted {
                task: "build timeline".into(),
            })
            .unwrap();
        writer
            .record(EventKind::VerificationGate {
                passed: true,
                detail: "cargo test passed".into(),
            })
            .unwrap();

        let preview = load_trace_preview(&dir, Some(Path::new(".argus/tasks/task-1.trace.jsonl")));

        assert_eq!(preview.target, ".argus/tasks/task-1.trace.jsonl");
        assert_eq!(preview.headline, "2 events");
        assert!(
            preview
                .lines
                .iter()
                .any(|line| line.contains("TASK") && line.contains("build timeline")),
            "{preview:?}"
        );
        assert!(
            preview
                .lines
                .iter()
                .any(|line| line.contains("GATE") && line.contains("cargo test passed")),
            "{preview:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_trace_preview_keeps_completed_events_when_live_tail_is_partial() {
        let dir = temp_dir("partial");
        let trace_path = dir.join(".argus/tasks/task-1.trace.jsonl");
        std::fs::create_dir_all(trace_path.parent().unwrap()).unwrap();
        let mut writer = TraceWriter::create(&trace_path).unwrap();
        writer
            .record(EventKind::TaskStarted {
                task: "stream trace".into(),
            })
            .unwrap();
        std::fs::OpenOptions::new()
            .append(true)
            .open(&trace_path)
            .unwrap()
            .write_all(br#"{"step":1,"ts_ms":0,"kind":{"type":"tool_call""#)
            .unwrap();

        let preview = load_trace_preview(&dir, Some(Path::new(".argus/tasks/task-1.trace.jsonl")));

        assert_eq!(preview.headline, "1 events");
        assert!(
            preview
                .lines
                .iter()
                .any(|line| line.contains("TASK") && line.contains("stream trace")),
            "{preview:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
