use crate::config::ArgusCodeConfig;
use crate::harness::argus_binary_path;
use crate::sessions::append_session;
use crate::tasks::{update_task_status, TaskRecord};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteRunOutput {
    pub task_id: String,
    pub task_text: String,
    pub status: String,
    pub trace: PathBuf,
    pub cheap_model: String,
    pub strong_model: String,
    pub stdout: String,
    pub stderr: String,
}

pub fn run_task_through_route(
    root: &Path,
    record: &TaskRecord,
    cheap_model: &str,
    strong_model: &str,
) -> Result<RouteRunOutput> {
    let config = ArgusCodeConfig::read(root)?;
    update_task_status(root, &record.id, "running")?;
    let trace = PathBuf::from(".argus/tasks").join(format!("{}.route.trace.jsonl", record.id));
    let mut command = build_argus_route_command(
        &argus_binary_path()?,
        root,
        &config,
        &record.text,
        cheap_model,
        strong_model,
        &trace,
    );

    let output = command.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        update_task_status(root, &record.id, "done")?;
        append_session(root, &record.id, &record.text, "done", trace.clone())?;
        Ok(RouteRunOutput {
            task_id: record.id.clone(),
            task_text: record.text.clone(),
            status: "done".into(),
            trace,
            cheap_model: cheap_model.into(),
            strong_model: strong_model.into(),
            stdout,
            stderr,
        })
    } else {
        update_task_status(root, &record.id, "failed")?;
        append_session(root, &record.id, &record.text, "failed", trace)?;
        anyhow::bail!(
            "Argus route failed with {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            output.status,
            stdout,
            stderr
        )
    }
}

pub(crate) fn build_argus_route_command(
    argus_path: &Path,
    root: &Path,
    config: &ArgusCodeConfig,
    task: &str,
    cheap_model: &str,
    strong_model: &str,
    trace: &Path,
) -> Command {
    let mut command = Command::new(argus_path);
    command
        .current_dir(root)
        .arg("route")
        .arg(task)
        .arg("--cheap")
        .arg(cheap_model)
        .arg("--strong")
        .arg(strong_model)
        .arg("--provider")
        .arg(&config.provider.default_provider)
        .arg("--trace")
        .arg(trace);
    for verify in &config.verify.commands {
        command.arg("--verify").arg(verify);
    }
    if let Some(base_url) = &config.provider.base_url {
        command.arg("--base-url").arg(base_url);
    }
    if let Some(api_key_env) = &config.provider.api_key_env {
        if let Some(value) = std::env::var_os(api_key_env) {
            command.env("OPENAI_API_KEY", value);
        }
    }
    command
}

#[cfg(test)]
mod tests {
    use crate::config::{
        ArgusCodeConfig, MemoryConfig, ProjectConfig, ProviderConfig, RulesConfig, SecurityConfig,
        UiConfig, VerifyConfig,
    };
    use std::path::Path;

    #[test]
    fn route_command_uses_models_verify_provider_and_api_key_alias() {
        std::env::set_var("ARGUS_TEST_ROUTE_KEY", "secret");
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
                api_key_env: Some("ARGUS_TEST_ROUTE_KEY".into()),
            },
            security: SecurityConfig::default(),
            mcp: crate::config::McpConfig::default(),
            verify: VerifyConfig {
                commands: vec!["cargo test".into(), "cargo clippy".into()],
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

        let command = super::build_argus_route_command(
            Path::new("argus"),
            Path::new("/tmp/demo"),
            &config,
            "fix tests",
            "deepseek-chat",
            "deepseek-reasoner",
            Path::new(".argus/tasks/task-1.route.trace.jsonl"),
        );
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert!(
            args.starts_with(&["route".into(), "fix tests".into()]),
            "{args:?}"
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--cheap", "deepseek-chat"]),
            "{args:?}"
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--strong", "deepseek-reasoner"]),
            "{args:?}"
        );
        assert!(
            args.windows(2).any(|pair| pair == ["--provider", "openai"]),
            "{args:?}"
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--trace", ".argus/tasks/task-1.route.trace.jsonl"]),
            "{args:?}"
        );
        assert_eq!(
            args.windows(2)
                .filter(|pair| pair[0] == "--verify")
                .map(|pair| pair[1].clone())
                .collect::<Vec<_>>(),
            vec!["cargo test", "cargo clippy"]
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--base-url", "https://api.deepseek.com"]),
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
        std::env::remove_var("ARGUS_TEST_ROUTE_KEY");
    }
}
