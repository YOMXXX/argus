use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const ARGUS_DIR: &str = ".argus";
pub const CONFIG_PATH: &str = ".argus/config.toml";
pub const PROJECT_MEMORY_PATH: &str = ".argus/memory/project.md";
pub const SMOKE_EVAL_PATH: &str = ".argus/evals/smoke.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArgusCodeConfig {
    pub schema_version: u32,
    pub project: ProjectConfig,
    pub provider: ProviderConfig,
    pub verify: VerifyConfig,
    pub rules: RulesConfig,
    pub memory: MemoryConfig,
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectConfig {
    pub name: String,
    pub root: String,
    pub languages: Vec<String>,
    pub package_manager: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderConfig {
    pub default_provider: String,
    pub default_model: String,
    pub routing: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerifyConfig {
    pub commands: Vec<String>,
    pub gate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RulesConfig {
    pub imported: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryConfig {
    pub project: String,
    pub lessons: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UiConfig {
    pub default_view: String,
    pub theme: String,
}

impl ArgusCodeConfig {
    pub fn path(root: &Path) -> PathBuf {
        root.join(CONFIG_PATH)
    }

    pub fn read(root: &Path) -> Result<Self> {
        let path = Self::path(root);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", path.display()))?;
        toml::from_str(&text)
            .map_err(|e| anyhow::anyhow!("invalid ArgusCode config {}: {e}", path.display()))
    }

    pub fn write(&self, root: &Path) -> Result<PathBuf> {
        let path = Self::path(root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)?;
        std::fs::write(&path, text)?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_roundtrips_as_toml() {
        let cfg = ArgusCodeConfig {
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
            verify: VerifyConfig {
                commands: vec!["cargo test".into()],
                gate: true,
            },
            rules: RulesConfig {
                imported: vec!["AGENTS.md".into()],
            },
            memory: MemoryConfig {
                project: PROJECT_MEMORY_PATH.into(),
                lessons: ".argus/memory/lessons.md".into(),
            },
            ui: UiConfig {
                default_view: "workbench".into(),
                theme: "nocturne".into(),
            },
        };

        let text = toml::to_string(&cfg).unwrap();
        let parsed: ArgusCodeConfig = toml::from_str(&text).unwrap();
        assert_eq!(parsed, cfg);
    }
}
