//! 工具抽象与内置文件工具（限工作目录）。

use crate::command::CommandRunner;
use crate::policy::OperationKind;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> Value;
    async fn execute(&self, input: &Value) -> anyhow::Result<String>;
    /// 该工具执行前是否需要用户审批（危险操作如执行 shell 命令应返回 true）。
    fn requires_approval(&self) -> bool {
        false
    }
    fn operation_kind(&self) -> OperationKind {
        if self.requires_approval() {
            OperationKind::Mcp
        } else {
            OperationKind::Read
        }
    }
}

/// 把相对路径限制在 root 之内，拒绝 `..`、绝对路径和符号链接逃逸。
fn safe_join(root: &Path, rel: &str) -> anyhow::Result<PathBuf> {
    let root_abs = root
        .canonicalize()
        .map_err(|e| anyhow::anyhow!("failed to resolve working directory: {e}"))?;
    let mut normalized = root_abs.clone();
    for comp in Path::new(rel).components() {
        use std::path::Component::*;
        match comp {
            ParentDir => {
                normalized.pop();
            }
            Normal(c) => normalized.push(c),
            CurDir => {}
            RootDir | Prefix(_) => anyhow::bail!("absolute paths not allowed: {rel}"),
        }
    }
    if !normalized.starts_with(&root_abs) {
        anyhow::bail!("path escapes working directory: {rel}");
    }
    ensure_resolved_path_stays_in_root(&root_abs, &normalized, rel)?;
    Ok(normalized)
}

fn ensure_resolved_path_stays_in_root(
    root_abs: &Path,
    path: &Path,
    rel: &str,
) -> anyhow::Result<()> {
    if std::fs::symlink_metadata(path).is_ok() {
        let resolved = path
            .canonicalize()
            .map_err(|e| anyhow::anyhow!("failed to resolve path {rel}: {e}"))?;
        if !resolved.starts_with(root_abs) {
            anyhow::bail!("path escapes working directory: {rel}");
        }
        return Ok(());
    }

    let mut ancestor = path.parent();
    while let Some(parent) = ancestor {
        if std::fs::symlink_metadata(parent).is_ok() {
            let resolved = parent
                .canonicalize()
                .map_err(|e| anyhow::anyhow!("failed to resolve parent for {rel}: {e}"))?;
            if !resolved.starts_with(root_abs) {
                anyhow::bail!("path escapes working directory: {rel}");
            }
            return Ok(());
        }
        ancestor = parent.parent();
    }
    anyhow::bail!("path escapes working directory: {rel}");
}

pub struct ReadFile {
    root: PathBuf,
}
impl ReadFile {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

#[async_trait]
impl Tool for ReadFile {
    fn name(&self) -> &str {
        "read_file"
    }
    fn description(&self) -> &str {
        "Read a UTF-8 text file within the working directory."
    }
    fn input_schema(&self) -> Value {
        json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false})
    }
    async fn execute(&self, input: &Value) -> anyhow::Result<String> {
        let rel = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("read_file: missing 'path'"))?;
        let p = safe_join(&self.root, rel)?;
        Ok(std::fs::read_to_string(&p)?)
    }
}

pub struct WriteFile {
    root: PathBuf,
}
impl WriteFile {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

#[async_trait]
impl Tool for WriteFile {
    fn name(&self) -> &str {
        "write_file"
    }
    fn description(&self) -> &str {
        "Write a UTF-8 text file within the working directory (creates parents)."
    }
    fn input_schema(&self) -> Value {
        json!({"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"],"additionalProperties":false})
    }
    fn operation_kind(&self) -> OperationKind {
        OperationKind::Write
    }
    async fn execute(&self, input: &Value) -> anyhow::Result<String> {
        let rel = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("write_file: missing 'path'"))?;
        let content = input
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("write_file: missing 'content'"))?;
        let p = safe_join(&self.root, rel)?;
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&p, content)?;
        Ok(format!("wrote {} bytes to {rel}", content.len()))
    }
}

