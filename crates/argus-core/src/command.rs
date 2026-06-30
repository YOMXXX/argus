//! Shared command execution with cwd bounds, timeout, and output limits.

use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};

pub const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
pub const DEFAULT_MAX_OUTPUT_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionLimits {
    pub timeout: Duration,
    pub max_output_bytes: usize,
}

impl Default for ExecutionLimits {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_COMMAND_TIMEOUT,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone)]
pub struct CommandRunner {
    workspace_root: PathBuf,
    cwd: PathBuf,
    limits: ExecutionLimits,
    cwd_relative_to_workspace: bool,
}

impl CommandRunner {
    pub fn new(cwd: impl Into<PathBuf>) -> Self {
        let cwd = cwd.into();
        Self {
            workspace_root: cwd.clone(),
            cwd,
            limits: ExecutionLimits::default(),
            cwd_relative_to_workspace: false,
        }
    }

    pub fn with_timeout(cwd: impl Into<PathBuf>, timeout: Duration) -> Self {
        Self::with_limits(
            cwd,
            ExecutionLimits {
                timeout,
                ..ExecutionLimits::default()
            },
        )
    }

    pub fn with_limits(cwd: impl Into<PathBuf>, limits: ExecutionLimits) -> Self {
        let cwd = cwd.into();
        Self {
            workspace_root: cwd.clone(),
            cwd,
            limits,
            cwd_relative_to_workspace: false,
        }
    }

    pub fn in_workspace(
        workspace_root: impl Into<PathBuf>,
        cwd: impl Into<PathBuf>,
        limits: ExecutionLimits,
    ) -> Self {
        Self {
            workspace_root: workspace_root.into(),
            cwd: cwd.into(),
            limits,
            cwd_relative_to_workspace: true,
        }
    }

    pub async fn run_shell(&self, command: &str) -> anyhow::Result<CommandOutput> {
        self.run("sh", ["-c", command]).await
    }

