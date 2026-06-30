use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const TASK_QUEUE_PATH: &str = ".argus/tasks/queue.jsonl";
static TASK_SEQUENCE: AtomicU64 = AtomicU64::new(1);

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
        id: next_task_id(created_ms),
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

pub fn update_task_status(root: &Path, id: &str, status: &str) -> Result<Option<TaskRecord>> {
    let mut tasks = list_tasks(root)?;
    let mut updated = None;
    for task in &mut tasks {
        if task.id == id {
            task.status = status.to_string();
            updated = Some(task.clone());
            break;
        }
    }
    if updated.is_some() {
        write_tasks(root, &tasks)?;
    }
    Ok(updated)
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

pub fn latest_resumable_task(root: &Path) -> Result<Option<TaskRecord>> {
    Ok(list_tasks(root)?
        .into_iter()
        .rev()
        .find(|task| task.status != "done"))
}

fn write_tasks(root: &Path, tasks: &[TaskRecord]) -> Result<()> {
    let path = task_queue_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)?;
    for task in tasks {
        writeln!(file, "{}", serde_json::to_string(task)?)?;
    }
    Ok(())
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn next_task_id(created_ms: u128) -> String {
    let sequence = TASK_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("task-{created_ms}-{}-{sequence}", std::process::id())
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

    #[test]
    fn queued_task_ids_are_unique_for_bursts() {
        let dir = temp_dir("burst");
        std::fs::create_dir_all(&dir).unwrap();

        for i in 0..64 {
            queue_task(&dir, &format!("task {i}")).unwrap();
        }

        let tasks = list_tasks(&dir).unwrap();
        let ids = tasks
            .iter()
            .map(|task| task.id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(ids.len(), tasks.len(), "tasks: {tasks:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn update_status_rewrites_matching_task() {
        let dir = temp_dir("status");
        std::fs::create_dir_all(&dir).unwrap();

        let task = queue_task(&dir, "ship status").unwrap();
        let updated = update_task_status(&dir, &task.id, "done").unwrap();

        assert_eq!(updated.as_ref().unwrap().status, "done");
        assert_eq!(list_tasks(&dir).unwrap()[0].status, "done");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn latest_resumable_skips_done_tasks() {
        let dir = temp_dir("resumable");
        std::fs::create_dir_all(&dir).unwrap();

        let older = queue_task(&dir, "still queued").unwrap();
        let newer = queue_task(&dir, "already done").unwrap();
        update_task_status(&dir, &newer.id, "done").unwrap();

        assert_eq!(latest_resumable_task(&dir).unwrap(), Some(older));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