/// 在工作目录内执行 shell 命令（`sh -c`），带超时。需审批。
pub struct RunShell {
    root: PathBuf,
}
impl RunShell {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

#[async_trait]
impl Tool for RunShell {
    fn name(&self) -> &str {
        "run_shell"
    }
    fn description(&self) -> &str {
        "Run a shell command (sh -c) in the working directory. Returns exit code, stdout, stderr."
    }
    fn input_schema(&self) -> Value {
        json!({"type":"object","properties":{"command":{"type":"string"}},"required":["command"],"additionalProperties":false})
    }
    fn requires_approval(&self) -> bool {
        true
    }
    fn operation_kind(&self) -> OperationKind {
        OperationKind::Shell
    }
    async fn execute(&self, input: &Value) -> anyhow::Result<String> {
        let command = input
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("run_shell: missing 'command'"))?;
        let output = CommandRunner::new(&self.root).run_shell(command).await?;
        Ok(format!(
            "exit: {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            output.status, output.stdout, output.stderr
        ))
    }
}

const IGNORE_DIRS: &[&str] = &[".git", "target", "node_modules", ".argus"];
const MAX_RESULTS: usize = 200;

/// 递归收集 root 下的文件相对路径，跳过常见忽略目录与隐藏文件。
fn walk_files(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || IGNORE_DIRS.contains(&name.as_str()) {
                continue;
            }
            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file() {
                if let Ok(rel) = path.strip_prefix(root) {
                    out.push(rel.to_string_lossy().to_string());
                }
            }
        }
    }
    out.sort();
    out
}

