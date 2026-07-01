use crate::workspace_filter::{is_argus_runtime_status_line, reviewable_status_lines};
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
    let numstat = run_git(root, ["diff", "--numstat"])?;
    let staged_numstat = run_git(root, ["diff", "--cached", "--numstat"])?;
    let diff = run_git(root, ["diff", "--unified=0"])?;
    let staged_diff = run_git(root, ["diff", "--cached", "--unified=0"])?;
    let context_diff = run_git(root, ["diff", "--unified=3"])?;
    let staged_context_diff = run_git(root, ["diff", "--cached", "--unified=3"])?;
    let status = reviewable_status_lines(&status)
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    let stat = stat.trim();
    let staged = staged.trim();
    let summary = patch_summary(&status, &numstat, &staged_numstat, &diff, &staged_diff);

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
    if !status.is_empty() {
        lines.push("".into());
        lines.push("Patch Summary".into());
        lines.extend(render_patch_summary(&summary));
        let risk_hints = review_risk_hints(&status, &summary, &context_diff, &staged_context_diff);
        if !risk_hints.is_empty() {
            lines.push("".into());
            lines.push("Risk Hints".into());
            lines.extend(risk_hints);
        }
        let hunks = patch_hunks(&diff, "unstaged")
            .into_iter()
            .chain(patch_hunks(&staged_diff, "staged"))
            .collect::<Vec<_>>();
        if !hunks.is_empty() {
            lines.push("".into());
            lines.push("Patch Hunks".into());
            lines.extend(render_patch_hunks(&hunks));
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct PatchSummary {
    files: usize,
    hunks: usize,
    insertions: usize,
    deletions: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PatchHunk {
    stage: String,
    path: String,
    header: String,
    insertions: usize,
    deletions: usize,
}

fn patch_summary(
    status: &[String],
    numstat: &str,
    staged_numstat: &str,
    diff: &str,
    staged_diff: &str,
) -> PatchSummary {
    let mut summary = PatchSummary {
        files: status.len(),
        ..PatchSummary::default()
    };
    add_numstat(&mut summary, numstat);
    add_numstat(&mut summary, staged_numstat);
    summary.hunks += count_reviewable_hunks(diff);
    summary.hunks += count_reviewable_hunks(staged_diff);
    summary
}

fn render_patch_summary(summary: &PatchSummary) -> Vec<String> {
    vec![
        format!("- reviewable files: {}", summary.files),
        format!("- hunks: {}", summary.hunks),
        format!("- insertions: {}", summary.insertions),
        format!("- deletions: {}", summary.deletions),
    ]
}

fn render_patch_hunks(hunks: &[PatchHunk]) -> Vec<String> {
    const MAX_RENDERED_HUNKS: usize = 12;
    let mut lines = hunks
        .iter()
        .take(MAX_RENDERED_HUNKS)
        .map(|hunk| {
            format!(
                "- {} {} {} (+{} -{})",
                hunk.stage, hunk.path, hunk.header, hunk.insertions, hunk.deletions
            )
        })
        .collect::<Vec<_>>();
    if hunks.len() > MAX_RENDERED_HUNKS {
        lines.push(format!(
            "- ... {} more hunks not shown",
            hunks.len() - MAX_RENDERED_HUNKS
        ));
    }
    lines
}

fn patch_hunks(diff: &str, stage: &str) -> Vec<PatchHunk> {
    let mut hunks = Vec::new();
    let mut current_path = String::new();
    let mut current_hunk: Option<PatchHunk> = None;

    for line in diff.lines() {
        if let Some(path) = parse_diff_file_path(line) {
            push_hunk(&mut hunks, current_hunk.take());
            current_path = path.to_string();
            continue;
        }
        if line.starts_with("@@ ") {
            push_hunk(&mut hunks, current_hunk.take());
            if !current_path.is_empty() && !is_argus_runtime_status_line(&current_path) {
                current_hunk = Some(PatchHunk {
                    stage: stage.to_string(),
                    path: current_path.clone(),
                    header: compact_hunk_header(line),
                    insertions: 0,
                    deletions: 0,
                });
            }
            continue;
        }
        if let Some(hunk) = current_hunk.as_mut() {
            if line.starts_with('+') && !line.starts_with("+++") {
                hunk.insertions += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                hunk.deletions += 1;
            }
        }
    }
    push_hunk(&mut hunks, current_hunk);
    hunks
}

fn push_hunk(hunks: &mut Vec<PatchHunk>, hunk: Option<PatchHunk>) {
    if let Some(hunk) = hunk {
        hunks.push(hunk);
    }
}

fn parse_diff_file_path(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("diff --git a/")?;
    let (_, path) = rest.rsplit_once(" b/")?;
    Some(path)
}

fn compact_hunk_header(line: &str) -> String {
    let mut parts = line.split("@@");
    let _ = parts.next();
    if let Some(range) = parts.next() {
        format!("@@{range}@@")
    } else {
        line.to_string()
    }
}

fn review_risk_hints(
    status: &[String],
    summary: &PatchSummary,
    diff: &str,
    staged_diff: &str,
) -> Vec<String> {
    let mut hints = Vec::new();
    let mut has_source_change = false;
    let mut has_test_change =
        diff_changes_inline_tests(diff) || diff_changes_inline_tests(staged_diff);

    for line in status {
        let code = status_code(line);
        let path = status_path(line);
        if path.is_empty() {
            continue;
        }
        if code.contains('D') {
            push_unique(&mut hints, format!("- deleted file: {path}"));
        }
        if is_critical_project_file(path) {
            push_unique(&mut hints, format!("- critical project file: {path}"));
        }
        if is_test_path(path) {
            has_test_change = true;
        } else if is_source_path(path) {
            has_source_change = true;
        }
    }

    let changed_lines = summary.insertions + summary.deletions;
    if changed_lines >= 250 || summary.hunks >= 12 {
        hints.push(format!(
            "- large patch: {changed_lines} changed lines across {} hunks; inspect hunks before /accept",
            summary.hunks
        ));
    }

    if has_source_change && !has_test_change {
        hints.push(
            "- source changes without test changes: add/update tests or run a targeted verify gate"
                .into(),
        );
    }

    hints
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn diff_changes_inline_tests(diff: &str) -> bool {
    let mut hunk_has_change = false;
    let mut hunk_has_inline_test = false;
    for line in diff.lines() {
        if line.starts_with("@@ ") {
            if hunk_has_change && hunk_has_inline_test {
                return true;
            }
            hunk_has_change = false;
            hunk_has_inline_test = false;
            continue;
        }
        let is_changed_line = (line.starts_with('+') && !line.starts_with("+++"))
            || (line.starts_with('-') && !line.starts_with("---"));
        if is_changed_line {
            hunk_has_change = true;
        }
        if line.contains("#[test]") || line.contains("#[cfg(test)]") || line.contains("mod tests") {
            hunk_has_inline_test = true;
        }
    }
    hunk_has_change && hunk_has_inline_test
}

fn add_numstat(summary: &mut PatchSummary, numstat: &str) {
    for line in numstat.lines() {
        let mut parts = line.split('\t');
        let Some(insertions) = parts.next() else {
            continue;
        };
        let Some(deletions) = parts.next() else {
            continue;
        };
        let Some(path) = parts.next() else {
            continue;
        };
        if is_argus_runtime_status_line(path) {
            continue;
        }
        summary.insertions += parse_numstat_count(insertions);
        summary.deletions += parse_numstat_count(deletions);
    }
}

fn parse_numstat_count(value: &str) -> usize {
    value.parse::<usize>().unwrap_or(0)
}

fn count_reviewable_hunks(diff: &str) -> usize {
    let mut current_path_reviewable = true;
    let mut hunks = 0;
    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("diff --git a/") {
            current_path_reviewable = !is_argus_runtime_status_line(path);
        } else if current_path_reviewable && line.starts_with("@@ ") {
            hunks += 1;
        }
    }
    hunks
}

fn format_change_file(status_line: &str) -> String {
    let code = status_code(status_line).trim();
    let path = status_path(status_line);
    let code = if code.is_empty() { "modified" } else { code };
    format!("- {code:<2} {path}")
}

fn status_code(status_line: &str) -> &str {
    status_line.get(..2).unwrap_or(status_line)
}

fn status_path(status_line: &str) -> &str {
    let path = status_line.get(3..).unwrap_or(status_line).trim();
    path.rsplit_once(" -> ")
        .map(|(_, renamed_to)| renamed_to)
        .unwrap_or(path)
}

fn is_critical_project_file(path: &str) -> bool {
    matches!(
        path,
        "Cargo.toml"
            | "Cargo.lock"
            | "package.json"
            | "package-lock.json"
            | "pnpm-lock.yaml"
            | "yarn.lock"
            | "bun.lockb"
            | "pyproject.toml"
            | "requirements.txt"
            | "go.mod"
            | "go.sum"
            | "Dockerfile"
            | "docker-compose.yml"
            | "docker-compose.yaml"
            | "install.sh"
            | "RELEASING.md"
            | "CHANGELOG.md"
    ) || path.starts_with(".github/workflows/")
        || (path.starts_with("scripts/") && path.contains("release"))
}

fn is_source_path(path: &str) -> bool {
    path.starts_with("src/")
        || path.starts_with("crates/")
        || matches!(
            path.rsplit_once('.').map(|(_, ext)| ext),
            Some(
                "rs" | "go"
                    | "py"
                    | "ts"
                    | "tsx"
                    | "js"
                    | "jsx"
                    | "java"
                    | "kt"
                    | "swift"
                    | "c"
                    | "cc"
                    | "cpp"
                    | "h"
                    | "hpp"
                    | "rb"
                    | "php"
                    | "cs"
            )
        )
}

fn is_test_path(path: &str) -> bool {
    path.starts_with("tests/")
        || path.contains("/tests/")
        || path.contains("__tests__/")
        || path.contains("_test.")
        || path.contains(".test.")
        || path.contains(".spec.")
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
    fn load_change_review_includes_patch_summary_for_tracked_diff() {
        let dir = temp_dir("patch-summary");
        std::fs::create_dir_all(&dir).unwrap();
        std::process::Command::new("git")
            .arg("init")
            .current_dir(&dir)
            .output()
            .unwrap();
        std::fs::write(dir.join("tracked.txt"), "one\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "tracked.txt"])
            .current_dir(&dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args([
                "-c",
                "user.email=test@example.com",
                "-c",
                "user.name=Argus Test",
                "commit",
                "-m",
                "seed",
            ])
            .current_dir(&dir)
            .output()
            .unwrap();
        std::fs::write(dir.join("tracked.txt"), "one\ntwo\nthree\n").unwrap();

        let review = super::load_change_review(&dir).unwrap();

        assert!(review.contains("Patch Summary"), "{review}");
        assert!(review.contains("reviewable files: 1"), "{review}");
        assert!(review.contains("hunks: 1"), "{review}");
        assert!(review.contains("insertions: 2"), "{review}");
        assert!(review.contains("deletions: 0"), "{review}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_change_review_includes_patch_hunk_preview_for_tracked_diff() {
        let dir = temp_dir("hunk-preview");
        std::fs::create_dir_all(&dir).unwrap();
        std::process::Command::new("git")
            .arg("init")
            .current_dir(&dir)
            .output()
            .unwrap();
        std::fs::write(dir.join("tracked.txt"), "one\ntwo\nthree\nfour\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "tracked.txt"])
            .current_dir(&dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args([
                "-c",
                "user.email=test@example.com",
                "-c",
                "user.name=Argus Test",
                "commit",
                "-m",
                "seed",
            ])
            .current_dir(&dir)
            .output()
            .unwrap();
        std::fs::write(dir.join("tracked.txt"), "one\nTWO\nthree\nFOUR\n").unwrap();

        let review = super::load_change_review(&dir).unwrap();

        assert!(review.contains("Patch Hunks"), "{review}");
        assert!(
            review.contains("- unstaged tracked.txt @@ -2 +2 @@ (+1 -1)"),
            "{review}"
        );
        assert!(
            review.contains("- unstaged tracked.txt @@ -4 +4 @@ (+1 -1)"),
            "{review}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_change_review_labels_staged_patch_hunks() {
        let dir = temp_dir("staged-hunk-preview");
        std::fs::create_dir_all(&dir).unwrap();
        std::process::Command::new("git")
            .arg("init")
            .current_dir(&dir)
            .output()
            .unwrap();
        std::fs::write(dir.join("tracked.txt"), "one\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "tracked.txt"])
            .current_dir(&dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args([
                "-c",
                "user.email=test@example.com",
                "-c",
                "user.name=Argus Test",
                "commit",
                "-m",
                "seed",
            ])
            .current_dir(&dir)
            .output()
            .unwrap();
        std::fs::write(dir.join("tracked.txt"), "one\ntwo\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "tracked.txt"])
            .current_dir(&dir)
            .output()
            .unwrap();

        let review = super::load_change_review(&dir).unwrap();

        assert!(review.contains("Patch Hunks"), "{review}");
        assert!(
            review.contains("- staged tracked.txt @@ -1,0 +2 @@ (+1 -0)"),
            "{review}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_change_review_flags_risky_config_and_missing_tests() {
        let dir = temp_dir("review_riskhints");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::process::Command::new("git")
            .arg("init")
            .current_dir(&dir)
            .output()
            .unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"demo\"\n").unwrap();
        std::fs::write(dir.join("src/lib.rs"), "pub fn value() -> i32 { 1 }\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "Cargo.toml", "src/lib.rs"])
            .current_dir(&dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args([
                "-c",
                "user.email=test@example.com",
                "-c",
                "user.name=Argus Test",
                "commit",
                "-m",
                "seed",
            ])
            .current_dir(&dir)
            .output()
            .unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("src/lib.rs"), "pub fn value() -> i32 { 2 }\n").unwrap();

        let review = super::load_change_review(&dir).unwrap();

        assert!(review.contains("Risk Hints"), "{review}");
        assert!(
            review.contains("critical project file: Cargo.toml"),
            "{review}"
        );
        assert!(
            review.contains("source changes without test changes"),
            "{review}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_change_review_suppresses_missing_test_hint_when_tests_change() {
        let dir = temp_dir("review_risktests");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("tests")).unwrap();
        std::process::Command::new("git")
            .arg("init")
            .current_dir(&dir)
            .output()
            .unwrap();
        std::fs::write(dir.join("src/lib.rs"), "pub fn value() -> i32 { 1 }\n").unwrap();
        std::fs::write(
            dir.join("tests/value.rs"),
            "#[test]\nfn value_is_one() {}\n",
        )
        .unwrap();
        std::process::Command::new("git")
            .args(["add", "src/lib.rs", "tests/value.rs"])
            .current_dir(&dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args([
                "-c",
                "user.email=test@example.com",
                "-c",
                "user.name=Argus Test",
                "commit",
                "-m",
                "seed",
            ])
            .current_dir(&dir)
            .output()
            .unwrap();
        std::fs::write(dir.join("src/lib.rs"), "pub fn value() -> i32 { 2 }\n").unwrap();
        std::fs::write(
            dir.join("tests/value.rs"),
            "#[test]\nfn value_is_two() {}\n",
        )
        .unwrap();

        let review = super::load_change_review(&dir).unwrap();

        assert!(
            !review.contains("source changes without test changes"),
            "{review}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_change_review_suppresses_missing_test_hint_for_inline_rust_tests() {
        let dir = temp_dir("review_inlinetests");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::process::Command::new("git")
            .arg("init")
            .current_dir(&dir)
            .output()
            .unwrap();
        std::fs::write(
            dir.join("src/lib.rs"),
            "pub fn value() -> i32 { 1 }\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn value_is_one() {}\n}\n",
        )
        .unwrap();
        std::process::Command::new("git")
            .args(["add", "src/lib.rs"])
            .current_dir(&dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args([
                "-c",
                "user.email=test@example.com",
                "-c",
                "user.name=Argus Test",
                "commit",
                "-m",
                "seed",
            ])
            .current_dir(&dir)
            .output()
            .unwrap();
        std::fs::write(
            dir.join("src/lib.rs"),
            "pub fn value() -> i32 { 2 }\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn value_is_two() {}\n}\n",
        )
        .unwrap();

        let review = super::load_change_review(&dir).unwrap();

        assert!(
            !review.contains("source changes without test changes"),
            "{review}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_change_review_flags_deleted_files_and_large_patches() {
        let dir = temp_dir("review_large");
        std::fs::create_dir_all(&dir).unwrap();
        std::process::Command::new("git")
            .arg("init")
            .current_dir(&dir)
            .output()
            .unwrap();
        let original = (0..20)
            .map(|index| format!("line {index}\n"))
            .collect::<String>();
        std::fs::write(dir.join("large.txt"), original).unwrap();
        std::fs::write(dir.join("remove.txt"), "delete me\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "large.txt", "remove.txt"])
            .current_dir(&dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args([
                "-c",
                "user.email=test@example.com",
                "-c",
                "user.name=Argus Test",
                "commit",
                "-m",
                "seed",
            ])
            .current_dir(&dir)
            .output()
            .unwrap();
        let changed = (0..330)
            .map(|index| format!("changed line {index}\n"))
            .collect::<String>();
        std::fs::write(dir.join("large.txt"), changed).unwrap();
        std::fs::remove_file(dir.join("remove.txt")).unwrap();

        let review = super::load_change_review(&dir).unwrap();

        assert!(review.contains("Risk Hints"), "{review}");
        assert!(review.contains("deleted file: remove.txt"), "{review}");
        assert!(review.contains("large patch:"), "{review}");

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
