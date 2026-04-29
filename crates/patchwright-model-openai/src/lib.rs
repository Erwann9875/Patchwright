#![forbid(unsafe_code)]

use patchwright_core::action::Action;
use patchwright_core::error::{PatchwrightError, Result};
use patchwright_core::traits::ModelProvider;
use patchwright_core::types::{
    FileQuery, LineRange, ModelRequest, ModelResponse, Patch, RepoPath, SearchQuery,
};
use serde_json::{json, Value};
use std::time::Duration;

const USER_PROMPT_MAX_CHARS: usize = 24 * 1024;
const USER_PROMPT_SECTION_MAX_CHARS: usize = 7 * 1024;
const TRUNCATION_MARKER: &str = "\n...[truncated]";

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
        let body = build_prompt(&request, &self.config.model);

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

pub fn build_prompt(request: &ModelRequest, model: &str) -> Value {
    json!({
        "model": model,
        "messages": [
            {
                "role": "system",
                "content": system_prompt(),
            },
            {
                "role": "user",
                "content": user_prompt(request),
            }
        ]
    })
}

fn system_prompt() -> &'static str {
    r#"Return only one JSON action. Do not include Markdown fences, commentary, or multiple actions.

Verification decides success. Passing tests or your confidence are not enough unless the verifier accepts the work.

Core rules:
- Choose exactly one allowed action for the next step.
- Inspect files before editing when the needed context is missing.
- Apply the smallest patch that addresses the task and counterexamples.
- After editing, run verification before finishing.
- Finish only when verification has accepted the change.

Allowed action examples:
{"action":"read_file","path":"src/lib.rs","start":1,"end":120}
{"action":"apply_patch","unified_diff":"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn add(a: i32, b: i32) -> i32 {\n-    a - b\n+    a + b\n }\n"}
{"action":"run_verifier"}
{"action":"finish","summary":"fixed the failing add test"}"#
}

fn user_prompt(request: &ModelRequest) -> String {
    let observations = format!("{:#?}", request.observations);
    let counterexamples = format!("{:#?}", request.counterexamples);

    let prompt = format!(
        "Task:\n{}\n\nObservations:\n{}\n\nCounterexamples:\n{}",
        truncate_chars(&request.task.text, USER_PROMPT_SECTION_MAX_CHARS),
        truncate_chars(&observations, USER_PROMPT_SECTION_MAX_CHARS),
        truncate_chars(&counterexamples, USER_PROMPT_SECTION_MAX_CHARS)
    );

    truncate_chars(&prompt, USER_PROMPT_MAX_CHARS)
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }

    let marker_chars = TRUNCATION_MARKER.chars().count();
    if max_chars <= marker_chars {
        return TRUNCATION_MARKER.chars().take(max_chars).collect();
    }

    let mut truncated = value
        .chars()
        .take(max_chars - marker_chars)
        .collect::<String>();
    truncated.push_str(TRUNCATION_MARKER);
    truncated
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
