use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
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
    let status = run_git(root, ["status", "--short"])?;
    let stat = run_git(root, ["diff", "--stat"])?;
    let staged = run_git(root, ["diff", "--cached", "--stat"])?;
    let status = status.trim();
    let stat = stat.trim();
    let staged = staged.trim();

    let mut lines = Vec::new();
    lines.push("Change Review".to_string());
    lines.push("Pending changes".to_string());
    if status.is_empty() {
        lines.push("(clean)".into());
    } else {
        lines.extend(status.lines().take(18).map(|line| line.to_string()));
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
        assert!(review.contains("Next actions"), "{review}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn record_review_decision_appends_jsonl_decision() {
        let dir = temp_dir("decision");
        std::fs::create_dir_all(&dir).unwrap();

        let record = super::record_review_decision(&dir, "accepted", "ship it").unwrap();

        assert_eq!(record.decision, "accepted");
        assert_eq!(record.note, "ship it");
        let text = std::fs::read_to_string(dir.join(".argus/reviews/decisions.jsonl")).unwrap();
        assert!(text.contains("\"decision\":\"accepted\""), "{text}");
        assert!(text.contains("\"note\":\"ship it\""), "{text}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
