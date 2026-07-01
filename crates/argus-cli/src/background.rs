use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const BACKGROUND_RUN_PATH: &str = ".argus/cockpit/background-run.json";
const BACKGROUND_OUTPUT_PATH: &str = ".argus/cockpit/background-output.jsonl";
const BACKGROUND_CANCEL_PATH: &str = ".argus/cockpit/background-cancel.json";
static BACKGROUND_OUTPUT_LOCK: Mutex<()> = Mutex::new(());
static BACKGROUND_CANCEL_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackgroundRun {
    pub task_id: String,
    pub status: String,
    pub detail: String,
    pub trace: Option<PathBuf>,
    pub updated_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackgroundOutput {
    pub stream: String,
    pub text: String,
    pub created_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackgroundCancel {
    pub task_id: String,
    pub reason: String,
    pub requested_ms: u128,
}

pub fn background_run_path(root: &Path) -> PathBuf {
    root.join(BACKGROUND_RUN_PATH)
}

pub fn background_output_path(root: &Path) -> PathBuf {
    root.join(BACKGROUND_OUTPUT_PATH)
}

pub fn background_cancel_path(root: &Path) -> PathBuf {
    root.join(BACKGROUND_CANCEL_PATH)
}

pub fn record_background_run(
    root: &Path,
    task_id: &str,
    status: &str,
    detail: &str,
    trace: Option<PathBuf>,
) -> Result<BackgroundRun> {
    let record = BackgroundRun {
        task_id: normalize(task_id),
        status: normalize(status),
        detail: normalize(detail),
        trace,
        updated_ms: now_ms(),
    };
    let path = background_run_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(&record)?)?;
    Ok(record)
}

pub fn load_background_run(root: &Path) -> Result<Option<BackgroundRun>> {
    let path = background_run_path(root);
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&text)?))
}

pub fn append_background_output(root: &Path, stream: &str, text: &str) -> Result<BackgroundOutput> {
    let record = BackgroundOutput {
        stream: normalize(stream),
        text: normalize_output(text),
        created_ms: now_ms(),
    };
    let path = background_output_path(root);
    let _guard = BACKGROUND_OUTPUT_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("background output lock poisoned"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut line = serde_json::to_string(&record)?;
    line.push('\n');
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(line.as_bytes())?;
    Ok(record)
}

pub fn list_background_output(root: &Path) -> Result<Vec<BackgroundOutput>> {
    let path = background_output_path(root);
    let _guard = BACKGROUND_OUTPUT_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("background output lock poisoned"))?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path)?;
    let mut records = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let record: BackgroundOutput = serde_json::from_str(line).map_err(|e| {
            anyhow::anyhow!(
                "invalid background output line {} in {}: {e}",
                index + 1,
                path.display()
            )
        })?;
        records.push(record);
    }
    Ok(records)
}

pub fn clear_background_output(root: &Path) -> Result<()> {
    let path = background_output_path(root);
    let _guard = BACKGROUND_OUTPUT_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("background output lock poisoned"))?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

pub fn request_background_cancel(
    root: &Path,
    task_id: &str,
    reason: &str,
) -> Result<BackgroundCancel> {
    let record = BackgroundCancel {
        task_id: normalize(task_id),
        reason: normalize(reason),
        requested_ms: now_ms(),
    };
    let path = background_cancel_path(root);
    let _guard = BACKGROUND_CANCEL_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("background cancel lock poisoned"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(&record)?)?;
    Ok(record)
}

pub fn load_background_cancel(root: &Path) -> Result<Option<BackgroundCancel>> {
    let path = background_cancel_path(root);
    let _guard = BACKGROUND_CANCEL_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("background cancel lock poisoned"))?;
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&text)?))
}

pub fn background_cancel_requested(root: &Path, task_id: &str) -> Result<bool> {
    let task_id = normalize(task_id);
    Ok(load_background_cancel(root)?.is_some_and(|request| request.task_id == task_id))
}

pub fn clear_background_cancel(root: &Path) -> Result<()> {
    let path = background_cancel_path(root);
    let _guard = BACKGROUND_CANCEL_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("background cancel lock poisoned"))?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

fn normalize(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        "(none)".into()
    } else {
        value.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

fn normalize_output(value: &str) -> String {
    let value = value.trim_end_matches(['\r', '\n']);
    if value.is_empty() {
        "(empty)".into()
    } else {
        value.to_string()
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "argus-background-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn background_run_roundtrips_status_file() {
        let dir = temp_dir("roundtrip");
        std::fs::create_dir_all(&dir).unwrap();

        let record = record_background_run(
            &dir,
            " task-1 ",
            " running ",
            " waiting for harness ",
            Some(PathBuf::from(".argus/tasks/task-1.trace.jsonl")),
        )
        .unwrap();
        let loaded = load_background_run(&dir).unwrap().unwrap();

        assert_eq!(record.task_id, "task-1");
        assert_eq!(loaded.status, "running");
        assert_eq!(loaded.detail, "waiting for harness");
        assert_eq!(
            loaded.trace,
            Some(PathBuf::from(".argus/tasks/task-1.trace.jsonl"))
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn background_output_appends_jsonl_records() {
        let dir = temp_dir("output");
        std::fs::create_dir_all(&dir).unwrap();

        append_background_output(&dir, " stdout ", "first line\n").unwrap();
        append_background_output(&dir, " stderr ", "warning\n").unwrap();

        let records = list_background_output(&dir).unwrap();

        assert_eq!(records.len(), 2);
        assert_eq!(records[0].stream, "stdout");
        assert_eq!(records[0].text, "first line");
        assert_eq!(records[1].stream, "stderr");
        assert_eq!(records[1].text, "warning");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn background_output_clear_removes_old_records() {
        let dir = temp_dir("clear-output");
        std::fs::create_dir_all(&dir).unwrap();
        append_background_output(&dir, "stdout", "stale\n").unwrap();

        clear_background_output(&dir).unwrap();

        let records = list_background_output(&dir).unwrap();
        assert!(records.is_empty(), "{records:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn background_cancel_request_roundtrips_and_clears() {
        let dir = temp_dir("cancel");
        std::fs::create_dir_all(&dir).unwrap();

        request_background_cancel(&dir, " task-1 ", " user pressed stop ").unwrap();
        let request = load_background_cancel(&dir).unwrap().unwrap();

        assert_eq!(request.task_id, "task-1");
        assert_eq!(request.reason, "user pressed stop");
        assert!(background_cancel_requested(&dir, "task-1").unwrap());
        assert!(!background_cancel_requested(&dir, "task-2").unwrap());

        clear_background_cancel(&dir).unwrap();
        assert!(load_background_cancel(&dir).unwrap().is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