/// 列出工作目录的文件（只读）。
pub struct ListFiles {
    root: PathBuf,
}
impl ListFiles {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

#[async_trait]
impl Tool for ListFiles {
    fn name(&self) -> &str {
        "list_files"
    }
    fn description(&self) -> &str {
        "List files in the working directory (recursive; skips .git/target/node_modules and hidden files). Optional 'contains' substring filter on the path."
    }
    fn input_schema(&self) -> Value {
        json!({"type":"object","properties":{"contains":{"type":"string"}},"additionalProperties":false})
    }
    async fn execute(&self, input: &Value) -> anyhow::Result<String> {
        let filter = input.get("contains").and_then(|v| v.as_str());
        let mut files = walk_files(&self.root);
        if let Some(f) = filter {
            files.retain(|p| p.contains(f));
        }
        let total = files.len();
        files.truncate(MAX_RESULTS);
        let mut out = files.join("\n");
        if total > MAX_RESULTS {
            out.push_str(&format!("\n... ({} more)", total - MAX_RESULTS));
        }
        Ok(if out.is_empty() {
            "(no files)".into()
        } else {
            out
        })
    }
}

/// 在工作目录内按 substring 搜文件内容（只读）。
pub struct SearchText {
    root: PathBuf,
}
impl SearchText {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

#[async_trait]
impl Tool for SearchText {
    fn name(&self) -> &str {
        "search_text"
    }
    fn description(&self) -> &str {
        "Search file contents for a substring across the working directory. Returns matching lines as 'path:line: text'."
    }
    fn input_schema(&self) -> Value {
        json!({"type":"object","properties":{"pattern":{"type":"string"}},"required":["pattern"],"additionalProperties":false})
    }
    async fn execute(&self, input: &Value) -> anyhow::Result<String> {
        let pattern = input
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("search_text: missing 'pattern'"))?;
        let mut hits = Vec::new();
        'outer: for rel in walk_files(&self.root) {
            let full = self.root.join(&rel);
            let content = match std::fs::read_to_string(&full) {
                Ok(c) => c,
                Err(_) => continue, // 跳过非 UTF-8/二进制文件
            };
            for (i, line) in content.lines().enumerate() {
                if line.contains(pattern) {
                    hits.push(format!("{}:{}: {}", rel, i + 1, line.trim()));
                    if hits.len() >= MAX_RESULTS {
                        break 'outer;
                    }
                }
            }
        }
        Ok(if hits.is_empty() {
            format!("(no matches for {pattern:?})")
        } else {
            hits.join("\n")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("argus-tool-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[tokio::test]
    async fn write_then_read_roundtrip() {
        let root = tmp_root("rw");
        let w = WriteFile::new(&root);
        let r = ReadFile::new(&root);
        w.execute(&json!({"path":"a/b.txt","content":"hello"}))
            .await
            .unwrap();
        let out = r.execute(&json!({"path":"a/b.txt"})).await.unwrap();
        assert_eq!(out, "hello");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn rejects_escape() {
        let root = tmp_root("esc");
        let r = ReadFile::new(&root);
        let err = r
            .execute(&json!({"path":"../../etc/passwd"}))
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("escapes working directory"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn read_file_rejects_symlink_escape() {
        let root = tmp_root("read-symlink");
        let outside = tmp_root("read-symlink-outside");
        std::fs::write(outside.join("secret.txt"), "secret").unwrap();
        std::os::unix::fs::symlink(outside.join("secret.txt"), root.join("secret-link")).unwrap();

        let r = ReadFile::new(&root);
        let err = r.execute(&json!({"path":"secret-link"})).await.unwrap_err();

        assert!(format!("{err}").contains("escapes working directory"));
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn write_file_rejects_symlink_parent_escape() {
        let root = tmp_root("write-symlink");
        let outside = tmp_root("write-symlink-outside");
        std::os::unix::fs::symlink(&outside, root.join("outside-link")).unwrap();

        let w = WriteFile::new(&root);
        let err = w
            .execute(&json!({"path":"outside-link/pwn.txt","content":"pwn"}))
            .await
            .unwrap_err();

        assert!(format!("{err}").contains("escapes working directory"));
        assert!(!outside.join("pwn.txt").exists());
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[tokio::test]
    async fn run_shell_executes_and_captures_output() {
        let root = tmp_root("shell");
        let sh = RunShell::new(&root);
        assert!(sh.requires_approval());
        let out = sh
            .execute(&json!({"command":"echo hello-argus"}))
            .await
            .unwrap();
        assert!(out.contains("hello-argus"), "out: {out}");
        assert!(out.contains("exit:"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn read_file_does_not_require_approval() {
        let r = ReadFile::new(".");
        assert!(!r.requires_approval());
    }

    #[tokio::test]
    async fn list_files_lists_and_filters() {
        let root = tmp_root("ls");
        std::fs::write(root.join("a.txt"), "x").unwrap();
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/b.rs"), "y").unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::write(root.join("target/ignored.txt"), "z").unwrap();

        let lf = ListFiles::new(&root);
        let all = lf.execute(&json!({})).await.unwrap();
        assert!(all.contains("a.txt"), "all: {all}");
        assert!(all.contains("sub/b.rs"), "all: {all}");
        assert!(!all.contains("ignored.txt"), "should skip target/: {all}");

        let filtered = lf.execute(&json!({"contains": ".rs"})).await.unwrap();
        assert!(filtered.contains("sub/b.rs"));
        assert!(!filtered.contains("a.txt"));
        assert!(!lf.requires_approval());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn list_files_does_not_follow_symlink_dirs() {
        let root = tmp_root("ls-symlink");
        let outside = tmp_root("ls-symlink-outside");
        std::fs::write(outside.join("secret.txt"), "secret").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("outside-link")).unwrap();

        let lf = ListFiles::new(&root);
        let all = lf.execute(&json!({})).await.unwrap();

        assert!(
            !all.contains("secret.txt"),
            "should not follow symlink: {all}"
        );
        assert!(
            !all.contains("outside-link"),
            "should not list symlink dir as file: {all}"
        );
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[tokio::test]
    async fn search_text_finds_matches() {
        let root = tmp_root("search");
        std::fs::write(
            root.join("code.rs"),
            "fn main() {\n    let token = 42;\n}\n",
        )
        .unwrap();
        let st = SearchText::new(&root);
        let hits = st.execute(&json!({"pattern": "token"})).await.unwrap();
        assert!(hits.contains("code.rs:2:"), "hits: {hits}");
        assert!(hits.contains("token"), "hits: {hits}");
        let none = st
            .execute(&json!({"pattern": "zzzznotfound"}))
            .await
            .unwrap();
        assert!(none.contains("no matches"), "none: {none}");
        assert!(!st.requires_approval());
        let _ = std::fs::remove_dir_all(&root);
    }
}
