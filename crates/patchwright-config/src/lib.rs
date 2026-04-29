#![forbid(unsafe_code)]

use patchwright_core::{PatchwrightError, Result};
use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PatchwrightConfig {
    pub model: ModelConfig,
    pub agent: AgentConfig,
    pub policy: PolicyConfig,
    pub rust: RustConfig,
}

impl PatchwrightConfig {
    pub fn load(repo: &Path) -> Result<Self> {
        if !repo.is_dir() {
            return Err(PatchwrightError::InvalidInput(format!(
                "repo path must be a directory: {}",
                repo.display()
            )));
        }

        let path = repo.join("patchwright.toml");
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path)?;
        Self::from_toml_str(&content)
    }

    pub fn from_toml_str(content: &str) -> Result<Self> {
        let config = toml::from_str::<Self>(content).map_err(|error| {
            PatchwrightError::InvalidInput(format!("failed to parse patchwright.toml: {error}"))
        })?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        if self.model.codex_cli.command.trim().is_empty() {
            return Err(PatchwrightError::InvalidInput(
                "model.codex_cli.command must not be empty".to_owned(),
            ));
        }

        if self.model.openai.timeout_seconds == 0 {
            return Err(PatchwrightError::InvalidInput(
                "model.openai.timeout_seconds must be greater than 0".to_owned(),
            ));
        }

        if self.agent.max_steps == 0 {
            return Err(PatchwrightError::InvalidInput(
                "agent.max_steps must be greater than 0".to_owned(),
            ));
        }

        if self.agent.max_changed_files == 0 {
            return Err(PatchwrightError::InvalidInput(
                "agent.max_changed_files must be greater than 0".to_owned(),
            ));
        }

        if self.agent.max_inserted_lines == 0 {
            return Err(PatchwrightError::InvalidInput(
                "agent.max_inserted_lines must be greater than 0".to_owned(),
            ));
        }

        if self.policy.allowed_programs.is_empty() {
            return Err(PatchwrightError::InvalidInput(
                "policy.allowed_programs must not be empty".to_owned(),
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModelConfig {
    pub provider: ModelProviderKind,
    pub base_url: String,
    pub model: Option<String>,
    pub api_key_env: String,
    pub codex_cli: CodexCliModelConfig,
    pub openai: OpenAiModelConfig,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            provider: ModelProviderKind::default(),
            base_url: "https://api.openai.com/v1".to_owned(),
            model: None,
            api_key_env: "OPENAI_API_KEY".to_owned(),
            codex_cli: CodexCliModelConfig::default(),
            openai: OpenAiModelConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub enum ModelProviderKind {
    #[serde(rename = "codex-cli")]
    #[default]
    CodexCli,
    #[serde(rename = "openai-compatible")]
    OpenAiCompatible,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CodexCliModelConfig {
    pub command: String,
    pub model: Option<String>,
}

impl Default for CodexCliModelConfig {
    fn default() -> Self {
        Self {
            command: "codex".to_owned(),
            model: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OpenAiModelConfig {
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub api_key_env: Option<String>,
    pub timeout_seconds: u64,
}

impl Default for OpenAiModelConfig {
    fn default() -> Self {
        Self {
            base_url: None,
            model: None,
            api_key_env: None,
            timeout_seconds: 30,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AgentConfig {
    pub max_steps: usize,
    pub require_patch: bool,
    pub max_changed_files: usize,
    pub max_inserted_lines: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_steps: 30,
            require_patch: true,
            max_changed_files: 5,
            max_inserted_lines: 300,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PolicyConfig {
    pub allowed_programs: Vec<String>,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            allowed_programs: vec!["cargo".to_owned()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RustConfig {
    pub fmt: bool,
    pub check: bool,
    pub test: bool,
    pub clippy: bool,
}

impl Default for RustConfig {
    fn default() -> Self {
        Self {
            fmt: true,
            check: true,
            test: true,
            clippy: false,
        }
    }
}
