use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const CHECKPOINT_DIR: &str = ".argus/checkpoints";
const SNAPSHOT_DIR: &str = "snapshot";
const MANIFEST_FILE: &str = "manifest.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckpointRecord {
    pub id: String,
    pub label: String,
    pub created_ms: u128,
    pub file_count: usize,
}

pub fn create_checkpoint(root: &Path, label: &str) -> Result<CheckpointRecord> {
    let id = unique_checkpoint_id();
    let checkpoint_dir = root.join(CHECKPOINT_DIR).join(&id);
    let snapshot_dir = checkpoint_dir.join(SNAPSHOT_DIR);
    std::fs::create_dir_all(&snapshot_dir)?;

    let files = workspace_files(root)?;
    for rel in &files {
        let src = root.join(rel);
        let dst = snapshot_dir.join(rel);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dst)?;
    }

    let record = CheckpointRecord {
        id,
        label: normalize_label(label),
        created_ms: now_ms(),
        file_count: files.len(),
    };
    write_manifest(root, &record)?;
    Ok(record)
}

pub fn latest_checkpoint(root: &Path) -> Result<Option<CheckpointRecord>> {
    let dir = root.join(CHECKPOINT_DIR);
    if !dir.exists() {
        return Ok(None);
    }
    let mut records = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let manifest = entry.path().join(MANIFEST_FILE);
        if manifest.exists() {
            let text = std::fs::read_to_string(manifest)?;
            records.push(serde_json::from_str::<CheckpointRecord>(&text)?);
        }
    }
    records.sort_by(|a, b| {
        a.created_ms
            .cmp(&b.created_ms)
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(records.pop())
}

pub fn restore_checkpoint(root: &Path, id: &str) -> Result<CheckpointRecord> {
    let record = read_manifest(root, id)?;
    let snapshot_dir = root.join(CHECKPOINT_DIR).join(id).join(SNAPSHOT_DIR);
    if !snapshot_dir.exists() {
        anyhow::bail!("checkpoint snapshot not found: {id}");
    }

    let snapshot_files = snapshot_files(&snapshot_dir)?;
    let snapshot_set = snapshot_files.iter().cloned().collect::<BTreeSet<_>>();
    for rel in workspace_files(root)? {
        if !snapshot_set.contains(&rel) {
            let path = root.join(rel);
            if path.is_file() {
                std::fs::remove_file(path)?;
            }
        }
    }
    for rel in &snapshot_files {
        let src = snapshot_dir.join(rel);
        let dst = root.join(rel);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dst)?;
    }
    Ok(record)
}

fn write_manifest(root: &Path, record: &CheckpointRecord) -> Result<()> {
    let path = root
        .join(CHECKPOINT_DIR)
        .join(&record.id)
        .join(MANIFEST_FILE);
    std::fs::write(path, serde_json::to_string_pretty(record)?)?;
    Ok(())
}

fn read_manifest(root: &Path, id: &str) -> Result<CheckpointRecord> {
    let path = root.join(CHECKPOINT_DIR).join(id).join(MANIFEST_FILE);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("failed to read checkpoint {}: {e}", path.display()))?;
    Ok(serde_json::from_str(&text)?)
}

fn workspace_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

fn snapshot_files(snapshot_root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_snapshot_files(snapshot_root, snapshot_root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_files(root: &Path, dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        let name = entry.file_name();
        if file_type.is_dir() {
            if is_excluded_dir(&name.to_string_lossy()) {
                continue;
            }
            collect_files(root, &path, files)?;
        } else if file_type.is_file() {
            files.push(path.strip_prefix(root)?.to_path_buf());
        }
    }
    Ok(())
}

fn collect_snapshot_files(root: &Path, dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_snapshot_files(root, &path, files)?;
        } else if file_type.is_file() {
            files.push(path.strip_prefix(root)?.to_path_buf());
        }
    }
    Ok(())
}

fn is_excluded_dir(name: &str) -> bool {
    matches!(name, ".git" | ".argus" | "target" | "node_modules")
}

fn normalize_label(label: &str) -> String {
    let label = label.trim();
    if label.is_empty() {
        "manual checkpoint".into()
    } else {
        label.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

fn unique_checkpoint_id() -> String {
    format!("ckpt-{}-{}", now_nanos(), std::process::id())
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
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
            "argus-checkpoint-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn checkpoint_restore_returns_workspace_to_snapshot() {
        let dir = temp_dir("restore");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::write(dir.join("src/lib.rs"), "before\n").unwrap();
        std::fs::write(dir.join("target/cache.txt"), "cache\n").unwrap();

        let checkpoint = super::create_checkpoint(&dir, "before agent").unwrap();
        std::fs::write(dir.join("src/lib.rs"), "after\n").unwrap();
        std::fs::write(dir.join("src/new.rs"), "new\n").unwrap();
        std::fs::write(dir.join("target/cache.txt"), "still here\n").unwrap();

        let restored = super::restore_checkpoint(&dir, &checkpoint.id).unwrap();

        assert_eq!(restored.id, checkpoint.id);
        assert_eq!(
            std::fs::read_to_string(dir.join("src/lib.rs")).unwrap(),
            "before\n"
        );
        assert!(!dir.join("src/new.rs").exists());
        assert_eq!(
            std::fs::read_to_string(dir.join("target/cache.txt")).unwrap(),
            "still here\n"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn latest_checkpoint_returns_newest_record() {
        let dir = temp_dir("latest");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "a\n").unwrap();

        let first = super::create_checkpoint(&dir, "first").unwrap();
        let second = super::create_checkpoint(&dir, "second").unwrap();
        let latest = super::latest_checkpoint(&dir).unwrap().unwrap();

        assert_ne!(first.id, second.id);
        assert_eq!(latest.id, second.id);
        assert_eq!(latest.label, "second");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
