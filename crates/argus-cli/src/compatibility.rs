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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentCommandMapping {
    pub source: &'static str,
    pub familiar: &'static str,
    pub arguscode: &'static str,
    pub tui: &'static str,
    pub purpose: &'static str,
}

impl AgentCommandMapping {
    fn matches_query(&self, query: &str) -> bool {
        self.source.to_ascii_lowercase().contains(query)
            || self.familiar.to_ascii_lowercase().contains(query)
            || self.arguscode.to_ascii_lowercase().contains(query)
            || self.tui.to_ascii_lowercase().contains(query)
            || self.purpose.to_ascii_lowercase().contains(query)
    }
}

pub const AGENT_COMMAND_MAPPINGS: &[AgentCommandMapping] = &[
    AgentCommandMapping {
        source: "Claude Code",
        familiar: "claude",
        arguscode: "arguscode",
        tui: "arguscode",
        purpose: "Open the ArgusCode TUI workbench",
    },
    AgentCommandMapping {
        source: "Claude Code",
        familiar: "claude \"fix login\"",
        arguscode: "arguscode chat \"fix login\"",
        tui: "/ask fix login",
        purpose: "Queue a conversational coding task and enter the workbench",
    },
    AgentCommandMapping {
        source: "Codex CLI",
        familiar: "codex",
        arguscode: "arguscode",
        tui: "arguscode",
        purpose: "Open the ArgusCode TUI workbench",
    },
    AgentCommandMapping {
        source: "Codex CLI",
        familiar: "codex exec \"implement auth\"",
        arguscode: "arguscode task \"implement auth\"",
        tui: "/code implement auth",
        purpose: "Queue a coding task without leaving the current repo",
    },
    AgentCommandMapping {
        source: "KimiCode",
        familiar: "kimicode fix \"parser bug\"",
        arguscode: "arguscode fix \"parser bug\"",
        tui: "/fix parser bug",
        purpose: "Queue a focused fix task",
    },
    AgentCommandMapping {
        source: "MiMoCode",
        familiar: "mimocode edit \"settings UI\"",
        arguscode: "arguscode edit \"settings UI\"",
        tui: "/edit settings UI",
        purpose: "Queue an edit task using familiar agent wording",
    },
    AgentCommandMapping {
        source: "Any agent",
        familiar: "implement \"feature\"",
        arguscode: "arguscode implement \"feature\"",
        tui: "/implement feature",
        purpose: "Queue an implementation task",
    },
    AgentCommandMapping {
        source: "Any agent",
        familiar: "continue",
        arguscode: "arguscode resume --run",
        tui: "/continue",
        purpose: "Resume the latest queued task through the harness",
    },
    AgentCommandMapping {
        source: "Any agent",
        familiar: "check / test",
        arguscode: "arguscode verify",
        tui: "/check",
        purpose: "Run the configured verification gate",
    },
    AgentCommandMapping {
        source: "Any agent",
        familiar: "doctor / health",
        arguscode: "arguscode doctor",
        tui: "/doctor",
        purpose: "Show project and agent compatibility signals",
    },
    AgentCommandMapping {
        source: "Any agent",
        familiar: "logs / output",
        arguscode: "arguscode history",
        tui: "/logs",
        purpose: "Inspect terminal output and recorded sessions",
    },
    AgentCommandMapping {
        source: "Any agent",
        familiar: "model / provider",
        arguscode: "arguscode provider",
        tui: "/provider",
        purpose: "Show or update the active model/provider profile",
    },
    AgentCommandMapping {
        source: "ArgusCode",
        familiar: "commands / cheatsheet",
        arguscode: "arguscode commands [query]",
        tui: "/commands [query]",
        purpose: "Search this compatibility command guide",
    },
];

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

pub fn render_agent_command_catalog(query: Option<&str>) -> String {
    let query = query.map(str::trim).filter(|query| !query.is_empty());
    let needle = query.map(str::to_ascii_lowercase);
    let mut lines = vec![
        "Agent command guide".to_string(),
        "Use familiar Claude Code, Codex CLI, KimiCode, MiMoCode, and general agent habits in ArgusCode.".to_string(),
        "CLI: arguscode commands [query] | TUI: /commands [query]".to_string(),
    ];
    if let Some(query) = query {
        lines.push(format!("Filter: {query}"));
    }
    lines.push(String::new());

    let mut matches = 0usize;
    for mapping in AGENT_COMMAND_MAPPINGS {
        if let Some(needle) = needle.as_deref() {
            if !mapping.matches_query(needle) {
                continue;
            }
        }
        matches += 1;
        lines.push(format!(
            "- {} | familiar: {} | arguscode: {} | tui: {} | {}",
            mapping.source, mapping.familiar, mapping.arguscode, mapping.tui, mapping.purpose
        ));
    }

    if matches == 0 {
        lines.push(format!(
            "No compatible command matched \"{}\".",
            query.unwrap_or_default()
        ));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_catalog_renders_migration_entries() {
        let catalog = render_agent_command_catalog(None);

        assert!(catalog.contains("Agent command guide"), "{catalog}");
        assert!(catalog.contains("Claude Code"), "{catalog}");
        assert!(catalog.contains("Codex CLI"), "{catalog}");
        assert!(catalog.contains("KimiCode"), "{catalog}");
        assert!(catalog.contains("arguscode fix"), "{catalog}");
        assert!(catalog.contains("/commands [query]"), "{catalog}");
    }

    #[test]
    fn command_catalog_filters_by_query() {
        let catalog = render_agent_command_catalog(Some("fix"));

        assert!(catalog.contains("Filter: fix"), "{catalog}");
        assert!(catalog.contains("arguscode fix"), "{catalog}");
        assert!(catalog.contains("/fix"), "{catalog}");
        assert!(!catalog.contains("arguscode provider"), "{catalog}");
    }

    #[test]
    fn command_catalog_reports_empty_filter_results() {
        let catalog = render_agent_command_catalog(Some("no-such-agent-command"));

        assert!(
            catalog.contains("No compatible command matched"),
            "{catalog}"
        );
    }
}
