use crate::workspace_filter::reviewable_status_lines;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const DECISIONS_FILE: &str = ".argus/reviews/decisions.jsonl";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewDecision {
    pub decision: String,
    pub note: String,
    pub created_ms: u128,
}

pub fn load_change_review(root: &Path) -> Result<String> {
    let status = run_git(root, ["status", "--short", "--untracked-files=all"])?;
    let stat = run_git(root, ["diff", "--stat"])?;
    let staged = run_git(root, ["diff", "--cached", "--stat"])?;
    let status = reviewable_status_lines(&status)
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    let stat = stat.trim();
    let staged = staged.trim();

    let mut lines = Vec::new();
    lines.push("Change Review".to_string());
    lines.push("Pending changes".to_string());
    if status.is_empty() {
        lines.push("(clean)".into());
    } else {
        lines.extend(status.iter().take(18).cloned());
        lines.push("".into());
        lines.push("Changed files".into());
        for line in status.iter().take(18) {
            lines.push(format_change_file(line));
        }
    }
    if !stat.is_empty() {
        lines.push("".into());
        lines.push("Unstaged diff stat".into());
        lines.extend(stat.lines().take(12).map(|line| line.to_string()));
    }
    if !staged.is_empty() {
        lines.push("".into());
        lines.push("Staged diff stat".into());
        lines.extend(staged.lines().take(12).map(|line| line.to_string()));
    }
    lines.push("".into());
    lines.push("Next actions".into());
    lines.push("- /verify to rerun the gate".into());
    lines.push("- /accept <note> to record acceptance".into());
    lines.push("- /rework <task> to queue a follow-up".into());
    lines.push("- /rollback to restore the last checkpoint".into());
    Ok(lines.join("\n"))
}

fn format_change_file(status_line: &str) -> String {
    let code = status_line.get(..2).unwrap_or(status_line).trim();
    let path = status_line.get(3..).unwrap_or(status_line).trim();
    let code = if code.is_empty() { "modified" } else { code };
    format!("- {code:<2} {path}")
}

pub fn record_review_decision(root: &Path, decision: &str, note: &str) -> Result<ReviewDecision> {
    let record = ReviewDecision {
        decision: normalize(decision),
        note: normalize(note),
        created_ms: now_ms(),
    };
    let path = root.join(DECISIONS_FILE);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut line = serde_json::to_string(&record)?;
    line.push('\n');
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(line.as_bytes())?;
    Ok(record)
}

pub fn review_decisions_path(root: &Path) -> PathBuf {
    root.join(DECISIONS_FILE)
}

pub fn list_review_decisions(root: &Path) -> Result<Vec<ReviewDecision>> {
    let path = review_decisions_path(root);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path)?;
    let mut records = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let record: ReviewDecision = serde_json::from_str(line).map_err(|e| {
            anyhow::anyhow!(
                "invalid review decision line {} in {}: {e}",
                index + 1,
                path.display()
            )
        })?;
        records.push(record);
    }
    Ok(records)
}

fn run_git<const N: usize>(root: &Path, args: [&str; N]) -> Result<String> {
    let output = Command::new("git").args(args).current_dir(root).output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Ok(String::from_utf8_lossy(&output.stderr).to_string())
    }
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
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "argus-review-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn load_change_review_summarizes_pending_git_changes() {
        let dir = temp_dir("summary");
        std::fs::create_dir_all(&dir).unwrap();
        std::process::Command::new("git")
            .arg("init")
            .current_dir(&dir)
            .output()
            .unwrap();
        std::fs::write(dir.join("new-file.txt"), "hello\n").unwrap();

        let review = super::load_change_review(&dir).unwrap();

        assert!(review.contains("Change Review"), "{review}");
        assert!(review.contains("Pending changes"), "{review}");
        assert!(review.contains("?? new-file.txt"), "{review}");
        assert!(review.contains("Changed files"), "{review}");
        assert!(review.contains("- ?? new-file.txt"), "{review}");
        assert!(review.contains("Next actions"), "{review}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_change_review_ignores_argus_runtime_metadata() {
        let dir = temp_dir("runtime");
        std::fs::create_dir_all(dir.join(".argus/cockpit")).unwrap();
        std::fs::create_dir_all(dir.join(".argus/tasks")).unwrap();
        std::process::Command::new("git")
            .arg("init")
            .current_dir(&dir)
            .output()
            .unwrap();
        std::fs::write(dir.join(".argus/cockpit/events.jsonl"), "{}\n").unwrap();
        std::fs::write(dir.join(".argus/tasks/queue.jsonl"), "{}\n").unwrap();
        std::fs::write(dir.join("real-change.txt"), "hello\n").unwrap();

        let review = super::load_change_review(&dir).unwrap();

        assert!(review.contains("real-change.txt"), "{review}");
        assert!(!review.contains(".argus/cockpit"), "{review}");
        assert!(!review.contains(".argus/tasks"), "{review}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn record_review_decision_appends_jsonl_decision() {
        let dir = temp_dir("decision");
        std::fs::create_dir_all(&dir).unwrap();

        let record = super::record_review_decision(&dir, "accepted", "ship it").unwrap();

        assert_eq!(record.decision, "accepted");
        assert_eq!(record.note, "ship it");
        let text = std::fs::read_to_string(super::review_decisions_path(&dir)).unwrap();
        assert!(text.contains("\"decision\":\"accepted\""), "{text}");
        assert!(text.contains("\"note\":\"ship it\""), "{text}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_review_decisions_reads_jsonl_records() {
        let dir = temp_dir("list");
        std::fs::create_dir_all(&dir).unwrap();

        super::record_review_decision(&dir, "accepted", "ship it").unwrap();
        super::record_review_decision(&dir, "rework", "tighten tests").unwrap();

        let records = super::list_review_decisions(&dir).unwrap();

        assert_eq!(records.len(), 2);
        assert_eq!(records[0].decision, "accepted");
        assert_eq!(records[1].decision, "rework");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
