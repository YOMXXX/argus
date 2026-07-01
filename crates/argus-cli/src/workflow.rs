use crate::review::{list_review_decisions, ReviewDecision};
use crate::sessions::{list_sessions, SessionRecord};
use crate::tasks::{list_tasks, TaskRecord};
use crate::workspace_filter::reviewable_status_lines;
use anyhow::Result;
use std::path::Path;
use std::process::Command;

pub fn load_workflow_status(root: &Path, verify_commands: &[String]) -> Result<String> {
    let tasks = list_tasks(root)?;
    let sessions = list_sessions(root)?;
    let decisions = list_review_decisions(root)?;
    let git_status = run_git(root, ["status", "--short", "--untracked-files=all"])?;
    Ok(render_workflow_status(
        &tasks,
        &sessions,
        &decisions,
        git_status.trim(),
        verify_commands,
    ))
}

fn render_workflow_status(
    tasks: &[TaskRecord],
    sessions: &[SessionRecord],
    decisions: &[ReviewDecision],
    git_status: &str,
    verify_commands: &[String],
) -> String {
    let latest_task = tasks.last();
    let resumable = tasks
        .iter()
        .rev()
        .find(|task| task.status != "done" && task.status != "canceled");
    let latest_session = sessions.last();
    let latest_decision = latest_fresh_decision(decisions, latest_session);
    let changed_paths = reviewable_status_lines(git_status).count();
    let dirty = changed_paths > 0;
    let has_verify = !verify_commands.is_empty();
    let rework_open = resumable.is_some_and(|task| task.text.starts_with("Review follow-up:"));
    let run_done = latest_session.is_some();
    let verify_done = latest_session
        .is_some_and(|session| matches!(session.status.as_str(), "done" | "passed" | "success"));
    let review_done = latest_decision
        .map(|decision| decision.decision == "accepted")
        .unwrap_or(false);
    let phase = phase_label(
        resumable,
        latest_session,
        latest_decision,
        dirty,
        has_verify,
    );
    let next = next_action(
        resumable,
        latest_session,
        latest_decision,
        dirty,
        has_verify,
    );

    let mut lines = Vec::new();
    lines.push("Workflow Status".to_string());
    lines.push(format!("Phase: {phase}"));
    lines.push(format!(
        "Flow: {} Queue -> {} Run -> {} Verify -> {} Review -> {} Rework",
        mark(latest_task.is_some(), resumable.is_some()),
        mark(
            run_done,
            resumable.is_none() && latest_task.is_some() && !run_done
        ),
        mark(
            verify_done,
            run_done && !verify_done && latest_session.is_some_and(|s| s.status == "failed")
        ),
        mark(review_done, dirty && !review_done && !rework_open),
        mark(!rework_open && review_done, rework_open)
    ));
    lines.push(format!(
        "Workspace: {}",
        if dirty {
            format!("{changed_paths} changed path(s)")
        } else {
            "clean".into()
        }
    ));
    lines.push(format!(
        "Verify: {}",
        if has_verify {
            format!("{} command(s)", verify_commands.len())
        } else {
            "not configured".into()
        }
    ));
    lines.push(format!("Next: {next}"));
    if let Some(task) = latest_task {
        lines.push(format!(
            "Latest task: [{}] {}",
            task.status,
            compact(&task.text, 84)
        ));
    } else {
        lines.push("Latest task: (none)".into());
    }
    if let Some(session) = latest_session {
        lines.push(format!(
            "Latest run: [{}] {}",
            session.status,
            compact(&session.task_text, 84)
        ));
    } else {
        lines.push("Latest run: (none)".into());
    }
    if let Some(decision) = latest_decision {
        lines.push(format!(
            "Review: {} - {}",
            decision.decision,
            compact(&decision.note, 72)
        ));
    } else if dirty {
        lines.push("Review: pending".into());
    } else {
        lines.push("Review: not needed".into());
    }
    lines.join("\n")
}

fn phase_label(
    resumable: Option<&TaskRecord>,
    latest_session: Option<&SessionRecord>,
    latest_decision: Option<&ReviewDecision>,
    dirty: bool,
    has_verify: bool,
) -> &'static str {
    if !has_verify {
        return "Configure verification";
    }
    if let Some(task) = resumable {
        return if task.text.starts_with("Review follow-up:") {
            "Rework queued"
        } else {
            "Task queued"
        };
    }
    if let Some(session) = latest_session {
        if session.status == "failed" {
            return "Repair needed";
        }
        if dirty {
            return match latest_decision.map(|decision| decision.decision.as_str()) {
                Some("accepted") => "Accepted",
                Some("rework") => "Rework planned",
                _ => "Review needed",
            };
        }
        return "Clean after run";
    }
    if dirty {
        "Review needed"
    } else {
        "Ready"
    }
}

fn next_action(
    resumable: Option<&TaskRecord>,
    latest_session: Option<&SessionRecord>,
    latest_decision: Option<&ReviewDecision>,
    dirty: bool,
    has_verify: bool,
) -> &'static str {
    if !has_verify {
        return "Add verify commands in .argus/config.toml, then /verify";
    }
    if resumable.is_some() {
        return "/run or /route-run";
    }
    if latest_session.is_some_and(|session| session.status == "failed") {
        return "/retry <task-id> or /rework <task>";
    }
    if dirty {
        return match latest_decision.map(|decision| decision.decision.as_str()) {
            Some("accepted") => "Ready to commit/push outside Argus, or /new for another task",
            Some("rework") => "/run the queued follow-up, or /rework <task>",
            _ => "/review, then /accept <note> or /rework <task>",
        };
    }
    "Type a task and press Enter"
}

