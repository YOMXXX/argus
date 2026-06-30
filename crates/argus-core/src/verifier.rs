//! 验证护栏：在 agent 声称完成前跑校验（测试/编译/lint）。

use crate::command::{CommandRunner, ExecutionLimits};
use async_trait::async_trait;
use std::path::PathBuf;
use std::time::Duration;

/// 一次验证的结果。
#[derive(Debug, Clone, PartialEq)]
pub struct VerifyResult {
    pub passed: bool,
    pub detail: String,
}

/// 验证器抽象：返回是否通过 + 详情（失败时含可供 agent 修复的输出）。
#[async_trait]
pub trait Verifier: Send + Sync {
    async fn verify(&self) -> VerifyResult;
}

/// 按序跑一组 shell 命令，全部 exit 0 才通过；任一失败立即返回该命令的输出。
pub struct CommandVerifier {
    root: PathBuf,
    commands: Vec<String>,
    timeout: Duration,
}

impl CommandVerifier {
    pub fn new(root: impl Into<PathBuf>, commands: Vec<String>) -> Self {
        Self::with_timeout(root, commands, Duration::from_secs(30))
    }

    pub fn with_timeout(
        root: impl Into<PathBuf>,
        commands: Vec<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            root: root.into(),
            commands,
            timeout,
        }
    }

    fn runner(&self) -> CommandRunner {
        CommandRunner::with_limits(
            &self.root,
            ExecutionLimits {
                timeout: self.timeout,
                ..ExecutionLimits::default()
            },
        )
    }
}

#[async_trait]
impl Verifier for CommandVerifier {
    async fn verify(&self) -> VerifyResult {
        let runner = self.runner();
        for cmd in &self.commands {
            let output = match runner.run_shell(cmd).await {
                Ok(output) => output,
                Err(e) => {
                    return VerifyResult {
                        passed: false,
                        detail: format!("`{cmd}` {e}"),
                    }
                }
            };
            if !output.status.success() {
                return VerifyResult {
                    passed: false,
                    detail: format!(
                        "`{cmd}` exited {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
                        output.status, output.stdout, output.stderr
                    ),
                };
            }
        }
        VerifyResult {
            passed: true,
            detail: format!("{} check(s) passed", self.commands.len()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("argus-verify-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[tokio::test]
    async fn passes_when_all_commands_succeed() {
        let root = tmp_root("ok");
        let v = CommandVerifier::new(&root, vec!["true".into(), "echo hi".into()]);
        let r = v.verify().await;
        assert!(r.passed, "detail: {}", r.detail);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn fails_with_output_when_a_command_fails() {
        let root = tmp_root("fail");
        let v = CommandVerifier::new(
            &root,
            vec!["echo before".into(), "sh -c 'echo boom >&2; exit 1'".into()],
        );
        let r = v.verify().await;
        assert!(!r.passed);
        assert!(r.detail.contains("boom"), "detail: {}", r.detail);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn times_out_long_running_commands() {
        let root = tmp_root("timeout");
        let v =
            CommandVerifier::with_timeout(&root, vec!["sleep 1".into()], Duration::from_millis(10));
        let r = v.verify().await;
        assert!(!r.passed);
        assert!(r.detail.contains("timed out"), "detail: {}", r.detail);
        let _ = std::fs::remove_dir_all(&root);
    }
}
