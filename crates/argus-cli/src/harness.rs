use crate::cockpit::append_cockpit_event;
use crate::config::ArgusCodeConfig;
use crate::sessions::append_session;
use crate::tasks::{update_task_status, TaskRecord};
use anyhow::Result;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessRunOutput {
    pub task_id: String,
    pub task_text: String,
    pub status: String,
    pub trace: PathBuf,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug)]
struct CommandRunOutput {
    status: ExitStatus,
    stdout: String,
    stderr: String,
    canceled: bool,
}

pub fn run_task_through_harness(root: &Path, record: &TaskRecord) -> Result<HarnessRunOutput> {
    let (_, config) = crate::workbench::ensure_config(root)?;
    update_task_status(root, &record.id, "running")?;
    let trace = PathBuf::from(".argus/tasks").join(format!("{}.trace.jsonl", record.id));
    append_cockpit_event(
        root,
        "harness",
        &format!("task {} running through argus run", record.id),
        "wait for model, tools, and verification gate",
    )?;

    let mut command = build_argus_run_command(&argus_binary_path()?, root, &config, record, &trace);

    let output = run_command_with_background_output(root, &record.id, &mut command)?;
    let stdout = output.stdout;
    let stderr = output.stderr;

    if output.canceled {
        crate::background::clear_background_cancel(root)?;
        update_task_status(root, &record.id, "canceled")?;
        append_session(root, &record.id, &record.text, "canceled", trace.clone())?;
        append_cockpit_event(
            root,
            "harness",
            &format!("task {} canceled by user request", record.id),
            "/retry <task-id> or edit the task before running again",
        )?;
        Ok(HarnessRunOutput {
            task_id: record.id.clone(),
            task_text: record.text.clone(),
            status: "canceled".into(),
            trace,
            stdout,
            stderr,
        })
    } else if output.status.success() {
        update_task_status(root, &record.id, "done")?;
        append_session(root, &record.id, &record.text, "done", trace.clone())?;
        append_cockpit_event(
            root,
            "harness",
            &format!("task {} completed with status done", record.id),
            "/review, /verify, or /accept <note>",
        )?;
        Ok(HarnessRunOutput {
            task_id: record.id.clone(),
            task_text: record.text.clone(),
            status: "done".into(),
            trace,
            stdout,
            stderr,
        })
    } else {
        update_task_status(root, &record.id, "failed")?;
        append_session(root, &record.id, &record.text, "failed", trace)?;
        append_cockpit_event(
            root,
            "harness",
            &format!("task {} completed with status failed", record.id),
            "/retry <task-id>, /rework <task>, or inspect trace",
        )?;
        let status = output.status;
        anyhow::bail!("Argus harness failed with {status}\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}")
    }
}

