#![forbid(unsafe_code)]

use patchwright_core::action::Action;
use patchwright_core::error::{PatchwrightError, Result};
use patchwright_core::types::ArchitectureDesign;
use patchwright_core::types::{
    ContextPack, FileQuery, LineRange, ModelRequest, Patch, RepoPath, SearchQuery,
};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

const USER_PROMPT_MAX_CHARS: usize = 24 * 1024;
const USER_PROMPT_SECTION_MAX_CHARS: usize = 7 * 1024;
const TRUNCATION_MARKER: &str = "\n...[truncated]";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub system: String,
    pub user: String,
}

pub fn build_prompt(request: &ModelRequest) -> Prompt {
    Prompt {
        system: system_prompt().to_owned(),
        user: user_prompt(request),
    }
}

pub fn build_openai_prompt(request: &ModelRequest, model: &str) -> Value {
    let prompt = build_prompt(request);
    json!({
        "model": model,
        "messages": [
            {
                "role": "system",
                "content": prompt.system,
            },
            {
                "role": "user",
                "content": prompt.user,
            }
        ]
    })
}

pub fn render_exec_prompt(request: &ModelRequest) -> String {
    let prompt = build_prompt(request);
    format!(
        "{}\n\nYou are running as a model provider transport. Do not edit files, run commands, or claim success. Return one final JSON object that matches the supplied schema.\n\n{}",
        prompt.system, prompt.user
    )
}

pub fn render_design_exec_prompt(request: &ModelRequest) -> String {
    format!(
        "{}\n\nYou are running as a model provider transport. Do not edit files, run commands, or claim success. Return one final JSON object that matches the supplied architecture design schema.\n\n{}",
        design_system_prompt(),
        user_prompt(request)
    )
}

pub fn action_output_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": [
            "action",
            "path",
            "start",
            "end",
            "pattern",
            "root",
            "unified_diff",
            "summary"
        ],
        "properties": {
            "action": {
                "type": "string",
                "enum": [
                    "read_file",
                    "search_text",
                    "list_files",
                    "apply_patch",
                    "run_verifier",
                    "run_tests",
                    "run_typecheck",
                    "run_benchmark",
                    "finish"
                ]
            },
            "path": { "type": ["string", "null"] },
            "start": { "type": ["integer", "null"], "minimum": 1 },
            "end": { "type": ["integer", "null"], "minimum": 1 },
            "pattern": { "type": ["string", "null"] },
            "root": { "type": ["string", "null"] },
            "unified_diff": { "type": ["string", "null"] },
            "summary": { "type": ["string", "null"] }
        }
    })
}

pub fn write_action_output_schema(path: &Path) -> Result<()> {
    let schema = serde_json::to_vec_pretty(&action_output_schema()).map_err(|error| {
        PatchwrightError::Model(format!("failed to serialize action schema: {error}"))
    })?;
    fs::write(path, schema).map_err(PatchwrightError::from)
}

pub fn write_architecture_design_schema(path: &Path) -> Result<()> {
    let schema = serde_json::to_vec_pretty(&architecture_design_schema()).map_err(|error| {
        PatchwrightError::Model(format!(
            "failed to serialize architecture design schema: {error}"
        ))
    })?;
    fs::write(path, schema).map_err(PatchwrightError::from)
}

pub fn parse_architecture_design_json(content: &str) -> Result<ArchitectureDesign> {
    serde_json::from_str(content).map_err(|error| {
        PatchwrightError::Model(format!(
            "model architecture design content was not valid JSON: {error}"
        ))
    })
}

pub fn design_system_prompt() -> &'static str {
    r#"Create a senior-engineer architecture design for the requested task.

Do not edit files. Do not claim implementation is complete. Every architecture claim should cite concrete file evidence when available.

The design must compare viable options, recommend one, identify affected files, define an implementation sequence, and include test, migration, rollback, risk, open-question, and acceptance-criteria sections."#
}

pub fn architecture_design_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": [
            "title",
            "goal",
            "current_architecture",
            "assumptions",
            "non_goals",
            "options",
            "recommendation",
            "file_impact",
            "implementation_plan",
            "test_strategy",
            "risks",
            "open_questions",
            "acceptance_criteria"
        ],
        "properties": {
            "title": string_schema(),
            "goal": string_schema(),
            "current_architecture": array_of(architecture_finding_schema()),
            "assumptions": string_array_schema(),
            "non_goals": string_array_schema(),
            "options": array_of(design_option_schema()),
            "recommendation": recommended_design_schema(),
            "file_impact": array_of(file_impact_schema()),
            "implementation_plan": array_of(plan_step_schema()),
            "test_strategy": test_strategy_schema(),
            "migration_plan": nullable_string_schema(),
            "rollback_plan": nullable_string_schema(),
            "risks": array_of(risk_schema()),
            "open_questions": string_array_schema(),
            "acceptance_criteria": string_array_schema()
        }
    })
}

fn architecture_finding_schema() -> Value {
    object_with_required(
        ["summary", "evidence"],
        json!({
            "summary": string_schema(),
            "evidence": evidence_array_schema()
        }),
    )
}

fn design_option_schema() -> Value {
    object_with_required(
        ["name", "summary", "pros", "cons", "evidence"],
        json!({
            "name": string_schema(),
            "summary": string_schema(),
            "pros": string_array_schema(),
            "cons": string_array_schema(),
            "evidence": evidence_array_schema()
        }),
    )
}

fn recommended_design_schema() -> Value {
    object_with_required(
        ["option_name", "rationale", "evidence"],
        json!({
            "option_name": string_schema(),
            "rationale": string_schema(),
            "evidence": evidence_array_schema()
        }),
    )
}

