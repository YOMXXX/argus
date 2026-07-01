use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const BACKGROUND_RUN_PATH: &str = ".argus/cockpit/background-run.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackgroundRun {
    pub task_id: String,
    pub status: String,
    pub detail: String,
    pub trace: Option<PathBuf>,
    pub updated_ms: u128,
}

pub fn background_run_path(root: &Path) -> PathBuf {
    root.join(BACKGROUND_RUN_PATH)
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

fn normalize(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        "(none)".into()
    } else {
        value.split_whitespace().collect::<Vec<_>>().join(" ")
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
}