fn run_command_with_background_output(
    root: &Path,
    task_id: &str,
    command: &mut Command,
) -> Result<CommandRunOutput> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    configure_process_group(command);
    let mut child = command.spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("failed to capture stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("failed to capture stderr"))?;
    let stdout_root = root.to_path_buf();
    let stderr_root = root.to_path_buf();
    let stdout_task =
        std::thread::spawn(move || read_stream_to_background(&stdout_root, "stdout", stdout));
    let stderr_task =
        std::thread::spawn(move || read_stream_to_background(&stderr_root, "stderr", stderr));
    let mut canceled = false;
    let status = loop {
        if crate::background::background_cancel_requested(root, task_id)? {
            canceled = true;
            kill_child_tree(&mut child);
            break child.wait()?;
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let stdout = stdout_task
        .join()
        .map_err(|_| anyhow::anyhow!("stdout reader thread panicked"))??;
    let stderr = stderr_task
        .join()
        .map_err(|_| anyhow::anyhow!("stderr reader thread panicked"))??;
    Ok(CommandRunOutput {
        status,
        stdout,
        stderr,
        canceled,
    })
}

fn read_stream_to_background<R>(root: &Path, stream: &str, reader: R) -> Result<String>
where
    R: Read,
{
    let mut reader = BufReader::new(reader);
    let mut output = String::new();
    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            break;
        }
        crate::background::append_background_output(root, stream, &line)?;
        output.push_str(&line);
    }
    Ok(output)
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_group(_command: &mut Command) {}

fn kill_child_tree(child: &mut Child) {
    #[cfg(unix)]
    {
        let pid = child.id();
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    let _ = child.kill();
}

fn build_argus_run_command(
    argus_path: &Path,
    root: &Path,
    config: &ArgusCodeConfig,
    record: &TaskRecord,
    trace: &Path,
) -> Command {
    let mut command = Command::new(argus_path);
    command
        .current_dir(root)
        .arg("run")
        .arg(&record.text)
        .arg("--provider")
        .arg(&config.provider.default_provider)
        .arg("--model")
        .arg(&config.provider.default_model)
        .arg("--sandbox")
        .arg(&config.security.sandbox)
        .arg("--trace")
        .arg(trace);
    if config.security.approval == "auto" {
        command.arg("--yes");
    }
    if let Some(base_url) = &config.provider.base_url {
        command.arg("--base-url").arg(base_url);
    }
    if let Some(api_key_env) = &config.provider.api_key_env {
        if let Some(value) = std::env::var_os(api_key_env) {
            command.env("OPENAI_API_KEY", value);
        }
    }
    if let Some(mcp_command) = &config.mcp.command {
        command.arg("--mcp").arg(mcp_command);
        for allowed in &config.mcp.allow {
            command.arg("--mcp-allow").arg(allowed);
        }
    }
    for verify in &config.verify.commands {
        command.arg("--verify").arg(verify);
    }
    command
}

pub fn argus_binary_path() -> Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let binary_name = if cfg!(windows) { "argus.exe" } else { "argus" };
    let sibling = exe.with_file_name(binary_name);
    if sibling.exists() {
        Ok(sibling)
    } else {
        Ok(PathBuf::from(binary_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        ArgusCodeConfig, MemoryConfig, ProjectConfig, ProviderConfig, RulesConfig, SecurityConfig,
        UiConfig, VerifyConfig,
    };

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "argus-harness-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn command_stream_writes_stdout_before_process_exits() {
        let dir = temp_dir("stream");
        std::fs::create_dir_all(&dir).unwrap();
        let run_dir = dir.clone();
        let handle = std::thread::spawn(move || {
            let mut command = Command::new("sh");
            command
                .arg("-c")
                .arg("echo first; sleep 0.35; echo second; echo warn >&2");
            super::run_command_with_background_output(&run_dir, "task-1", &mut command).unwrap()
        });

        let mut saw_first = false;
        for _ in 0..10 {
            let output = crate::background::list_background_output(&dir).unwrap();
            if output
                .iter()
                .any(|record| record.stream == "stdout" && record.text == "first")
            {
                saw_first = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        assert!(saw_first, "stdout should stream before process exits");
        let output = handle.join().unwrap();
        assert!(output.status.success(), "{}", output.status);
        assert!(output.stdout.contains("first"), "{}", output.stdout);
        assert!(output.stdout.contains("second"), "{}", output.stdout);
        assert!(output.stderr.contains("warn"), "{}", output.stderr);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn command_stream_stops_when_background_cancel_requested() {
        let dir = temp_dir("cancel-stream");
        std::fs::create_dir_all(&dir).unwrap();
        let run_dir = dir.clone();
        let started_at = std::time::Instant::now();
        let handle = std::thread::spawn(move || {
            let mut command = Command::new("sh");
            command.arg("-c").arg("echo started; sleep 5; echo never");
            super::run_command_with_background_output(&run_dir, "task-1", &mut command).unwrap()
        });

        for _ in 0..20 {
            let output = crate::background::list_background_output(&dir).unwrap();
            if output
                .iter()
                .any(|record| record.stream == "stdout" && record.text == "started")
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        crate::background::request_background_cancel(&dir, "task-1", "test requested stop")
            .unwrap();
        let output = handle.join().unwrap();

        assert!(output.canceled, "{output:?}");
        assert!(!output.stdout.contains("never"), "{}", output.stdout);
        assert!(
            started_at.elapsed() < std::time::Duration::from_secs(2),
            "cancel should stop the command promptly"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn openai_compatible_config_adds_base_url_and_api_key_alias() {
        std::env::set_var("ARGUS_TEST_PROVIDER_KEY", "secret");
        let config = ArgusCodeConfig {
            schema_version: 1,
            project: ProjectConfig {
                name: "demo".into(),
                root: ".".into(),
                languages: vec!["rust".into()],
                package_manager: Some("cargo".into()),
            },
            provider: ProviderConfig {
                default_provider: "openai".into(),
                default_model: "deepseek-chat".into(),
                routing: "manual".into(),
                base_url: Some("https://api.deepseek.com".into()),
                api_key_env: Some("ARGUS_TEST_PROVIDER_KEY".into()),
            },
            security: SecurityConfig::default(),
            mcp: crate::config::McpConfig::default(),
            verify: VerifyConfig {
                commands: vec!["cargo test".into()],
                gate: true,
            },
            rules: RulesConfig { imported: vec![] },
            memory: MemoryConfig {
                project: ".argus/memory/project.md".into(),
                lessons: ".argus/memory/lessons.md".into(),
            },
            ui: UiConfig {
                default_view: "workbench".into(),
                theme: "nocturne".into(),
            },
        };
        let record = TaskRecord {
            id: "task-1".into(),
            text: "fix tests".into(),
            status: "queued".into(),
            created_ms: 1,
        };

        let command = build_argus_run_command(
            Path::new("argus"),
            Path::new("/tmp/demo"),
            &config,
            &record,
            Path::new(".argus/tasks/task-1.trace.jsonl"),
        );
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--base-url", "https://api.deepseek.com"]),
            "{args:?}"
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--model", "deepseek-chat"]),
            "{args:?}"
        );
        let envs = command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().to_string(),
                    value.map(|v| v.to_string_lossy().to_string()),
                )
            })
            .collect::<Vec<_>>();
        assert!(
            envs.iter()
                .any(|(key, value)| key == "OPENAI_API_KEY" && value.as_deref() == Some("secret")),
            "{envs:?}"
        );
        std::env::remove_var("ARGUS_TEST_PROVIDER_KEY");
    }

    #[test]
    fn security_config_maps_to_argus_run_sandbox_and_approval_args() {
        let mut config = ArgusCodeConfig {
            schema_version: 1,
            project: ProjectConfig {
                name: "demo".into(),
                root: ".".into(),
                languages: vec!["rust".into()],
                package_manager: Some("cargo".into()),
            },
            provider: ProviderConfig {
                default_provider: "mock".into(),
                default_model: "mock".into(),
                routing: "manual".into(),
                base_url: None,
                api_key_env: None,
            },
            security: SecurityConfig {
                sandbox: "read-only".into(),
                approval: "ask".into(),
            },
            mcp: crate::config::McpConfig::default(),
            verify: VerifyConfig {
                commands: vec![],
                gate: true,
            },
            rules: RulesConfig { imported: vec![] },
            memory: MemoryConfig {
                project: ".argus/memory/project.md".into(),
                lessons: ".argus/memory/lessons.md".into(),
            },
            ui: UiConfig {
                default_view: "workbench".into(),
                theme: "nocturne".into(),
            },
        };
        let record = TaskRecord {
            id: "task-1".into(),
            text: "inspect only".into(),
            status: "queued".into(),
            created_ms: 1,
        };

        let command = build_argus_run_command(
            Path::new("argus"),
            Path::new("/tmp/demo"),
            &config,
            &record,
            Path::new(".argus/tasks/task-1.trace.jsonl"),
        );
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--sandbox", "read-only"]),
            "{args:?}"
        );
        assert!(
            !args.iter().any(|arg| arg == "--yes"),
            "ask approval should not auto-approve: {args:?}"
        );

        config.security.approval = "auto".into();
        let command = build_argus_run_command(
            Path::new("argus"),
            Path::new("/tmp/demo"),
            &config,
            &record,
            Path::new(".argus/tasks/task-1.trace.jsonl"),
        );
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert!(args.iter().any(|arg| arg == "--yes"), "{args:?}");
    }

    #[test]
    fn mcp_config_maps_to_argus_run_mcp_args() {
        let config = ArgusCodeConfig {
            schema_version: 1,
            project: ProjectConfig {
                name: "demo".into(),
                root: ".".into(),
                languages: vec!["rust".into()],
                package_manager: Some("cargo".into()),
            },
            provider: ProviderConfig {
                default_provider: "mock".into(),
                default_model: "mock".into(),
                routing: "manual".into(),
                base_url: None,
                api_key_env: None,
            },
            security: SecurityConfig::default(),
            mcp: crate::config::McpConfig {
                command: Some("argus __mcp-mock".into()),
                allow: vec!["echo".into(), "read_file".into()],
            },
            verify: VerifyConfig {
                commands: vec![],
                gate: true,
            },
            rules: RulesConfig { imported: vec![] },
            memory: MemoryConfig {
                project: ".argus/memory/project.md".into(),
                lessons: ".argus/memory/lessons.md".into(),
            },
            ui: UiConfig {
                default_view: "workbench".into(),
                theme: "nocturne".into(),
            },
        };
        let record = TaskRecord {
            id: "task-1".into(),
            text: "call external tool".into(),
            status: "queued".into(),
            created_ms: 1,
        };

        let command = build_argus_run_command(
            Path::new("argus"),
            Path::new("/tmp/demo"),
            &config,
            &record,
            Path::new(".argus/tasks/task-1.trace.jsonl"),
        );
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert!(
            args.windows(2)
                .any(|pair| pair == ["--mcp", "argus __mcp-mock"]),
            "{args:?}"
        );
        assert_eq!(
            args.windows(2)
                .filter(|pair| pair[0] == "--mcp-allow")
                .map(|pair| pair[1].clone())
                .collect::<Vec<_>>(),
            vec!["echo", "read_file"]
        );
    }
}
