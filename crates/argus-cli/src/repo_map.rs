use crate::config::ArgusCodeConfig;
use crate::project::ProjectProfile;
use anyhow::Result;
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

const MAX_SCANNED_FILES: usize = 5_000;
const IGNORED_DIRS: &[&str] = &[
    ".argus",
    ".git",
    ".hg",
    ".svn",
    ".next",
    "build",
    "coverage",
    "dist",
    "node_modules",
    "target",
];

pub fn load_repo_map(
    root: &Path,
    profile: &ProjectProfile,
    config: &ArgusCodeConfig,
) -> Result<String> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort();
    if files.len() > MAX_SCANNED_FILES {
        files.truncate(MAX_SCANNED_FILES);
    }

    let mut dirs = BTreeMap::<String, usize>::new();
    let mut extensions = BTreeMap::<String, usize>::new();
    for path in &files {
        *dirs.entry(top_dir(path)).or_insert(0) += 1;
        *extensions.entry(extension(path)).or_insert(0) += 1;
    }

    let languages = if profile.languages.is_empty() {
        "unknown".to_string()
    } else {
        profile.languages.join(", ")
    };
    let rules = if profile.rules_files.is_empty() {
        "(none)".to_string()
    } else {
        profile
            .rules_files
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let verify = if config.verify.commands.is_empty() {
        "(none)".to_string()
    } else {
        config.verify.commands.join("\n")
    };

    Ok(format!(
        "Repo Map\nFiles scanned: {}\nLanguages: {}\nPackage manager: {}\n\nTop Directories\n{}\n\nExtensions\n{}\n\nRules\n{}\n\nVerify\n{}",
        files.len(),
        languages,
        profile.package_manager.as_deref().unwrap_or("unknown"),
        render_counts(&dirs, 8),
        render_counts(&extensions, 8),
        rules,
        verify,
    ))
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if out.len() >= MAX_SCANNED_FILES {
        return Ok(());
    }
    let mut entries = std::fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        if out.len() >= MAX_SCANNED_FILES {
            break;
        }
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            if should_skip_dir(&path) {
                continue;
            }
            collect_files(root, &path, out)?;
        } else if file_type.is_file() {
            let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            out.push(relative);
        }
    }
    Ok(())
}

fn should_skip_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| IGNORED_DIRS.contains(&name))
}

fn top_dir(path: &Path) -> String {
    match path.components().next() {
        Some(Component::Normal(name)) if path.components().count() > 1 => {
            name.to_string_lossy().to_string()
        }
        _ => ".".into(),
    }
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .unwrap_or_else(|| "(none)".into())
}

fn render_counts(counts: &BTreeMap<String, usize>, limit: usize) -> String {
    if counts.is_empty() {
        return "(none)".into();
    }
    let mut rows = counts
        .iter()
        .map(|(name, count)| (name.as_str(), *count))
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    rows.into_iter()
        .take(limit)
        .map(|(name, count)| format!("{name}  {count}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{build_config, ProjectProfile};

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "arguscode-repo-map-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn load_repo_map_counts_extensions_and_ignores_build_dirs() {
        let dir = temp_dir("counts");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("target/debug")).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"demo\"\n").unwrap();
        std::fs::write(dir.join("src/lib.rs"), "pub fn demo() {}\n").unwrap();
        std::fs::write(dir.join("target/debug/build.rs"), "ignored\n").unwrap();
        let profile = ProjectProfile {
            root: dir.clone(),
            name: "demo".into(),
            languages: vec!["rust".into()],
            package_manager: Some("cargo".into()),
            verify_commands: vec!["cargo test".into()],
            rules_files: vec![PathBuf::from("AGENTS.md")],
            detected_files: vec![],
        };
        let config = build_config(&profile);

        let map = load_repo_map(&dir, &profile, &config).unwrap();

        assert!(map.contains("Repo Map"), "{map}");
        assert!(map.contains("Files scanned: 2"), "{map}");
        assert!(map.contains("src  1"), "{map}");
        assert!(map.contains("rs  1"), "{map}");
        assert!(!map.contains("build.rs"), "{map}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
