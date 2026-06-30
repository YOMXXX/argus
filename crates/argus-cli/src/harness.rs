use crate::config::ArgusCodeConfig;
use crate::sessions::append_session;
use crate::tasks::{update_task_status, TaskRecord};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessRunOutput {
    pub task_id: String,
    pub task_text: String,
    pub status: String,
    pub trace: PathBuf,
    pub stdout: String,
    pub stderr: String,
}

pub fn run_task_through_harness(root: &Path, record: &TaskRecord) -> Result<HarnessRunOutput> {
    let (_, config) = crate::workbench::ensure_config(root)?;
    update_task_status(root, &record.id, "running")?;
    let trace = PathBuf::from(".argus/tasks").join(format!("{}.trace.jsonl", record.id));

    let mut command = build_argus_run_command(&argus_binary_path()?, root, &config, record, &trace);

    let output = command.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        update_task_status(root, &record.id, "done")?;
        append_session(root, &record.id, &record.text, "done", trace.clone())?;
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
        anyhow::bail!(
            "Argus harness failed with {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            output.status,
            stdout,
            stderr
        )
    }
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
}