fn file_impact_schema() -> Value {
    object_with_required(
        ["path", "change_summary", "risk", "evidence"],
        json!({
            "path": string_schema(),
            "change_summary": string_schema(),
            "risk": nullable_string_schema(),
            "evidence": evidence_array_schema()
        }),
    )
}

fn plan_step_schema() -> Value {
    object_with_required(
        [
            "id",
            "title",
            "description",
            "depends_on",
            "target_files",
            "acceptance_criteria",
            "verification_commands",
        ],
        json!({
            "id": string_schema(),
            "title": string_schema(),
            "description": string_schema(),
            "depends_on": string_array_schema(),
            "target_files": string_array_schema(),
            "acceptance_criteria": string_array_schema(),
            "verification_commands": string_array_schema()
        }),
    )
}

fn test_strategy_schema() -> Value {
    object_with_required(
        ["unit", "integration", "end_to_end", "manual", "commands"],
        json!({
            "unit": string_array_schema(),
            "integration": string_array_schema(),
            "end_to_end": string_array_schema(),
            "manual": string_array_schema(),
            "commands": string_array_schema()
        }),
    )
}

fn risk_schema() -> Value {
    object_with_required(
        ["title", "impact", "mitigation", "evidence"],
        json!({
            "title": string_schema(),
            "impact": string_schema(),
            "mitigation": string_schema(),
            "evidence": evidence_array_schema()
        }),
    )
}

fn evidence_array_schema() -> Value {
    array_of(object_with_required(
        ["path", "start_line", "end_line", "reason"],
        json!({
            "path": string_schema(),
            "start_line": nullable_integer_schema(),
            "end_line": nullable_integer_schema(),
            "reason": string_schema()
        }),
    ))
}

fn object_with_required<const N: usize>(required: [&str; N], properties: Value) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": required.to_vec(),
        "properties": properties
    })
}

fn array_of(items: Value) -> Value {
    json!({
        "type": "array",
        "items": items
    })
}

fn string_array_schema() -> Value {
    array_of(string_schema())
}

fn string_schema() -> Value {
    json!({ "type": "string" })
}

fn nullable_string_schema() -> Value {
    json!({ "type": ["string", "null"] })
}

fn nullable_integer_schema() -> Value {
    json!({ "type": ["integer", "null"], "minimum": 1 })
}

pub fn system_prompt() -> &'static str {
    r#"Return only one JSON action. Do not include Markdown fences, commentary, or multiple actions.

Verification decides success. Passing tests or your confidence are not enough unless the verifier accepts the work.

Core rules:
- Choose exactly one allowed action for the next step.
- Inspect files before editing when the needed context is missing.
- Apply the smallest patch that addresses the task and counterexamples.
- After editing, run verification before finishing.
- Finish only when verification has accepted the change.
- Set fields that are not used by the selected action to null.

Allowed action examples:
{"action":"read_file","path":"src/lib.rs","start":1,"end":120,"pattern":null,"root":null,"unified_diff":null,"summary":null}
{"action":"apply_patch","path":null,"start":null,"end":null,"pattern":null,"root":null,"unified_diff":"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn add(a: i32, b: i32) -> i32 {\n-    a - b\n+    a + b\n }\n","summary":null}
{"action":"run_verifier","path":null,"start":null,"end":null,"pattern":null,"root":null,"unified_diff":null,"summary":null}
{"action":"finish","path":null,"start":null,"end":null,"pattern":null,"root":null,"unified_diff":null,"summary":"fixed the failing add test"}"#
}

fn user_prompt(request: &ModelRequest) -> String {
    let observations = format!("{:#?}", request.observations);
    let counterexamples = format!("{:#?}", request.counterexamples);
    let context = request.context.as_ref().map(render_context);

    let mut sections = vec![format!(
        "Task:\n{}",
        truncate_chars(&request.task.text, USER_PROMPT_SECTION_MAX_CHARS)
    )];
    if let Some(context) = context {
        sections.push(format!(
            "Context:\n{}",
            truncate_chars(&context, USER_PROMPT_SECTION_MAX_CHARS)
        ));
    }
    sections.push(format!(
        "Observations:\n{}",
        truncate_chars(&observations, USER_PROMPT_SECTION_MAX_CHARS)
    ));
    sections.push(format!(
        "Counterexamples:\n{}",
        truncate_chars(&counterexamples, USER_PROMPT_SECTION_MAX_CHARS)
    ));

    let prompt = sections.join("\n\n");
    truncate_chars(&prompt, USER_PROMPT_MAX_CHARS)
}

fn render_context(context: &ContextPack) -> String {
    let mut output = String::new();

    output.push_str("Ranked files:\n");
    if context.files.is_empty() {
        output.push_str("- none\n");
    } else {
        for file in &context.files {
            output.push_str(&format!("- {} (score {})\n", file.path.0, file.score));
        }
    }

    output.push_str("Likely tests:\n");
    if context.likely_tests.is_empty() {
        output.push_str("- none\n");
    } else {
        for path in &context.likely_tests {
            output.push_str(&format!("- {}\n", path.0));
        }
    }

    output.push_str("Manifests:\n");
    if context.manifests.is_empty() {
        output.push_str("- none\n");
    } else {
        for path in &context.manifests {
            output.push_str(&format!("- {}\n", path.0));
        }
    }

    output
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
    if path.is_null() {
        return Ok(None);
    }
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
        (Some(start), Some(end)) if start.is_null() && end.is_null() => Ok(None),
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
