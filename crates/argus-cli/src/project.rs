use crate::compatibility::detect_rule_files;
use crate::config::{
    ArgusCodeConfig, MemoryConfig, ProjectConfig, ProviderConfig, RulesConfig, SecurityConfig,
    UiConfig, VerifyConfig, ARGUS_DIR, PROJECT_MEMORY_PATH, SMOKE_EVAL_PATH,
};
use anyhow::Result;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectProfile {
    pub root: PathBuf,
    pub name: String,
    pub languages: Vec<String>,
    pub package_manager: Option<String>,
    pub verify_commands: Vec<String>,
    pub rules_files: Vec<PathBuf>,
    pub detected_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitReport {
    pub config_path: PathBuf,
    pub memory_path: PathBuf,
    pub eval_path: PathBuf,
    pub profile: ProjectProfile,
    pub created_config: bool,
}

pub fn detect_project(start: &Path) -> Result<ProjectProfile> {
    let root = find_project_root(start)?;
    let name = root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("project")
        .to_string();

    let mut languages = BTreeSet::new();
    let mut detected_files = Vec::new();
    let mut package_manager = None;
    let mut verify_commands = Vec::new();

    detect_file(&root, "Cargo.toml", &mut detected_files, || {
        languages.insert("rust".to_string());
        package_manager = Some("cargo".to_string());
        verify_commands.push("cargo test --workspace --locked".to_string());
    });
    detect_file(&root, "package.json", &mut detected_files, || {
        languages.insert("javascript".to_string());
        package_manager = Some(detect_node_package_manager(&root));
        verify_commands.push(detect_node_verify_command(&root));
    });
    detect_file(&root, "pyproject.toml", &mut detected_files, || {
        languages.insert("python".to_string());
        if package_manager.is_none() {
            package_manager = Some("python".to_string());
        }
        verify_commands.push("python -m pytest".to_string());
    });
    detect_file(&root, "requirements.txt", &mut detected_files, || {
        languages.insert("python".to_string());
        if package_manager.is_none() {
            package_manager = Some("pip".to_string());
        }
        if !verify_commands.iter().any(|cmd| cmd == "python -m pytest") {
            verify_commands.push("python -m pytest".to_string());
        }
    });
    detect_file(&root, "go.mod", &mut detected_files, || {
        languages.insert("go".to_string());
        package_manager = Some("go".to_string());
        verify_commands.push("go test ./...".to_string());
    });

    if verify_commands.is_empty() {
        verify_commands.push("git status --short".to_string());
    }

    let rules_files = detect_rules_files(&root);

    Ok(ProjectProfile {
        root,
        name,
        languages: languages.into_iter().collect(),
        package_manager,
        verify_commands,
        rules_files,
        detected_files,
    })
}

fn detect_file<F>(root: &Path, rel: &str, detected_files: &mut Vec<PathBuf>, mut on_found: F)
where
    F: FnMut(),
{
    let path = root.join(rel);
    if path.exists() {
        detected_files.push(PathBuf::from(rel));
        on_found();
    }
}

fn detect_node_package_manager(root: &Path) -> String {
    if root.join("pnpm-lock.yaml").exists() {
        "pnpm".into()
    } else if root.join("yarn.lock").exists() {
        "yarn".into()
    } else if root.join("bun.lockb").exists() || root.join("bun.lock").exists() {
        "bun".into()
    } else {
        "npm".into()
    }
}

fn detect_node_verify_command(root: &Path) -> String {
    let manager = detect_node_package_manager(root);
    match manager.as_str() {
        "pnpm" => "pnpm test".into(),
        "yarn" => "yarn test".into(),
        "bun" => "bun test".into(),
        _ => "npm test".into(),
    }
}

fn detect_rules_files(root: &Path) -> Vec<PathBuf> {
    detect_rule_files(root)
}

fn find_project_root(start: &Path) -> Result<PathBuf> {
    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(start)
        .output()
    {
        if output.status.success() {
            let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !root.is_empty() {
                return Ok(PathBuf::from(root));
            }
        }
    }
    Ok(start.canonicalize()?)
}

pub fn build_config(profile: &ProjectProfile) -> ArgusCodeConfig {
    ArgusCodeConfig {
        schema_version: 1,
        project: ProjectConfig {
            name: profile.name.clone(),
            root: profile.root.display().to_string(),
            languages: profile.languages.clone(),
            package_manager: profile.package_manager.clone(),
        },
        provider: ProviderConfig {
            default_provider: "mock".into(),
            default_model: "mock".into(),
            routing: "manual".into(),
            base_url: None,
            api_key_env: None,
        },
        security: SecurityConfig::default(),
        mcp: crate::config::McpConfig::default(),
        verify: VerifyConfig {
            commands: profile.verify_commands.clone(),
            gate: true,
        },
        rules: RulesConfig {
            imported: profile
                .rules_files
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
        },
        memory: MemoryConfig {
            project: PROJECT_MEMORY_PATH.into(),
            lessons: ".argus/memory/lessons.md".into(),
        },
        ui: UiConfig {
            default_view: "workbench".into(),
            theme: "nocturne".into(),
        },
    }
}

pub fn init_project(start: &Path, force: bool) -> Result<InitReport> {
    let profile = detect_project(start)?;
    let config = build_config(&profile);
    let argus_dir = profile.root.join(ARGUS_DIR);
    std::fs::create_dir_all(argus_dir.join("memory"))?;
    std::fs::create_dir_all(argus_dir.join("evals"))?;
    std::fs::create_dir_all(argus_dir.join("tasks"))?;
    std::fs::create_dir_all(argus_dir.join("sessions"))?;

    let config_path = ArgusCodeConfig::path(&profile.root);
    let created_config = force || !config_path.exists();
    if created_config {
        config.write(&profile.root)?;
    }

    let memory_path = profile.root.join(PROJECT_MEMORY_PATH);
    if force || !memory_path.exists() {
        std::fs::write(&memory_path, project_memory_markdown(&profile))?;
    }

    let lessons_path = profile.root.join(".argus/memory/lessons.md");
    if force || !lessons_path.exists() {
        std::fs::write(
            lessons_path,
            "# ArgusCode Lessons\n\nFailures and durable project lessons will be recorded here.\n",
        )?;
    }

    let eval_path = profile.root.join(SMOKE_EVAL_PATH);
    if force || !eval_path.exists() {
        std::fs::write(&eval_path, smoke_eval_json(&profile)?)?;
    }

    Ok(InitReport {
        config_path,
        memory_path,
        eval_path,
        profile,
        created_config,
    })
}

pub fn project_memory_markdown(profile: &ProjectProfile) -> String {
    format!(
        "# ArgusCode Project Memory\n\n\
Project: {}\n\n\
Root: `{}`\n\n\
Languages: {}\n\n\
Package manager: {}\n\n\
Verification commands:\n{}\n\n\
Imported rules:\n{}\n\n\
Notes:\n- Keep this file concise and durable.\n- Add project-specific conventions that should survive across sessions.\n",
        profile.name,
        profile.root.display(),
        if profile.languages.is_empty() {
            "unknown".into()
        } else {
            profile.languages.join(", ")
        },
        profile.package_manager.as_deref().unwrap_or("unknown"),
        bullet_list(&profile.verify_commands),
        if profile.rules_files.is_empty() {
            "- none detected".into()
        } else {
            bullet_list(
                &profile
                    .rules_files
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>(),
            )
        }
    )
}

fn bullet_list(items: &[String]) -> String {
    items
        .iter()
        .map(|item| format!("- `{item}`"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn smoke_eval_json(profile: &ProjectProfile) -> Result<String> {
    let verify = profile.verify_commands.clone();
    let value = serde_json::json!({
        "name": format!("{} smoke", profile.name),
        "cases": [{
            "id": "project-smoke",
            "task": "Inspect the project and keep behavior unchanged. Do not edit files unless a verification command requires it.",
            "dir": "../..",
            "verify": verify
        }]
    });
    Ok(serde_json::to_string_pretty(&value)?)
}

pub fn init_report_text(report: &InitReport) -> String {
    let mut lines = Vec::new();
    lines.push("ArgusCode initialized".to_string());
    lines.push(format!("project: {}", report.profile.name));
    lines.push(format!("root: {}", report.profile.root.display()));
    lines.push(format!(
        "languages: {}",
        if report.profile.languages.is_empty() {
            "unknown".into()
        } else {
            report.profile.languages.join(", ")
        }
    ));
    lines.push(format!(
        "package manager: {}",
        report
            .profile
            .package_manager
            .as_deref()
            .unwrap_or("unknown")
    ));
    lines.push("verify:".to_string());
    for command in &report.profile.verify_commands {
        lines.push(format!("  - {command}"));
    }
    lines.push(format!("config: {}", report.config_path.display()));
    lines.push(format!("memory: {}", report.memory_path.display()));
    lines.push(format!("eval: {}", report.eval_path.display()));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("arguscode-{name}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn detects_rust_project_and_rules() {
        let dir = temp_dir("detect-rust");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"demo\"\n").unwrap();
        std::fs::write(dir.join("AGENTS.md"), "rules").unwrap();

        let profile = detect_project(&dir).unwrap();
        assert!(profile.languages.contains(&"rust".into()));
        assert_eq!(profile.package_manager.as_deref(), Some("cargo"));
        assert!(profile
            .verify_commands
            .contains(&"cargo test --workspace --locked".into()));
        assert!(profile.rules_files.contains(&PathBuf::from("AGENTS.md")));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detects_cross_agent_rule_files() {
        let dir = temp_dir("detect-agent-rules");
        std::fs::create_dir_all(dir.join(".cursor/rules")).unwrap();
        std::fs::write(dir.join("AGENTS.md"), "codex rules").unwrap();
        std::fs::write(dir.join("CLAUDE.md"), "claude rules").unwrap();
        std::fs::write(dir.join(".cursorrules"), "cursor legacy rules").unwrap();
        std::fs::write(dir.join(".cursor/rules/frontend.mdc"), "cursor rules").unwrap();
        std::fs::write(dir.join("GEMINI.md"), "gemini rules").unwrap();
        std::fs::write(dir.join("KIMI.md"), "kimi rules").unwrap();
        std::fs::write(dir.join("MIMO.md"), "mimo rules").unwrap();

        let profile = detect_project(&dir).unwrap();

        for expected in [
            "AGENTS.md",
            "CLAUDE.md",
            ".cursorrules",
            ".cursor/rules/frontend.mdc",
            "GEMINI.md",
            "KIMI.md",
            "MIMO.md",
        ] {
            assert!(
                profile.rules_files.contains(&PathBuf::from(expected)),
                "missing {expected}: {:?}",
                profile.rules_files
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn init_writes_config_memory_and_eval() {
        let dir = temp_dir("init");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("go.mod"), "module demo\n").unwrap();

        let report = init_project(&dir, false).unwrap();
        assert!(report.config_path.exists());
        assert!(report.memory_path.exists());
        assert!(report.eval_path.exists());
        let config = ArgusCodeConfig::read(&report.profile.root).unwrap();
        assert_eq!(config.verify.commands, vec!["go test ./..."]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn init_writes_smoke_eval_that_core_can_parse() {
        let dir = temp_dir("init-parseable-smoke-eval");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"demo\"\n").unwrap();

        let report = init_project(&dir, false).unwrap();
        let text = std::fs::read_to_string(&report.eval_path).unwrap();
        let suite: argus_core::EvalSuite = serde_json::from_str(&text).unwrap();

        assert!(suite.name.ends_with(" smoke"), "{}", suite.name);
        assert_eq!(suite.cases.len(), 1);
        assert_eq!(suite.cases[0].id, "project-smoke");
        assert_eq!(suite.cases[0].dir.as_deref(), Some("../.."));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