    pub async fn run<I, S>(&self, program: &str, args: I) -> anyhow::Result<CommandOutput>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let cwd = self.resolve_cwd()?;
        let args = args
            .into_iter()
            .map(|arg| arg.as_ref().to_string())
            .collect::<Vec<_>>();
        let mut command = Command::new(program);
        command
            .args(&args)
            .current_dir(&cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        configure_process_group(&mut command);
        let mut child = command
            .spawn()
            .map_err(|e| anyhow::anyhow!("failed to spawn {program}: {e}"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("failed to capture stdout for {program}"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow::anyhow!("failed to capture stderr for {program}"))?;
        let stdout_task = tokio::spawn(read_limited(stdout, self.limits.max_output_bytes));
        let stderr_task = tokio::spawn(read_limited(stderr, self.limits.max_output_bytes));
        let status = match tokio::time::timeout(self.limits.timeout, child.wait()).await {
            Ok(Ok(status)) => status,
            Ok(Err(e)) => anyhow::bail!("failed to wait for {program}: {e}"),
            Err(_) => {
                kill_child_tree(&mut child).await;
                let _ = child.wait().await;
                let _ = stdout_task.await;
                let _ = stderr_task.await;
                anyhow::bail!(
                    "`{}` timed out after {}s",
                    format_command(program, &args),
                    self.limits.timeout.as_secs_f64()
                )
            }
        };
        let stdout = stdout_task
            .await
            .map_err(|e| anyhow::anyhow!("stdout reader task failed: {e}"))??;
        let stderr = stderr_task
            .await
            .map_err(|e| anyhow::anyhow!("stderr reader task failed: {e}"))??;
        Ok(CommandOutput {
            status,
            stdout,
            stderr,
        })
    }

    fn resolve_cwd(&self) -> anyhow::Result<PathBuf> {
        let root = self.workspace_root.canonicalize().map_err(|e| {
            anyhow::anyhow!(
                "failed to resolve workspace root {}: {e}",
                self.workspace_root.display()
            )
        })?;
        let cwd = if self.cwd_relative_to_workspace && !self.cwd.is_absolute() {
            root.join(&self.cwd)
        } else {
            self.cwd.clone()
        };
        let cwd = cwd.canonicalize().map_err(|e| {
            anyhow::anyhow!("failed to resolve command cwd {}: {e}", self.cwd.display())
        })?;
        if !cwd.starts_with(&root) {
            anyhow::bail!(
                "command cwd {} escapes workspace root {}",
                cwd.display(),
                root.display()
            );
        }
        Ok(cwd)
    }
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_group(_command: &mut Command) {}

async fn kill_child_tree(child: &mut Child) {
    #[cfg(unix)]
    {
        if let Some(pid) = child.id() {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
    }
    let _ = child.start_kill();
}

pub fn format_command(program: &str, args: &[String]) -> String {
    if args.is_empty() {
        program.to_string()
    } else {
        format!("{program} {}", args.join(" "))
    }
}

async fn read_limited<R>(mut reader: R, max_bytes: usize) -> anyhow::Result<String>
where
    R: AsyncRead + Unpin,
{
    let mut out = Vec::with_capacity(max_bytes.min(8192));
    let mut truncated = 0usize;
    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        if out.len() < max_bytes {
            let take = (max_bytes - out.len()).min(n);
            out.extend_from_slice(&buf[..take]);
            truncated += n - take;
        } else {
            truncated += n;
        }
    }
    let text = String::from_utf8_lossy(&out);
    if truncated == 0 {
        Ok(text.into_owned())
    } else {
        Ok(format!("{text}\n[truncated {truncated} byte(s)]"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("argus-command-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[tokio::test]
    async fn rejects_cwd_that_escapes_workspace_root() {
        let root = tmp_root("root");
        let outside = tmp_root("outside");
        let runner = CommandRunner::in_workspace(&root, &outside, ExecutionLimits::default());

        let err = runner.run_shell("true").await.unwrap_err();

        assert!(
            format!("{err}").contains("escapes workspace root"),
            "err: {err}"
        );
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[tokio::test]
    async fn times_out_and_kills_long_running_commands() {
        let root = tmp_root("timeout");
        let runner = CommandRunner::with_timeout(&root, Duration::from_millis(10));

        let err = runner.run_shell("sleep 1").await.unwrap_err();

        assert!(format!("{err}").contains("timed out"), "err: {err}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn truncates_stdout_and_stderr_independently() {
        let root = tmp_root("truncate");
        let runner = CommandRunner::with_limits(
            &root,
            ExecutionLimits {
                timeout: Duration::from_secs(30),
                max_output_bytes: 8,
            },
        );

        let output = runner
            .run_shell("printf abcdefghijklmnop; printf qrstuvwxyz >&2")
            .await
            .unwrap();

        assert!(output.status.success());
        assert!(
            output.stdout.contains("abcdefgh"),
            "stdout: {}",
            output.stdout
        );
        assert!(
            output.stdout.contains("truncated"),
            "stdout: {}",
            output.stdout
        );
        assert!(
            output.stderr.contains("qrstuvwx"),
            "stderr: {}",
            output.stderr
        );
        assert!(
            output.stderr.contains("truncated"),
            "stderr: {}",
            output.stderr
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn runs_program_with_args() {
        let root = tmp_root("program");
        let runner = CommandRunner::new(&root);

        let output = runner.run("sh", ["-c", "printf ok"]).await.unwrap();

        assert!(output.status.success());
        assert_eq!(output.stdout, "ok");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn captures_large_output_without_returning_more_than_limit() {
        let root = tmp_root("large-output");
        let runner = CommandRunner::with_limits(
            &root,
            ExecutionLimits {
                timeout: Duration::from_secs(5),
                max_output_bytes: 1024,
            },
        );

        let output = runner.run_shell("yes x | head -c 1048576").await.unwrap();

        assert!(output.status.success());
        assert!(
            output.stdout.len() < 2048,
            "stdout should stay bounded: {} bytes",
            output.stdout.len()
        );
        assert!(
            output.stdout.contains("truncated"),
            "stdout should explain truncation: {}",
            output.stdout
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn timeout_kills_shell_child_process_group() {
        let root = tmp_root("process-group");
        let pid_file = root.join("child.pid");
        let runner = CommandRunner::with_timeout(&root, Duration::from_millis(500));

        let err = runner
            .run_shell("sleep 5 & echo $! > child.pid; wait")
            .await
            .unwrap_err();

        assert!(format!("{err}").contains("timed out"), "err: {err}");
        let pid_text = std::fs::read_to_string(&pid_file).unwrap();
        let pid = pid_text.trim();
        std::thread::sleep(Duration::from_millis(150));
        let alive = std::process::Command::new("sh")
            .arg("-c")
            .arg("kill -0 \"$1\" 2>/dev/null")
            .arg("sh")
            .arg(pid)
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        assert!(!alive, "child process {pid} should have been killed");
        let _ = std::fs::remove_dir_all(&root);
    }
}
