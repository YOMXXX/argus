use crate::config::ArgusCodeConfig;
use crate::harness::argus_binary_path;
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

const DEFAULT_EVAL_RUN_DIR: &str = ".argus/eval-runs";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalRunOutput {
    pub suite: PathBuf,
    pub out_dir: PathBuf,
    pub report_json: PathBuf,
    pub status: String,
    pub stdout: String,
    pub stderr: String,
}

pub fn run_eval_suite(
    root: &Path,
    config: &ArgusCodeConfig,
    suite: &Path,
) -> Result<EvalRunOutput> {
    let out_dir = PathBuf::from(DEFAULT_EVAL_RUN_DIR);
    let report_json = report_path_for_suite(suite);
    let mut command = build_eval_run_command(
        &argus_binary_path()?,
        root,
        config,
        suite,
        &out_dir,
        &report_json,
    );
    let output = command.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let status = if output.status.success() {
        "passed"
    } else {
        "failed"
    }
    .to_string();

    Ok(EvalRunOutput {
        suite: suite.to_path_buf(),
        out_dir,
        report_json,
        status,
        stdout,
        stderr,
    })
}

fn report_path_for_suite(suite: &Path) -> PathBuf {
    let stem = suite
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("suite");
    PathBuf::from(DEFAULT_EVAL_RUN_DIR).join(format!("{}.report.json", safe_file_stem(stem)))
}

fn safe_file_stem(stem: &str) -> String {
    let safe = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '-'
            }
        })
        .collect::<String>();
    safe.trim_matches('-').to_string()
}

pub(crate) fn build_eval_run_command(
    argus_path: &Path,
    root: &Path,
    config: &ArgusCodeConfig,
    suite: &Path,
    out_dir: &Path,
    report_json: &Path,
) -> Command {
    let mut command = Command::new(argus_path);
    command
        .current_dir(root)
        .arg("eval")
        .arg(suite)
        .arg("--provider")
        .arg(&config.provider.default_provider)
        .arg("--model")
        .arg(&config.provider.default_model)
        .arg("--out-dir")
        .arg(out_dir)
        .arg("--report-json")
        .arg(report_json)
        .arg("--in-place");
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
    fn eval_command_uses_provider_model_output_paths_and_api_key_alias() {
        std::env::set_var("ARGUS_TEST_EVAL_KEY", "secret");
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
                api_key_env: Some("ARGUS_TEST_EVAL_KEY".into()),
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

        let command = super::build_eval_run_command(
            Path::new("argus"),
            Path::new("/tmp/demo"),
            &config,
            Path::new(".argus/evals/smoke.json"),
            Path::new(".argus/eval-runs"),
            Path::new(".argus/eval-runs/smoke.report.json"),
        );
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert!(
            args.starts_with(&["eval".into(), ".argus/evals/smoke.json".into()]),
            "{args:?}"
        );
        assert!(
            args.windows(2).any(|pair| pair == ["--provider", "openai"]),
            "{args:?}"
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--model", "deepseek-chat"]),
            "{args:?}"
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--out-dir", ".argus/eval-runs"]),
            "{args:?}"
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--report-json", ".argus/eval-runs/smoke.report.json"]),
            "{args:?}"
        );
        assert!(
            args.iter().any(|arg| arg == "--in-place"),
            "workbench evals should verify the current workspace: {args:?}"
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
        std::env::remove_var("ARGUS_TEST_EVAL_KEY");
    }
}
