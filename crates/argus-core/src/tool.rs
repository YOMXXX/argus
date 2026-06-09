//! 工具抽象与内置文件工具（限工作目录）。

use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> Value;
    async fn execute(&self, input: &Value) -> anyhow::Result<String>;
}

/// 把相对路径限制在 root 之内，拒绝逃逸（.. / 绝对路径越界）。
/// 注意：不防护符号链接逃逸——root 内若有指向外部的 symlink 仍可能被跟随；
/// 调用方需保证 root 内无恶意 symlink（agent 的 write_file 只写文本、不创建 symlink）。
fn safe_join(root: &Path, rel: &str) -> anyhow::Result<PathBuf> {
    let root_abs = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut normalized = root_abs.clone();
    for comp in Path::new(rel).components() {
        use std::path::Component::*;
        match comp {
            ParentDir => { normalized.pop(); }
            Normal(c) => normalized.push(c),
            CurDir => {}
            RootDir | Prefix(_) => anyhow::bail!("absolute paths not allowed: {rel}"),
        }
    }
    if !normalized.starts_with(&root_abs) {
        anyhow::bail!("path escapes working directory: {rel}");
    }
    Ok(normalized)
}

pub struct ReadFile { root: PathBuf }
impl ReadFile { pub fn new(root: impl Into<PathBuf>) -> Self { Self { root: root.into() } } }

#[async_trait]
impl Tool for ReadFile {
    fn name(&self) -> &str { "read_file" }
    fn description(&self) -> &str { "Read a UTF-8 text file within the working directory." }
    fn input_schema(&self) -> Value {
        json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false})
    }
    async fn execute(&self, input: &Value) -> anyhow::Result<String> {
        let rel = input.get("path").and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("read_file: missing 'path'"))?;
        let p = safe_join(&self.root, rel)?;
        Ok(std::fs::read_to_string(&p)?)
    }
}

pub struct WriteFile { root: PathBuf }
impl WriteFile { pub fn new(root: impl Into<PathBuf>) -> Self { Self { root: root.into() } } }

#[async_trait]
impl Tool for WriteFile {
    fn name(&self) -> &str { "write_file" }
    fn description(&self) -> &str { "Write a UTF-8 text file within the working directory (creates parents)." }
    fn input_schema(&self) -> Value {
        json!({"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"],"additionalProperties":false})
    }
    async fn execute(&self, input: &Value) -> anyhow::Result<String> {
        let rel = input.get("path").and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("write_file: missing 'path'"))?;
        let content = input.get("content").and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("write_file: missing 'content'"))?;
        let p = safe_join(&self.root, rel)?;
        if let Some(parent) = p.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::write(&p, content)?;
        Ok(format!("wrote {} bytes to {rel}", content.len()))
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
        w.execute(&json!({"path":"a/b.txt","content":"hello"})).await.unwrap();
        let out = r.execute(&json!({"path":"a/b.txt"})).await.unwrap();
        assert_eq!(out, "hello");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn rejects_escape() {
        let root = tmp_root("esc");
        let r = ReadFile::new(&root);
        let err = r.execute(&json!({"path":"../../etc/passwd"})).await.unwrap_err();
        assert!(format!("{err}").contains("escapes working directory"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
