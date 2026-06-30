use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

const TASK_QUEUE_PATH: &str = ".argus/tasks/queue.jsonl";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskRecord {
    pub id: String,
    pub text: String,
    pub status: String,
    pub created_ms: u128,
}

pub fn task_queue_path(root: &Path) -> PathBuf {
    root.join(TASK_QUEUE_PATH)
}

pub fn queue_task(root: &Path, text: &str) -> Result<TaskRecord> {
    let text = text.trim();
    if text.is_empty() {
        anyhow::bail!("task text must not be empty");
    }
    let created_ms = now_ms();
    let record = TaskRecord {
        id: format!("task-{created_ms}"),
        text: text.to_string(),
        status: "queued".into(),
        created_ms,
    };
    let path = task_queue_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(file, "{}", serde_json::to_string(&record)?)?;
    Ok(record)
}

pub fn list_tasks(root: &Path) -> Result<Vec<TaskRecord>> {
    let path = task_queue_path(root);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path)?;
    let mut tasks = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let record: TaskRecord = serde_json::from_str(line).map_err(|e| {
            anyhow::anyhow!(
                "invalid task queue line {} in {}: {e}",
                index + 1,
                path.display()
            )
        })?;
        tasks.push(record);
    }
    Ok(tasks)
}

pub fn latest_task(root: &Path) -> Result<Option<TaskRecord>> {
    Ok(list_tasks(root)?.into_iter().last())
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
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
            "arguscode-task-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn queue_list_and_latest_roundtrip() {
        let dir = temp_dir("roundtrip");
        std::fs::create_dir_all(&dir).unwrap();

        let first = queue_task(&dir, "fix tests").unwrap();
        let second = queue_task(&dir, "write docs").unwrap();
        let tasks = list_tasks(&dir).unwrap();

        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0], first);
        assert_eq!(latest_task(&dir).unwrap(), Some(second));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
