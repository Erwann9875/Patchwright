#![forbid(unsafe_code)]

use patchwright_core::action::Action;
use patchwright_core::error::{PatchwrightError, Result};
use patchwright_core::traits::ModelProvider;
use patchwright_core::types::{ModelRequest, ModelResponse};
use serde_json::Value;
use std::time::Duration;

pub use patchwright_model_contract::{build_openai_prompt as build_prompt, parse_action_json};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiConfig {
    pub base_url: String,
    pub model: String,
    pub api_key_env: String,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Mode {
    DryRun,
    Http,
}

#[derive(Debug, Clone)]
pub struct OpenAiCompatibleClient {
    config: OpenAiConfig,
    mode: Mode,
}

impl OpenAiCompatibleClient {
    pub fn new(config: OpenAiConfig) -> Self {
        Self {
            config,
            mode: Mode::Http,
        }
    }

    pub fn dry_run(config: OpenAiConfig) -> Self {
        Self {
            config,
            mode: Mode::DryRun,
        }
    }

    pub fn config(&self) -> &OpenAiConfig {
        &self.config
    }
}

impl ModelProvider for OpenAiCompatibleClient {
    fn propose_action(&mut self, request: ModelRequest) -> Result<ModelResponse> {
        let action = match self.mode {
            Mode::DryRun => Action::Finish {
                summary: format!(
                    "Dry run for model '{}': {}",
                    self.config.model, request.task.text
                ),
            },
            Mode::Http => self.propose_http_action(request)?,
        };

        Ok(ModelResponse { action })
    }
}

impl OpenAiCompatibleClient {
    fn propose_http_action(&self, request: ModelRequest) -> Result<Action> {
        let api_key = std::env::var(&self.config.api_key_env).map_err(|_| {
            PatchwrightError::Model(format!(
                "OpenAI-compatible API key environment variable '{}' is not set",
                self.config.api_key_env
            ))
        })?;
        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );
        let authorization = format!("Bearer {api_key}");
        let body = patchwright_model_contract::build_openai_prompt(&request, &self.config.model);

        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(self.config.timeout_seconds))
            .build();
        let response: Value = agent
            .post(&url)
            .set("Authorization", &authorization)
            .send_json(body)
            .map_err(|error| {
                PatchwrightError::Model(format!("OpenAI-compatible request failed: {error}"))
            })?
            .into_json()
            .map_err(|error| {
                PatchwrightError::Model(format!(
                    "OpenAI-compatible response was not valid JSON: {error}"
                ))
            })?;

        let content = response
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                PatchwrightError::Model(
                    "OpenAI-compatible response missing choices[0].message.content".to_string(),
                )
            })?;

        patchwright_model_contract::parse_action_json(content)
    }
}
