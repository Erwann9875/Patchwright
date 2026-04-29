#![forbid(unsafe_code)]

use patchwright_core::action::Action;
use patchwright_core::error::{PatchwrightError, Result};
use patchwright_core::traits::ModelProvider;
use patchwright_core::types::{
    FileQuery, LineRange, ModelRequest, ModelResponse, Patch, RepoPath, SearchQuery,
};
use serde_json::{json, Value};
use std::time::Duration;

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
        let body = json!({
            "model": self.config.model,
            "messages": [
                {
                    "role": "system",
                    "content": "Return only a JSON Patchwright action object. Supported shape: {\"action\":\"finish\",\"summary\":\"...\"}."
                },
                {
                    "role": "user",
                    "content": request.task.text
                }
            ]
        });

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

        parse_action_json(content)
    }
}

pub fn parse_action_json(content: &str) -> Result<Action> {
    let value: Value = serde_json::from_str(content).map_err(|error| {
        PatchwrightError::Model(format!("model action content was not valid JSON: {error}"))
    })?;

    let action = value.get("action").and_then(Value::as_str).ok_or_else(|| {
        PatchwrightError::Model("model action JSON missing string field 'action'".to_string())
    })?;

    match action {
        "read_file" => Ok(Action::ReadFile {
            path: required_repo_path(&value, "path")?,
            range: optional_line_range(&value)?,
        }),
        "search_text" => Ok(Action::SearchText(SearchQuery {
            pattern: required_string_field(&value, "pattern")?.to_string(),
            root: optional_repo_path(&value, "root")?,
        })),
        "list_files" => Ok(Action::ListFiles(FileQuery {
            root: optional_repo_path(&value, "root")?,
        })),
        "apply_patch" => Ok(Action::ApplyPatch(Patch {
            unified_diff: required_string_field(&value, "unified_diff")?.to_string(),
        })),
        "run_verifier" => Ok(Action::RunVerifier),
        "run_tests" => Ok(Action::RunTests),
        "run_typecheck" => Ok(Action::RunTypecheck),
        "run_benchmark" => Ok(Action::RunBenchmark),
        "finish" => {
            let summary = required_string_field(&value, "summary")?;
            Ok(Action::Finish {
                summary: summary.to_string(),
            })
        }
        unsupported => Err(PatchwrightError::Model(format!(
            "unsupported model action '{unsupported}'"
        ))),
    }
}

fn required_string_field<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value.get(field).and_then(Value::as_str).ok_or_else(|| {
        PatchwrightError::Model(format!("model action JSON missing string field '{field}'"))
    })
}

fn required_repo_path(value: &Value, field: &str) -> Result<RepoPath> {
    let path = required_string_field(value, field)?;
    normalized_relative_repo_path(path)
}

fn optional_repo_path(value: &Value, field: &str) -> Result<Option<RepoPath>> {
    let Some(path) = value.get(field) else {
        return Ok(None);
    };
    let path = path.as_str().ok_or_else(|| {
        PatchwrightError::Model(format!(
            "model action JSON field '{field}' must be a string"
        ))
    })?;
    normalized_relative_repo_path(path).map(Some)
}

fn normalized_relative_repo_path(path: &str) -> Result<RepoPath> {
    if path.trim().is_empty()
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.contains(':')
        || has_windows_prefix(path)
    {
        return Err(relative_path_error(path));
    }

    let mut parts = Vec::new();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => return Err(relative_path_error(path)),
            part => parts.push(part),
        }
    }

    if parts.is_empty() {
        return Err(relative_path_error(path));
    }

    Ok(RepoPath::new(parts.join("/")))
}

fn has_windows_prefix(path: &str) -> bool {
    path.as_bytes().get(1).is_some_and(|byte| *byte == b':')
}

fn relative_path_error(path: &str) -> PatchwrightError {
    PatchwrightError::Model(format!(
        "model repo path must be a normalized relative path, got '{path}'"
    ))
}

fn optional_line_range(value: &Value) -> Result<Option<LineRange>> {
    match (value.get("start"), value.get("end")) {
        (None, None) => Ok(None),
        (Some(start), Some(end)) => {
            let start = line_number(start, "start")?;
            let end = line_number(end, "end")?;
            if start == 0 || end < start {
                return Err(PatchwrightError::Model(
                    "read_file action JSON has invalid line range".to_string(),
                ));
            }
            Ok(Some(LineRange { start, end }))
        }
        _ => Err(PatchwrightError::Model(
            "read_file action JSON must include both 'start' and 'end' or neither".to_string(),
        )),
    }
}

fn line_number(value: &Value, field: &str) -> Result<usize> {
    let number = value.as_u64().ok_or_else(|| {
        PatchwrightError::Model(format!(
            "read_file action JSON field '{field}' must be a number"
        ))
    })?;
    usize::try_from(number).map_err(|_| {
        PatchwrightError::Model(format!(
            "read_file action JSON field '{field}' is too large"
        ))
    })
}
