use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const SESSION_HISTORY_PATH: &str = ".argus/sessions/history.jsonl";
static SESSION_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionRecord {
    pub id: String,
    pub task_id: String,
    pub task_text: String,
    pub status: String,
    pub trace: PathBuf,
    pub created_ms: u128,
}

pub fn session_history_path(root: &Path) -> PathBuf {
    root.join(SESSION_HISTORY_PATH)
}

pub fn append_session(
    root: &Path,
    task_id: &str,
    task_text: &str,
    status: &str,
    trace: impl Into<PathBuf>,
) -> Result<SessionRecord> {
    let created_ms = now_ms();
    let record = SessionRecord {
        id: next_session_id(created_ms),
        task_id: task_id.to_string(),
        task_text: task_text.to_string(),
        status: status.to_string(),
        trace: trace.into(),
        created_ms,
    };
    let path = session_history_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(file, "{}", serde_json::to_string(&record)?)?;
    Ok(record)
}

pub fn list_sessions(root: &Path) -> Result<Vec<SessionRecord>> {
    let path = session_history_path(root);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path)?;
    let mut sessions = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let record: SessionRecord = serde_json::from_str(line).map_err(|e| {
            anyhow::anyhow!(
                "invalid session history line {} in {}: {e}",
                index + 1,
                path.display()
            )
        })?;
        sessions.push(record);
    }
    Ok(sessions)
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn next_session_id(created_ms: u128) -> String {
    let sequence = SESSION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("session-{created_ms}-{}-{sequence}", std::process::id())
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
            "arguscode-session-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn append_and_list_sessions_roundtrip() {
        let dir = temp_dir("roundtrip");
        std::fs::create_dir_all(&dir).unwrap();

        let session = append_session(
            &dir,
            "task-1",
            "fix tests",
            "done",
            ".argus/tasks/task-1.trace.jsonl",
        )
        .unwrap();

        assert_eq!(list_sessions(&dir).unwrap(), vec![session]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
