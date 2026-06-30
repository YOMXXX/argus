use anyhow::Result;
use std::path::Path;
use std::process::Command;

pub fn load_diff_preview(root: &Path) -> Result<String> {
    let status = run_git(root, ["status", "--short"])?;
    let stat = run_git(root, ["diff", "--stat"])?;
    let diff = run_git(root, ["diff", "--", "."])?;

    let status = status.trim();
    let stat = stat.trim();
    let diff = diff.trim();

    if status.is_empty() && stat.is_empty() && diff.is_empty() {
        return Ok("Git Status\n(clean)".into());
    }

    let mut sections = Vec::new();
    sections.push(format!(
        "Git Status\n{}",
        if status.is_empty() { "(clean)" } else { status }
    ));
    if !stat.is_empty() {
        sections.push(format!("Diff Stat\n{stat}"));
    }
    if !diff.is_empty() {
        sections.push(format!("Diff\n{}", truncate_chars(diff, 2400)));
    }
    Ok(sections.join("\n\n"))
}

fn run_git<const N: usize>(root: &Path, args: [&str; N]) -> Result<String> {
    let output = Command::new("git").args(args).current_dir(root).output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Ok(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

fn truncate_chars(text: &str, max: usize) -> String {
    let mut out = text.chars().take(max).collect::<String>();
    if text.chars().count() > max {
        out.push_str("\n... truncated ...");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "arguscode-diff-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn load_diff_preview_reports_untracked_files() {
        let dir = temp_dir("untracked");
        std::fs::create_dir_all(&dir).unwrap();
        std::process::Command::new("git")
            .arg("init")
            .current_dir(&dir)
            .output()
            .unwrap();
        std::fs::write(dir.join("new-file.txt"), "hello\n").unwrap();

        let preview = load_diff_preview(&dir).unwrap();

        assert!(preview.contains("Git Status"), "{preview}");
        assert!(preview.contains("?? new-file.txt"), "{preview}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
