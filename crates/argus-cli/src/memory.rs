use crate::config::MemoryConfig;
use anyhow::Result;
use std::path::Path;

pub fn load_memory_preview(root: &Path, config: &MemoryConfig) -> Result<String> {
    let project = read_memory_file(root, &config.project, "(project memory not found)")?;
    let lessons = read_memory_file(root, &config.lessons, "(lessons memory not found)")?;
    Ok(format!(
        "Project Memory\n{}\n\nLessons\n{}",
        trim_preview(&project, 18),
        trim_preview(&lessons, 14)
    ))
}

pub fn append_lesson(root: &Path, config: &MemoryConfig, lesson: &str) -> Result<()> {
    let lesson = lesson.trim();
    if lesson.is_empty() {
        anyhow::bail!("lesson must not be empty");
    }
    let path = root.join(&config.lessons);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if !path.exists() {
        std::fs::write(&path, "# ArgusCode Lessons\n\n")?;
    }
    let mut existing = std::fs::read_to_string(&path)?;
    if !existing.ends_with('\n') {
        existing.push('\n');
    }
    existing.push_str(&format!("- {}\n", single_line(lesson)));
    std::fs::write(path, existing)?;
    Ok(())
}

fn read_memory_file(root: &Path, rel: &str, fallback: &str) -> Result<String> {
    let path = root.join(rel);
    if path.exists() {
        Ok(std::fs::read_to_string(path)?)
    } else {
        Ok(fallback.into())
    }
}

fn trim_preview(text: &str, max_lines: usize) -> String {
    let lines = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(max_lines)
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    if lines.is_empty() {
        "(empty)".into()
    } else {
        lines.join("\n")
    }
}

fn single_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use crate::config::{MemoryConfig, PROJECT_MEMORY_PATH};
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "argus-memory-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn memory_preview_reads_project_and_lessons_files() {
        let dir = temp_dir("preview");
        std::fs::create_dir_all(dir.join(".argus/memory")).unwrap();
        std::fs::write(dir.join(PROJECT_MEMORY_PATH), "# Project\n\nUse cargo.\n").unwrap();
        std::fs::write(
            dir.join(".argus/memory/lessons.md"),
            "# Lessons\n\n- Prefer focused diffs.\n",
        )
        .unwrap();
        let config = MemoryConfig {
            project: PROJECT_MEMORY_PATH.into(),
            lessons: ".argus/memory/lessons.md".into(),
        };

        let preview = super::load_memory_preview(&dir, &config).unwrap();

        assert!(preview.contains("Project Memory"), "{preview}");
        assert!(preview.contains("Use cargo."), "{preview}");
        assert!(preview.contains("Lessons"), "{preview}");
        assert!(preview.contains("Prefer focused diffs."), "{preview}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn append_lesson_creates_file_and_rejects_empty_text() {
        let dir = temp_dir("append");
        let config = MemoryConfig {
            project: PROJECT_MEMORY_PATH.into(),
            lessons: ".argus/memory/lessons.md".into(),
        };

        super::append_lesson(&dir, &config, "  Run cargo fmt before committing.  ").unwrap();

        let lessons = std::fs::read_to_string(dir.join(".argus/memory/lessons.md")).unwrap();
        assert!(lessons.contains("# ArgusCode Lessons"), "{lessons}");
        assert!(
            lessons.contains("- Run cargo fmt before committing."),
            "{lessons}"
        );
        let err = super::append_lesson(&dir, &config, "   ").unwrap_err();
        assert!(err.to_string().contains("lesson must not be empty"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
