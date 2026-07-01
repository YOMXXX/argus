use anyhow::Result;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRuleFile {
    pub agent: &'static str,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedRules {
    pub files: Vec<PathBuf>,
    pub prompt: String,
}

pub fn detect_rule_files(root: &Path) -> Vec<PathBuf> {
    detect_agent_rule_files(root)
        .into_iter()
        .map(|rule| rule.path)
        .collect()
}

pub fn detect_agent_rule_files(root: &Path) -> Vec<AgentRuleFile> {
    let mut files = Vec::new();
    for (rel, agent) in [
        ("AGENTS.md", "Codex / Argus"),
        ("CLAUDE.md", "Claude Code"),
        (".cursorrules", "Cursor"),
        ("GEMINI.md", "Gemini CLI"),
        ("KIMI.md", "KimiCode"),
        ("MIMO.md", "MiMoCode"),
    ] {
        push_if_file(root, rel, agent, &mut files);
    }
    push_rule_dir(root, ".cursor/rules", "Cursor", &mut files);
    files
}

pub fn load_auto_rules(root: &Path) -> Result<Option<LoadedRules>> {
    let mut files = Vec::new();
    let mut sections = Vec::new();
    for path in detect_rule_files(root) {
        let full_path = root.join(&path);
        let content = std::fs::read_to_string(&full_path)
            .map_err(|e| anyhow::anyhow!("failed to read rules {}: {e}", full_path.display()))?;
        let trimmed = content.trim();
        if trimmed.is_empty() {
            continue;
        }
        files.push(path.clone());
        sections.push(format!("## Rules from {}\n\n{trimmed}", path.display()));
    }
    if sections.is_empty() {
        Ok(None)
    } else {
        Ok(Some(LoadedRules {
            files,
            prompt: sections.join("\n\n"),
        }))
    }
}

pub fn render_agent_compatibility(root: &Path) -> String {
    let rules = detect_agent_rule_files(root);
    let mut lines = vec!["Agent compatibility".to_string()];
    for agent in [
        "Codex / Argus",
        "Claude Code",
        "Cursor",
        "Gemini CLI",
        "KimiCode",
        "MiMoCode",
    ] {
        let paths = rules
            .iter()
            .filter(|rule| rule.agent == agent)
            .map(|rule| rule.path.display().to_string())
            .collect::<Vec<_>>();
        if paths.is_empty() {
            lines.push(format!("- {agent}: no native rule file detected"));
        } else {
            lines.push(format!("- {agent}: {}", paths.join(", ")));
        }
    }
    for signal in detect_config_signals(root) {
        lines.push(format!("- {signal}"));
    }
    lines.join("\n")
}

fn push_if_file(root: &Path, rel: &str, agent: &'static str, files: &mut Vec<AgentRuleFile>) {
    if root.join(rel).is_file() {
        files.push(AgentRuleFile {
            agent,
            path: PathBuf::from(rel),
        });
    }
}

fn push_rule_dir(root: &Path, rel: &str, agent: &'static str, files: &mut Vec<AgentRuleFile>) {
    let dir = root.join(rel);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter_map(|path| path.strip_prefix(root).ok().map(Path::to_path_buf))
        .collect::<Vec<_>>();
    paths.sort();
    files.extend(paths.into_iter().map(|path| AgentRuleFile { agent, path }));
}

fn detect_config_signals(root: &Path) -> Vec<String> {
    let mut signals = Vec::new();
    for rel in [
        ".aider.conf.yml",
        ".aider.conf.yaml",
        "aider.conf.yml",
        "aider.conf.yaml",
    ] {
        if root.join(rel).is_file() {
            signals.push(format!("Aider config detected: {rel}"));
        }
    }
    signals
}