fn latest_fresh_decision<'a>(
    decisions: &'a [ReviewDecision],
    latest_session: Option<&SessionRecord>,
) -> Option<&'a ReviewDecision> {
    let latest = decisions.last()?;
    let session = latest_session?;
    if latest.created_ms < session.created_ms {
        return None;
    }
    Some(latest)
}

fn mark(done: bool, active: bool) -> &'static str {
    if done {
        "[x]"
    } else if active {
        "[>]"
    } else {
        "[ ]"
    }
}

fn compact(value: &str, max_chars: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }
    let mut out = normalized
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    out.push_str("...");
    out
}

fn run_git<const N: usize>(root: &Path, args: [&str; N]) -> Result<String> {
    let output = Command::new("git").args(args).current_dir(root).output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Ok(String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cockpit::append_cockpit_event;
    use crate::review::record_review_decision;
    use crate::sessions::append_session;
    use crate::tasks::{queue_task, update_task_status};
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "argus-workflow-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    fn init_git(dir: &Path) {
        std::process::Command::new("git")
            .arg("init")
            .current_dir(dir)
            .output()
            .unwrap();
    }

    #[test]
    fn workflow_reports_missing_verify_before_work_starts() {
        let dir = temp_dir("missing-verify");
        std::fs::create_dir_all(&dir).unwrap();
        init_git(&dir);

        let status = load_workflow_status(&dir, &[]).unwrap();

        assert!(status.contains("Phase: Configure verification"), "{status}");
        assert!(status.contains("Verify: not configured"), "{status}");
        assert!(status.contains("Next: Add verify commands"), "{status}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn workflow_reports_queued_task_as_next_run() {
        let dir = temp_dir("queued");
        std::fs::create_dir_all(&dir).unwrap();
        init_git(&dir);
        queue_task(&dir, "fix parser").unwrap();

        let status = load_workflow_status(&dir, &["cargo test".into()]).unwrap();

        assert!(status.contains("Phase: Task queued"), "{status}");
        assert!(
            status.contains("Latest task: [queued] fix parser"),
            "{status}"
        );
        assert!(status.contains("Next: /run or /route-run"), "{status}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn workflow_reports_review_needed_after_completed_dirty_run() {
        let dir = temp_dir("review-needed");
        std::fs::create_dir_all(&dir).unwrap();
        init_git(&dir);
        let task = queue_task(&dir, "ship feature").unwrap();
        update_task_status(&dir, &task.id, "done").unwrap();
        append_session(
            &dir,
            &task.id,
            &task.text,
            "done",
            ".argus/tasks/task.trace.jsonl",
        )
        .unwrap();
        std::fs::write(dir.join("feature.txt"), "changed\n").unwrap();

        let status = load_workflow_status(&dir, &["cargo test".into()]).unwrap();

        assert!(status.contains("Phase: Review needed"), "{status}");
        assert!(status.contains("Workspace: 1 changed path(s)"), "{status}");
        assert!(status.contains("Review: pending"), "{status}");
        assert!(
            status.contains("Next: /review, then /accept <note> or /rework <task>"),
            "{status}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn workflow_reports_fresh_acceptance_for_latest_run() {
        let dir = temp_dir("accepted");
        std::fs::create_dir_all(&dir).unwrap();
        init_git(&dir);
        let task = queue_task(&dir, "ship feature").unwrap();
        update_task_status(&dir, &task.id, "done").unwrap();
        append_session(
            &dir,
            &task.id,
            &task.text,
            "done",
            ".argus/tasks/task.trace.jsonl",
        )
        .unwrap();
        std::fs::write(dir.join("feature.txt"), "changed\n").unwrap();
        record_review_decision(&dir, "accepted", "ready to ship").unwrap();

        let status = load_workflow_status(&dir, &["cargo test".into()]).unwrap();

        assert!(status.contains("Phase: Accepted"), "{status}");
        assert!(
            status.contains("Review: accepted - ready to ship"),
            "{status}"
        );
        assert!(status.contains("Next: Ready to commit/push"), "{status}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn workflow_ignores_argus_runtime_metadata_as_workspace_changes() {
        let dir = temp_dir("runtime-metadata");
        std::fs::create_dir_all(&dir).unwrap();
        init_git(&dir);
        queue_task(&dir, "metadata only").unwrap();
        append_cockpit_event(&dir, "run", "metadata only", "/review").unwrap();

        let status = load_workflow_status(&dir, &["cargo test".into()]).unwrap();

        assert!(status.contains("Workspace: clean"), "{status}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn workflow_ignores_review_decisions_without_a_run_anchor() {
        let dir = temp_dir("unanchored-review");
        std::fs::create_dir_all(&dir).unwrap();
        init_git(&dir);
        std::fs::write(dir.join("manual.txt"), "manual\n").unwrap();
        record_review_decision(&dir, "accepted", "old manual decision").unwrap();

        let status = load_workflow_status(&dir, &["cargo test".into()]).unwrap();

        assert!(status.contains("Phase: Review needed"), "{status}");
        assert!(status.contains("Review: pending"), "{status}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
