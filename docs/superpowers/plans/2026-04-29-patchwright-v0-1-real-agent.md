# Patchwright v0.1 Real Agent Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn Patchwright from a verified foundation into a minimal real coding agent that can ask an OpenAI-compatible model for structured actions, patch a tiny Rust repo, run verification, and accept only evidence-backed fixes.

**Architecture:** Keep the core language-agnostic and controller-owned. The model returns one JSON action at a time; the controller executes structured tools inside a temporary git worktree, verifies patches, rejects unsafe diffs, and feeds observations/counterexamples back into the next model call. The first working path is Rust-only through `RustAdapter`, local execution, and Cargo verification.

**Tech Stack:** Rust workspace, `serde_json`, `ureq`, Cargo, git worktrees, local test fixtures, optional `serde`/`toml` for config parsing.

---

## Scope

This plan implements the next v0.1 slice:

- Parse model JSON for structured Patchwright actions.
- Send the model enough state to act like a tool-using agent.
- Wire real OpenAI-compatible model mode into `patchwright solve`.
- Prevent unverified `finish` from satisfying code-change tasks.
- Add a sandboxed end-to-end Rust bugfix fixture.
- Add a cheap context pack before any embedding work.
- Load `patchwright.toml` for model, policy, agent, and Rust verifier settings.

This plan does not implement Docker, embeddings, multi-agent debate, GitHub apps, VS Code extensions, a web UI, fine-tuning, MCP marketplace integration, or non-Rust language adapters.

## File Structure

- `crates/patchwright-core/src/action.rs`
  - Keep model-visible action enum. `RevertAttempt` remains controller/internal in v0.1.
- `crates/patchwright-core/src/agent.rs`
  - Enforce `require_patch` for code-change tasks.
  - Optionally seed model context once the context pack exists.
- `crates/patchwright-core/src/types.rs`
  - Add `TaskSpec.require_patch`.
  - Add `ContextPack` if shared across model/index/core.
- `crates/patchwright-core/tests/agent_loop.rs`
  - Add finish-gate tests.
- `crates/patchwright-cli/tests/verified_patch_fixture.rs`
  - Add end-to-end sandboxed Rust patch fixture using the assembled crates.
- `crates/patchwright-model-openai/src/lib.rs`
  - Parse structured actions.
  - Build OpenAI-compatible prompts from task, observations, and counterexamples.
- `crates/patchwright-model-openai/tests/openai_client.rs`
  - Add parser and prompt contract tests.
- `crates/patchwright-cli/src/main.rs`
  - Parse `solve` model/config flags.
  - Select dry-run or real OpenAI-compatible client.
  - Print accepted patch verification details.
- `crates/patchwright-cli/Cargo.toml`
  - Add `patchwright-config` dependency after Task 7.
- `crates/patchwright-index/src/lib.rs`
  - Add cheap `ContextPack` builder or helper.
- `crates/patchwright-index/tests/basic_index.rs`
  - Add context ranking tests.
- `crates/patchwright-lang-rust/src/lib.rs`
  - Add Rust-specific context weighting if the generic indexer needs language hints.
- `crates/patchwright-lang-rust/tests/rust_adapter.rs`
  - Add context or ranking tests.
- `crates/patchwright-config/Cargo.toml`
  - New crate in Task 7.
- `crates/patchwright-config/src/lib.rs`
  - Parse and validate `patchwright.toml`.
- `crates/patchwright-config/tests/config.rs`
  - Config defaults and validation tests.
- `Cargo.toml`
  - Add `crates/patchwright-config` when Task 7 creates it.
- `README.md`
  - Update the demo command and config example.

## Task 1: Parse Structured Agent Actions

**Commit:** `feat(model-openai): parse structured agent actions`

**Files:**
- Modify: `crates/patchwright-model-openai/src/lib.rs`
- Modify: `crates/patchwright-model-openai/tests/openai_client.rs`

- [ ] **Step 1: Add parser tests first**

Append tests to `crates/patchwright-model-openai/tests/openai_client.rs`:

```rust
use patchwright_core::types::{LineRange, Patch, RepoPath, SearchQuery, FileQuery};

#[test]
fn parses_read_file_action_json() {
    let action = parse_action_json(
        r#"{"action":"read_file","path":"src/lib.rs","start":1,"end":120}"#,
    )
    .expect("read_file should parse");

    assert_eq!(
        action,
        Action::ReadFile {
            path: RepoPath::new("src/lib.rs"),
            range: Some(LineRange { start: 1, end: 120 }),
        }
    );
}

#[test]
fn parses_read_file_action_without_range_json() {
    let action = parse_action_json(r#"{"action":"read_file","path":"README.md"}"#)
        .expect("read_file without range should parse");

    assert_eq!(
        action,
        Action::ReadFile {
            path: RepoPath::new("README.md"),
            range: None,
        }
    );
}

#[test]
fn parses_search_text_action_json() {
    let action = parse_action_json(
        r#"{"action":"search_text","pattern":"parse_user","root":"src"}"#,
    )
    .expect("search_text should parse");

    assert_eq!(
        action,
        Action::SearchText(SearchQuery {
            pattern: "parse_user".to_owned(),
            root: Some(RepoPath::new("src")),
        })
    );
}

#[test]
fn parses_list_files_action_json() {
    let action = parse_action_json(r#"{"action":"list_files","root":"src"}"#)
        .expect("list_files should parse");

    assert_eq!(
        action,
        Action::ListFiles(FileQuery {
            root: Some(RepoPath::new("src")),
        })
    );
}

#[test]
fn parses_apply_patch_action_json() {
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n";
    let content = serde_json::json!({
        "action": "apply_patch",
        "unified_diff": diff
    })
    .to_string();

    let action = parse_action_json(&content).expect("apply_patch should parse");

    assert_eq!(
        action,
        Action::ApplyPatch(Patch {
            unified_diff: diff.to_owned(),
        })
    );
}

#[test]
fn parses_run_actions_json() {
    assert_eq!(
        parse_action_json(r#"{"action":"run_verifier"}"#).expect("run_verifier should parse"),
        Action::RunVerifier
    );
    assert_eq!(
        parse_action_json(r#"{"action":"run_tests"}"#).expect("run_tests should parse"),
        Action::RunTests
    );
    assert_eq!(
        parse_action_json(r#"{"action":"run_typecheck"}"#).expect("run_typecheck should parse"),
        Action::RunTypecheck
    );
    assert_eq!(
        parse_action_json(r#"{"action":"run_benchmark"}"#).expect("run_benchmark should parse"),
        Action::RunBenchmark
    );
}

#[test]
fn reject_unknown_action_json() {
    let error = parse_action_json(r#"{"action":"unknown"}"#)
        .expect_err("unknown action should fail");

    assert!(error.to_string().contains("unsupported model action"));
}

#[test]
fn reject_absolute_or_parent_paths_if_applicable() {
    for content in [
        r#"{"action":"read_file","path":"/etc/passwd"}"#,
        r#"{"action":"read_file","path":"../Cargo.toml"}"#,
        r#"{"action":"search_text","pattern":"x","root":"../src"}"#,
        r#"{"action":"list_files","root":"/tmp"}"#,
    ] {
        let error = parse_action_json(content).expect_err("unsafe path should fail");
        assert!(error.to_string().contains("relative"));
    }
}
```

Update the existing unsupported-action test so it no longer uses `run_tests`, because `run_tests` becomes supported:

```rust
#[test]
fn rejects_unsupported_action_json() {
    let error = parse_action_json(r#"{"action":"unknown"}"#)
        .expect_err("unsupported action should fail");

    assert!(error.to_string().contains("unsupported model action"));
}
```

- [ ] **Step 2: Run parser tests and verify they fail**

Run:

```bash
cargo test -p patchwright-model-openai parse_
```

Expected: failures for every new non-`finish` action because `parse_action_json` only supports `finish`.

- [ ] **Step 3: Implement safe path parsing and action parsing**

Modify `crates/patchwright-model-openai/src/lib.rs` imports:

```rust
use patchwright_core::types::{
    FileQuery, LineRange, Patch, RepoPath, SearchQuery,
};
use std::path::{Component, Path, PathBuf};
```

Add helpers near `parse_action_json`:

```rust
fn required_string<'a>(value: &'a Value, field: &str, action: &str) -> Result<&'a str> {
    value.get(field).and_then(Value::as_str).ok_or_else(|| {
        PatchwrightError::Model(format!(
            "{action} action JSON missing string field '{field}'"
        ))
    })
}

fn optional_repo_path(value: &Value, field: &str, action: &str) -> Result<Option<RepoPath>> {
    value
        .get(field)
        .map(|path| {
            let path = path.as_str().ok_or_else(|| {
                PatchwrightError::Model(format!(
                    "{action} action JSON field '{field}' must be a string"
                ))
            })?;
            parse_repo_path(path)
        })
        .transpose()
}

fn required_repo_path(value: &Value, field: &str, action: &str) -> Result<RepoPath> {
    parse_repo_path(required_string(value, field, action)?)
}

fn parse_repo_path(path: &str) -> Result<RepoPath> {
    let path = Path::new(path);

    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(PatchwrightError::Model(format!(
            "model path must be a non-empty relative path: {}",
            path.display()
        )));
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(PatchwrightError::Model(format!(
                    "model path must be relative and stay inside the repository: {}",
                    path.display()
                )));
            }
        }
    }

    Ok(RepoPath::new(
        normalized.to_string_lossy().replace('\\', "/"),
    ))
}

fn optional_range(value: &Value, action: &str) -> Result<Option<LineRange>> {
    let start = value.get("start").and_then(Value::as_u64);
    let end = value.get("end").and_then(Value::as_u64);

    match (start, end) {
        (None, None) => Ok(None),
        (Some(start), Some(end)) if start > 0 && end >= start => Ok(Some(LineRange {
            start: usize::try_from(start).map_err(|_| {
                PatchwrightError::Model(format!("{action} action start is too large"))
            })?,
            end: usize::try_from(end).map_err(|_| {
                PatchwrightError::Model(format!("{action} action end is too large"))
            })?,
        })),
        _ => Err(PatchwrightError::Model(format!(
            "{action} action range requires positive start and end >= start"
        ))),
    }
}
```

Replace the `match action` body in `parse_action_json` with:

```rust
match action {
    "read_file" => Ok(Action::ReadFile {
        path: required_repo_path(&value, "path", action)?,
        range: optional_range(&value, action)?,
    }),
    "search_text" => Ok(Action::SearchText(SearchQuery {
        pattern: required_string(&value, "pattern", action)?.to_owned(),
        root: optional_repo_path(&value, "root", action)?,
    })),
    "list_files" => Ok(Action::ListFiles(FileQuery {
        root: optional_repo_path(&value, "root", action)?,
    })),
    "apply_patch" => Ok(Action::ApplyPatch(Patch {
        unified_diff: required_string(&value, "unified_diff", action)?.to_owned(),
    })),
    "run_verifier" => Ok(Action::RunVerifier),
    "run_tests" => Ok(Action::RunTests),
    "run_typecheck" => Ok(Action::RunTypecheck),
    "run_benchmark" => Ok(Action::RunBenchmark),
    "finish" => Ok(Action::Finish {
        summary: required_string(&value, "summary", action)?.to_owned(),
    }),
    unsupported => Err(PatchwrightError::Model(format!(
        "unsupported model action '{unsupported}'"
    ))),
}
```

Do not parse `revert_attempt` from model JSON in v0.1. `Action::RevertAttempt` remains a controller/internal action until snapshot IDs are intentionally exposed to the model.

- [ ] **Step 4: Verify parser support**

Run:

```bash
cargo test -p patchwright-model-openai
```

Expected: all `patchwright-model-openai` tests pass.

- [ ] **Step 5: Commit**

Run:

```bash
git add crates/patchwright-model-openai
git commit -m "feat(model-openai): parse structured agent actions"
```

## Task 2: Build a Real Agent Prompt

**Commit:** `feat(model-openai): build real agent prompt`

**Files:**
- Modify: `crates/patchwright-model-openai/src/lib.rs`
- Modify: `crates/patchwright-model-openai/tests/openai_client.rs`

- [ ] **Step 1: Add prompt tests first**

Append tests to `crates/patchwright-model-openai/tests/openai_client.rs`:

```rust
use patchwright_core::action::Observation;
use patchwright_core::types::{Counterexample, FileSlice};

#[test]
fn build_prompt_includes_action_contract_and_state() {
    let request = ModelRequest {
        task: TaskSpec::from_text(PathBuf::from("."), "Fix the add test"),
        observations: vec![Observation::FileRead(FileSlice {
            path: RepoPath::new("src/lib.rs"),
            start_line: 1,
            content: "pub fn add(a: i32, b: i32) -> i32 { a - b }\n".to_owned(),
        })],
        counterexamples: vec![Counterexample {
            source: "cargo".to_owned(),
            detail: "assertion failed: left == right".to_owned(),
        }],
    };

    let body = patchwright_model_openai::build_prompt(&request, "test-model");

    assert_eq!(body["model"], "test-model");
    let messages = body["messages"].as_array().expect("messages array");
    assert_eq!(messages[0]["role"], "system");
    assert_eq!(messages[1]["role"], "user");

    let system = messages[0]["content"].as_str().expect("system content");
    assert!(system.contains("Return only one JSON action"));
    assert!(system.contains("\"action\":\"read_file\""));
    assert!(system.contains("\"action\":\"apply_patch\""));
    assert!(system.contains("Verification decides success"));

    let user = messages[1]["content"].as_str().expect("user content");
    assert!(user.contains("Task:"));
    assert!(user.contains("Fix the add test"));
    assert!(user.contains("src/lib.rs"));
    assert!(user.contains("a - b"));
    assert!(user.contains("assertion failed"));
}
```

- [ ] **Step 2: Run prompt tests and verify they fail**

Run:

```bash
cargo test -p patchwright-model-openai build_prompt
```

Expected: compile failure because `build_prompt` does not exist.

- [ ] **Step 3: Implement prompt builder**

Add this public function to `crates/patchwright-model-openai/src/lib.rs`:

```rust
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
```

Add helper functions:

```rust
fn system_prompt() -> &'static str {
    r#"You are Patchwright. Return only one JSON action.
Never claim success. Verification decides success.
Prefer reading/searching before patching.
Use apply_patch only with a unified diff.
Use finish only when no code change is needed.

Allowed actions:
{"action":"read_file","path":"src/lib.rs","start":1,"end":120}
{"action":"search_text","pattern":"parse_user","root":"src"}
{"action":"list_files","root":"src"}
{"action":"apply_patch","unified_diff":"diff --git a/src/lib.rs b/src/lib.rs\n..."}
{"action":"run_verifier"}
{"action":"run_tests"}
{"action":"run_typecheck"}
{"action":"run_benchmark"}
{"action":"finish","summary":"No code change needed because ..."}

Rules:
- Do not modify .git, .env, secrets, or generated files.
- Do not modify lockfiles unless the task requires dependency resolution.
- Keep diffs minimal.
- After applying a patch, verification will run automatically.
- If verification fails, use the counterexamples in the next action."#
}

fn user_prompt(request: &ModelRequest) -> String {
    format!(
        "Task:\n{}\n\nObservations:\n{}\n\nCounterexamples:\n{}",
        request.task.text,
        format_observations(&request.observations),
        format_counterexamples(&request.counterexamples),
    )
}

fn format_observations(observations: &[patchwright_core::action::Observation]) -> String {
    if observations.is_empty() {
        return "none".to_owned();
    }

    observations
        .iter()
        .map(|observation| format!("{observation:?}"))
        .collect::<Vec<_>>()
        .join("\n---\n")
}

fn format_counterexamples(counterexamples: &[patchwright_core::types::Counterexample]) -> String {
    if counterexamples.is_empty() {
        return "none".to_owned();
    }

    counterexamples
        .iter()
        .map(|counterexample| {
            format!("{}: {}", counterexample.source, counterexample.detail)
        })
        .collect::<Vec<_>>()
        .join("\n")
}
```

Replace the hardcoded `body = json!({ ... })` in `propose_http_action` with:

```rust
let body = build_prompt(&request, &self.config.model);
```

- [ ] **Step 4: Verify model prompt contract**

Run:

```bash
cargo test -p patchwright-model-openai
```

Expected: model tests pass, including the existing local HTTP contract test. The test server should still observe `POST /chat/completions`, authorization, model, system message, and user message.

- [ ] **Step 5: Commit**

Run:

```bash
git add crates/patchwright-model-openai
git commit -m "feat(model-openai): build real agent prompt"
```

## Task 3: Wire Real Model Config for Solve

**Commit:** `feat(cli): wire real model config for solve`

**Files:**
- Modify: `crates/patchwright-cli/src/main.rs`

- [ ] **Step 1: Add CLI parsing tests first**

Append tests to `crates/patchwright-cli/src/main.rs` test module:

```rust
#[test]
fn solve_dry_run_accepts_model_flags_without_network() {
    let repo = TempRepo::new("cli-solve-dry-run-flags");
    repo.write(
        "Cargo.toml",
        "[package]\nname = \"cli_solve_dry_run_flags\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    repo.write("src/lib.rs", "pub fn answer() -> u8 { 42 }\n");
    repo.commit_all("seed valid rust crate");

    let result = run([
        "solve".to_owned(),
        "--repo".to_owned(),
        repo.root().to_string_lossy().into_owned(),
        "--task".to_owned(),
        "summarize".to_owned(),
        "--dry-run".to_owned(),
        "--model".to_owned(),
        "dry-test".to_owned(),
        "--base-url".to_owned(),
        "http://127.0.0.1:9".to_owned(),
        "--max-steps".to_owned(),
        "2".to_owned(),
        "--api-key-env".to_owned(),
        "PATCHWRIGHT_TEST_KEY".to_owned(),
    ]);

    assert!(result.is_ok());
}

#[test]
fn solve_real_mode_requires_model_name() {
    let repo = TempRepo::new("cli-solve-real-requires-model");
    repo.write(
        "Cargo.toml",
        "[package]\nname = \"cli_solve_real_requires_model\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    repo.write("src/lib.rs", "pub fn answer() -> u8 { 42 }\n");
    repo.commit_all("seed valid rust crate");

    let result = run([
        "solve".to_owned(),
        "--repo".to_owned(),
        repo.root().to_string_lossy().into_owned(),
        "--task".to_owned(),
        "fix something".to_owned(),
    ]);

    assert_eq!(
        result,
        Err("solve real mode requires --model <name> or --dry-run".to_owned())
    );
}

#[test]
fn solve_rejects_invalid_max_steps() {
    let result = run([
        "solve".to_owned(),
        "--repo".to_owned(),
        ".",
        "--task".to_owned(),
        "summarize".to_owned(),
        "--dry-run".to_owned(),
        "--max-steps".to_owned(),
        "0".to_owned(),
    ]);

    assert_eq!(
        result,
        Err("solve requires --max-steps to be a positive integer".to_owned())
    );
}
```

- [ ] **Step 2: Run CLI tests and verify they fail**

Run:

```bash
cargo test -p patchwright-cli solve_
```

Expected: failures because `--dry-run`, `--model`, `--base-url`, `--max-steps`, and `--api-key-env` are not parsed yet.

- [ ] **Step 3: Add `SolveOptions` parser**

Add this struct and parser near `run_solve`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
struct SolveOptions {
    repo: PathBuf,
    task: String,
    dry_run: bool,
    model: Option<String>,
    base_url: String,
    api_key_env: String,
    max_steps: usize,
    require_patch: bool,
}

fn solve_options(args: &[String]) -> Result<SolveOptions, String> {
    let Some(repo) = value_after(args, "--repo") else {
        return Err("solve requires --repo <path> and --task <text>".to_owned());
    };
    let Some(task) = value_after(args, "--task") else {
        return Err("solve requires --repo <path> and --task <text>".to_owned());
    };

    let dry_run = has_flag(args, "--dry-run");
    let info_only = has_flag(args, "--info-only");
    let model = value_after(args, "--model");
    if !dry_run && model.is_none() {
        return Err("solve real mode requires --model <name> or --dry-run".to_owned());
    }

    let max_steps = value_after(args, "--max-steps")
        .unwrap_or_else(|| "30".to_owned())
        .parse::<usize>()
        .ok()
        .filter(|steps| *steps > 0)
        .ok_or_else(|| "solve requires --max-steps to be a positive integer".to_owned())?;

    Ok(SolveOptions {
        repo: accessible_repo_path(&repo)?,
        task,
        dry_run,
        model,
        base_url: value_after(args, "--base-url")
            .unwrap_or_else(|| "https://api.openai.com/v1".to_owned()),
        api_key_env: value_after(args, "--api-key-env")
            .unwrap_or_else(|| "OPENAI_API_KEY".to_owned()),
        max_steps,
        require_patch: !info_only,
    })
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}
```

Keep `value_after` rejecting flag-like values.

- [ ] **Step 4: Use real or dry-run model in `run_solve`**

Replace the start of `run_solve` with:

```rust
fn run_solve(args: &[String]) -> Result<(), String> {
    let options = solve_options(args)?;

    let sandbox = GitWorktreeSandbox::create(&options.repo).map_err(|error| error.to_string())?;
    let sandbox_repo = sandbox.root().to_path_buf();

    let config = OpenAiConfig {
        base_url: options.base_url,
        model: options.model.unwrap_or_else(|| "dry-run".to_owned()),
        api_key_env: options.api_key_env,
        timeout_seconds: 30,
    };
    let model = if options.dry_run {
        OpenAiCompatibleClient::dry_run(config)
    } else {
        OpenAiCompatibleClient::new(config)
    };
```

Update `.max_steps(3)` to:

```rust
.max_steps(options.max_steps)
```

Task 4 will update `TaskSpec` construction for `require_patch`.

- [ ] **Step 5: Update help text**

Update `print_help()` solve line:

```text
patchwright solve --repo <path> --task <text> [--dry-run] [--model <name>] [--base-url <url>] [--api-key-env <env>] [--max-steps <n>] [--info-only]
```

Do not add raw API key support.

- [ ] **Step 6: Verify CLI solve config**

Run:

```bash
cargo test -p patchwright-cli
cargo run -p patchwright-cli -- solve --repo . --task "summarize" --dry-run --info-only
```

Expected: tests pass; dry-run exits 0 without network. Real model mode is not exercised in normal tests because network tests must remain opt-in/local.

- [ ] **Step 7: Commit**

Run:

```bash
git add crates/patchwright-cli
git commit -m "feat(cli): wire real model config for solve"
```

## Task 4: Add a Finish Gate for Coding Tasks

**Commit:** `feat(core): reject unverified finish for code-change tasks`

**Files:**
- Modify: `crates/patchwright-core/src/types.rs`
- Modify: `crates/patchwright-core/src/agent.rs`
- Modify: `crates/patchwright-core/tests/agent_loop.rs`
- Modify: `crates/patchwright-cli/src/main.rs`

- [ ] **Step 1: Add core tests first**

Append tests to `crates/patchwright-core/tests/agent_loop.rs`:

```rust
#[test]
fn code_change_task_rejects_immediate_finish() {
    let model = ScriptedModel {
        actions: vec![Action::Finish {
            summary: "looks fixed".to_owned(),
        }],
    };

    let mut agent = Agent::builder()
        .model(model)
        .execution(FakeExecution::default())
        .language_adapter(FakeLanguage)
        .indexer(EmptyIndex)
        .verifier(AcceptingVerifier)
        .policy(Policy::SafeStructuredOnly)
        .max_steps(3)
        .try_build()
        .expect("agent should build");

    let report = agent
        .solve(TaskSpec::code_change(PathBuf::from("."), "fix the bug"))
        .expect("simulation should return a report");

    assert_eq!(report.status, SolveStatus::BudgetExhausted);
    assert!(report.summary.contains("patch required"));
    assert!(matches!(
        report.observations.last(),
        Some(Observation::Error(message)) if message.contains("patch required")
    ));
}

#[test]
fn info_only_task_can_finish_without_patch() {
    let model = ScriptedModel {
        actions: vec![Action::Finish {
            summary: "summary only".to_owned(),
        }],
    };

    let mut agent = Agent::builder()
        .model(model)
        .execution(FakeExecution::default())
        .language_adapter(FakeLanguage)
        .indexer(EmptyIndex)
        .verifier(AcceptingVerifier)
        .policy(Policy::SafeStructuredOnly)
        .max_steps(3)
        .try_build()
        .expect("agent should build");

    let report = agent
        .solve(TaskSpec::from_text(PathBuf::from("."), "summarize"))
        .expect("simulation should finish");

    assert_eq!(report.status, SolveStatus::Finished);
    assert_eq!(report.summary, "summary only");
}
```

- [ ] **Step 2: Run core tests and verify they fail**

Run:

```bash
cargo test -p patchwright-core code_change_task_rejects_immediate_finish
```

Expected: compile failure because `TaskSpec::code_change` does not exist.

- [ ] **Step 3: Add `require_patch` to `TaskSpec`**

Modify `crates/patchwright-core/src/types.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSpec {
    pub text: String,
    pub repo_path: PathBuf,
    pub require_patch: bool,
}

impl TaskSpec {
    pub fn from_text(repo_path: PathBuf, text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            repo_path,
            require_patch: false,
        }
    }

    pub fn code_change(repo_path: PathBuf, text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            repo_path,
            require_patch: true,
        }
    }

    pub fn with_require_patch(mut self, require_patch: bool) -> Self {
        self.require_patch = require_patch;
        self
    }
}
```

- [ ] **Step 4: Enforce finish gate in `Agent::solve`**

In `Action::Finish { summary }` branch in `crates/patchwright-core/src/agent.rs`, replace the unconditional finish with:

```rust
if task.require_patch {
    let message = "patch required but model returned finish before an accepted patch".to_owned();
    observations.push(Observation::Error(message.clone()));
    return Ok(SolveReport {
        status: SolveStatus::BudgetExhausted,
        summary: message,
        attempts,
        observations,
        counterexamples,
    });
}

observations.push(Observation::Finished(summary.clone()));
return Ok(SolveReport {
    status: SolveStatus::Finished,
    summary,
    attempts,
    observations,
    counterexamples,
});
```

- [ ] **Step 5: Wire CLI task kind**

In `run_solve`, create the task with:

```rust
let task = TaskSpec::from_text(sandbox_repo, options.task)
    .with_require_patch(options.require_patch);
```

Pass that task to `agent.solve(task)`.

With Task 3's parser, `--info-only` sets `require_patch = false`; all other `solve` calls require an accepted patch by default.

- [ ] **Step 6: Verify finish gate**

Run:

```bash
cargo test -p patchwright-core
cargo test -p patchwright-cli
cargo run -p patchwright-cli -- solve --repo . --task "summarize" --dry-run --info-only
```

Expected: core and CLI tests pass; dry-run info-only can finish. A dry-run solve without `--info-only` exits 0 but prints `BudgetExhausted` and a patch-required summary.

- [ ] **Step 7: Commit**

Run:

```bash
git add crates/patchwright-core crates/patchwright-cli
git commit -m "feat(core): reject unverified finish for code-change tasks"
```

## Task 5: Add End-to-End Verified Patch Fixture

**Commit:** `test(agent): add end-to-end verified patch fixture`

**Files:**
- Create: `crates/patchwright-cli/tests/verified_patch_fixture.rs`

- [ ] **Step 1: Write end-to-end fixture test**

Create `crates/patchwright-cli/tests/verified_patch_fixture.rs`:

```rust
use patchwright_core::action::Action;
use patchwright_core::agent::{Agent, SolveStatus};
use patchwright_core::error::{PatchwrightError, Result};
use patchwright_core::policy::Policy;
use patchwright_core::traits::ModelProvider;
use patchwright_core::types::{ModelRequest, ModelResponse, Patch, TaskSpec, VerificationStatus};
use patchwright_exec_local::{GitWorktreeSandbox, LocalExecution};
use patchwright_index::BasicIndexer;
use patchwright_lang_rust::RustAdapter;
use patchwright_test_support::TempRepo;
use patchwright_verify::PlanVerifier;
use std::fs;
use std::process::Command;

struct ScriptedPatchModel {
    actions: Vec<Action>,
}

impl ModelProvider for ScriptedPatchModel {
    fn propose_action(&mut self, _request: ModelRequest) -> Result<ModelResponse> {
        if self.actions.is_empty() {
            return Err(PatchwrightError::Model("script exhausted".to_owned()));
        }

        Ok(ModelResponse {
            action: self.actions.remove(0),
        })
    }
}

fn broken_add_repo(name: &str) -> TempRepo {
    let repo = TempRepo::new(name);
    repo.write(
        "Cargo.toml",
        "[package]\nname = \"broken_add\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    repo.write(
        "src/lib.rs",
        "pub fn add(a: i32, b: i32) -> i32 {\n    a - b\n}\n\n#[cfg(test)]\nmod tests {\n    use super::add;\n\n    #[test]\n    fn adds_numbers() {\n        assert_eq!(add(2, 3), 5);\n    }\n}\n",
    );
    repo.commit_all("seed broken add repo");
    repo
}

#[test]
fn scripted_model_applies_verified_patch_inside_sandbox() -> Result<()> {
    let repo = broken_add_repo("agent-e2e-verified-patch");
    let original = fs::read_to_string(repo.root().join("src/lib.rs"))?;
    let sandbox = GitWorktreeSandbox::create(repo.root())?;
    let sandbox_root = sandbox.root().to_path_buf();

    let patch = Patch {
        unified_diff: "diff --git a/src/lib.rs b/src/lib.rs\nindex b6f47d2..7a70a66 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,5 +1,5 @@\n pub fn add(a: i32, b: i32) -> i32 {\n-    a - b\n+    a + b\n }\n \n #[cfg(test)]\n".to_owned(),
    };
    let model = ScriptedPatchModel {
        actions: vec![Action::ApplyPatch(patch)],
    };

    let mut agent = Agent::builder()
        .model(model)
        .execution(LocalExecution::new(&sandbox_root))
        .language_adapter(RustAdapter::default())
        .indexer(BasicIndexer::new(&sandbox_root))
        .verifier(PlanVerifier)
        .policy(Policy::ProjectConfiguredCommands {
            allowed_programs: vec!["cargo".to_owned()],
        })
        .max_steps(5)
        .try_build()?;

    let report = agent.solve(TaskSpec::code_change(
        sandbox_root.clone(),
        "Fix the failing add test with the smallest correct patch.",
    ))?;

    assert_eq!(report.status, SolveStatus::Accepted);
    assert_eq!(report.attempts.len(), 1);
    assert_eq!(
        report.attempts[0].verification.status,
        VerificationStatus::Accepted
    );
    assert_eq!(fs::read_to_string(repo.root().join("src/lib.rs"))?, original);
    assert!(fs::read_to_string(sandbox_root.join("src/lib.rs"))?.contains("a + b"));

    let output = Command::new("cargo")
        .arg("test")
        .current_dir(&sandbox_root)
        .output()?;
    assert!(
        output.status.success(),
        "cargo test failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(())
}
```

- [ ] **Step 2: Run the fixture and verify it fails if implementation is incomplete**

Run:

```bash
cargo test -p patchwright-cli --test verified_patch_fixture
```

Expected before the final implementation is complete: failures may be compile errors from missing `TaskSpec::code_change`. After Task 4, the fixture should pass.

- [ ] **Step 3: Add rejected patch fixture**

Append a second test to `verified_patch_fixture.rs`:

```rust
#[test]
fn rejected_patch_is_reverted_inside_sandbox() -> Result<()> {
    let repo = broken_add_repo("agent-e2e-rejected-patch");
    let sandbox = GitWorktreeSandbox::create(repo.root())?;
    let sandbox_root = sandbox.root().to_path_buf();

    let patch = Patch {
        unified_diff: "diff --git a/src/lib.rs b/src/lib.rs\nindex b6f47d2..0b9e5bd 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,5 +1,5 @@\n pub fn add(a: i32, b: i32) -> i32 {\n-    a - b\n+    0\n }\n \n #[cfg(test)]\n".to_owned(),
    };
    let model = ScriptedPatchModel {
        actions: vec![patchwright_core::action::Action::ApplyPatch(patch)],
    };

    let mut agent = Agent::builder()
        .model(model)
        .execution(LocalExecution::new(&sandbox_root))
        .language_adapter(RustAdapter::default())
        .indexer(BasicIndexer::new(&sandbox_root))
        .verifier(PlanVerifier)
        .policy(Policy::ProjectConfiguredCommands {
            allowed_programs: vec!["cargo".to_owned()],
        })
        .max_steps(1)
        .try_build()?;

    let report = agent.solve(TaskSpec::code_change(
        sandbox_root.clone(),
        "Fix the failing add test with the smallest correct patch.",
    ))?;

    assert_eq!(report.status, SolveStatus::BudgetExhausted);
    assert_eq!(report.attempts.len(), 1);
    assert_eq!(
        report.attempts[0].verification.status,
        VerificationStatus::Rejected
    );
    assert!(fs::read_to_string(sandbox_root.join("src/lib.rs"))?.contains("a - b"));

    Ok(())
}
```

- [ ] **Step 4: Verify end-to-end agent fixture**

Run:

```bash
cargo test -p patchwright-cli --test verified_patch_fixture
cargo test --workspace
```

Expected: the new fixture passes; the workspace remains green.

- [ ] **Step 5: Commit**

Run:

```bash
git add crates/patchwright-cli/tests/verified_patch_fixture.rs
git commit -m "test(agent): add end-to-end verified patch fixture"
```

## Task 6: Add Ranked Context Pack

**Commit:** `feat(index): add ranked context pack`

**Files:**
- Modify: `crates/patchwright-core/src/types.rs`
- Modify: `crates/patchwright-core/src/traits.rs`
- Modify: `crates/patchwright-core/src/agent.rs`
- Modify: `crates/patchwright-core/tests/agent_loop.rs`
- Modify: `crates/patchwright-index/src/lib.rs`
- Modify: `crates/patchwright-index/tests/basic_index.rs`
- Modify: `crates/patchwright-model-openai/src/lib.rs`
- Modify: `crates/patchwright-model-openai/tests/openai_client.rs`

- [ ] **Step 1: Add core context type**

Add to `crates/patchwright-core/src/types.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContextPack {
    pub files: Vec<ScoredPath>,
    pub likely_tests: Vec<RepoPath>,
    pub manifests: Vec<RepoPath>,
    pub recent_observations: Vec<Observation>,
    pub counterexamples: Vec<Counterexample>,
}
```

Update `ModelRequest`:

```rust
pub struct ModelRequest {
    pub task: TaskSpec,
    pub observations: Vec<Observation>,
    pub counterexamples: Vec<Counterexample>,
    pub context: Option<ContextPack>,
}
```

Then update every `ModelRequest` construction in tests and code with `context: None`.

Update `crates/patchwright-core/src/traits.rs` so the agent can request context through the language-agnostic indexer boundary:

```rust
fn context_pack(
    &self,
    task: &TaskSpec,
    observations: &[Observation],
    counterexamples: &[Counterexample],
) -> Result<ContextPack> {
    let mut files = self.list_files(FileQuery::default())?;
    files.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.path.0.cmp(&right.path.0))
    });

    Ok(ContextPack {
        files: files.into_iter().take(20).collect(),
        recent_observations: observations.iter().rev().take(8).cloned().collect(),
        counterexamples: counterexamples.to_vec(),
        ..ContextPack::default()
    })
}
```

Add the needed imports in `traits.rs`:

```rust
use crate::action::Observation;
use crate::types::{ContextPack, Counterexample};
```

Update `Agent::solve` model request construction:

```rust
let context = self
    .indexer
    .context_pack(&task, &observations, &counterexamples)?;
let response = self.model.propose_action(ModelRequest {
    task: task.clone(),
    observations: observations.clone(),
    counterexamples: counterexamples.clone(),
    context: Some(context),
})?;
```

- [ ] **Step 2: Add index tests first**

Append to `crates/patchwright-index/tests/basic_index.rs`:

```rust
#[test]
fn context_pack_prioritizes_rust_sources_tests_and_manifests() -> Result<()> {
    let repo = TempRepo::new("index-context-pack");
    repo.write("Cargo.toml", "[package]\nname = \"sample\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");
    repo.write("src/lib.rs", "pub fn parse_user() {}\n");
    repo.write("tests/parser_test.rs", "#[test]\nfn parse_user_handles_empty() {}\n");
    repo.write("README.md", "sample\n");
    repo.commit_all("seed");

    let indexer = BasicIndexer::new(repo.root());
    let pack = indexer.context_pack(
        &TaskSpec::from_text(repo.root().to_path_buf(), "fix parse_user test"),
        &[],
        &[],
    )?;

    assert_eq!(pack.manifests, vec![RepoPath::new("Cargo.toml")]);
    assert_eq!(pack.likely_tests, vec![RepoPath::new("tests/parser_test.rs")]);
    assert_eq!(pack.files[0].path, RepoPath::new("tests/parser_test.rs"));
    assert!(pack.files.iter().any(|file| file.path == RepoPath::new("src/lib.rs")));

    Ok(())
}
```

- [ ] **Step 3: Run index context test and verify it fails**

Run:

```bash
cargo test -p patchwright-index context_pack
```

Expected: compile failure because `BasicIndexer::context_pack` does not exist.

- [ ] **Step 4: Implement cheap context pack**

Override `context_pack` in the `impl Indexer for BasicIndexer` block in `crates/patchwright-index/src/lib.rs`:

```rust
fn context_pack(
    &self,
    task: &patchwright_core::types::TaskSpec,
    observations: &[patchwright_core::action::Observation],
    counterexamples: &[patchwright_core::types::Counterexample],
) -> Result<patchwright_core::types::ContextPack> {
    let mut files = self.list_files(FileQuery::default())?;
    let task_words = task_words(&task.text);

    for file in &mut files {
        if file.path.0 == "Cargo.toml" || file.path.0.ends_with("/Cargo.toml") {
            file.score += 50;
        }
        if file.path.0.ends_with(".rs") {
            file.score += 20;
        }
        if file.path.0.starts_with("tests/") || file.path.0.contains("_test") {
            file.score += 30;
        }
        for word in &task_words {
            if file.path.0.to_lowercase().contains(word) {
                file.score += 10;
            }
        }
        for counterexample in counterexamples {
            if counterexample.detail.contains(&file.path.0) {
                file.score += 25;
            }
        }
    }

    files.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.path.0.cmp(&right.path.0))
    });

    let likely_tests = files
        .iter()
        .filter(|file| file.path.0.starts_with("tests/") || file.path.0.contains("_test"))
        .map(|file| file.path.clone())
        .collect();
    let manifests = files
        .iter()
        .filter(|file| file.path.0 == "Cargo.toml" || file.path.0.ends_with("/Cargo.toml"))
        .map(|file| file.path.clone())
        .collect();

    Ok(patchwright_core::types::ContextPack {
        files: files.into_iter().take(20).collect(),
        likely_tests,
        manifests,
        recent_observations: observations.iter().rev().take(8).cloned().collect(),
        counterexamples: counterexamples.to_vec(),
    })
}
```

Add helper:

```rust
fn task_words(task: &str) -> Vec<String> {
    task.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .map(str::to_lowercase)
        .filter(|word| word.len() >= 3)
        .collect()
}
```

- [ ] **Step 5: Include context in prompt when present**

Update `user_prompt` in `patchwright-model-openai`:

```rust
let context = request
    .context
    .as_ref()
    .map(format_context_pack)
    .unwrap_or_else(|| "none".to_owned());

format!(
    "Task:\n{}\n\nContext:\n{}\n\nObservations:\n{}\n\nCounterexamples:\n{}",
    request.task.text,
    context,
    format_observations(&request.observations),
    format_counterexamples(&request.counterexamples),
)
```

Add:

```rust
fn format_context_pack(pack: &patchwright_core::types::ContextPack) -> String {
    let files = pack
        .files
        .iter()
        .map(|file| format!("{} score={}", file.path.0, file.score))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "files:\n{}\nlikely_tests: {:?}\nmanifests: {:?}",
        files, pack.likely_tests, pack.manifests
    )
}
```

Update prompt tests to set `context: None`, then add one test that passes a `ContextPack` and asserts the prompt contains `Cargo.toml`, `src/lib.rs`, and `tests/parser_test.rs`.

- [ ] **Step 6: Verify context pack**

Run:

```bash
cargo test -p patchwright-index
cargo test -p patchwright-model-openai
cargo test --workspace
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

Run:

```bash
git add crates/patchwright-core crates/patchwright-index crates/patchwright-model-openai
git commit -m "feat(index): add ranked context pack"
```

## Task 7: Add `patchwright.toml`

**Commit:** `feat(config): load patchwright.toml`

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/patchwright-config/Cargo.toml`
- Create: `crates/patchwright-config/src/lib.rs`
- Create: `crates/patchwright-config/tests/config.rs`
- Modify: `crates/patchwright-cli/Cargo.toml`
- Modify: `crates/patchwright-cli/src/main.rs`
- Modify: `README.md`

- [ ] **Step 1: Add config crate to workspace**

Modify root `Cargo.toml` members:

```toml
"crates/patchwright-config",
```

Create `crates/patchwright-config/Cargo.toml`:

```toml
[package]
name = "patchwright-config"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true

[lib]
path = "src/lib.rs"

[dependencies]
patchwright-core = { path = "../patchwright-core" }
serde = { version = "1", features = ["derive"] }
toml = "0.8"

[lints]
workspace = true
```

- [ ] **Step 2: Write config tests first**

Create `crates/patchwright-config/tests/config.rs`:

```rust
use patchwright_config::PatchwrightConfig;

#[test]
fn default_config_matches_v0_1_safe_defaults() {
    let config = PatchwrightConfig::default();

    assert_eq!(config.model.base_url, "https://api.openai.com/v1");
    assert_eq!(config.model.api_key_env, "OPENAI_API_KEY");
    assert_eq!(config.agent.max_steps, 30);
    assert!(config.agent.require_patch);
    assert_eq!(config.agent.max_changed_files, 5);
    assert_eq!(config.agent.max_inserted_lines, 300);
    assert_eq!(config.policy.allowed_programs, vec!["cargo"]);
    assert!(config.rust.fmt);
    assert!(config.rust.check);
    assert!(config.rust.test);
    assert!(!config.rust.clippy);
}

#[test]
fn parses_patchwright_toml() {
    let config = PatchwrightConfig::from_toml_str(
        r#"
[model]
base_url = "http://127.0.0.1:8080/v1"
model = "local-code-model"
api_key_env = "LOCAL_API_KEY"

[agent]
max_steps = 12
require_patch = false
max_changed_files = 3
max_inserted_lines = 100

[policy]
allowed_programs = ["cargo", "git"]

[rust]
fmt = true
check = true
test = false
clippy = true
"#,
    )
    .expect("config should parse");

    assert_eq!(config.model.base_url, "http://127.0.0.1:8080/v1");
    assert_eq!(config.model.model.as_deref(), Some("local-code-model"));
    assert_eq!(config.agent.max_steps, 12);
    assert!(!config.agent.require_patch);
    assert_eq!(config.policy.allowed_programs, vec!["cargo", "git"]);
    assert!(config.rust.clippy);
}

#[test]
fn rejects_zero_max_steps() {
    let error = PatchwrightConfig::from_toml_str("[agent]\nmax_steps = 0\n")
        .expect_err("zero max steps should fail");

    assert!(error.to_string().contains("max_steps"));
}
```

- [ ] **Step 3: Run config tests and verify they fail**

Run:

```bash
cargo test -p patchwright-config
```

Expected: crate does not compile yet.

- [ ] **Step 4: Implement config parsing**

Create `crates/patchwright-config/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

use patchwright_core::error::{PatchwrightError, Result};
use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct PatchwrightConfig {
    pub model: ModelConfig,
    pub agent: AgentConfig,
    pub policy: PolicyConfig,
    pub rust: RustConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct ModelConfig {
    pub base_url: String,
    pub model: Option<String>,
    pub api_key_env: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    pub max_steps: usize,
    pub require_patch: bool,
    pub max_changed_files: usize,
    pub max_inserted_lines: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct PolicyConfig {
    pub allowed_programs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct RustConfig {
    pub fmt: bool,
    pub check: bool,
    pub test: bool,
    pub clippy: bool,
}

impl Default for PatchwrightConfig {
    fn default() -> Self {
        Self {
            model: ModelConfig::default(),
            agent: AgentConfig::default(),
            policy: PolicyConfig::default(),
            rust: RustConfig::default(),
        }
    }
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".to_owned(),
            model: None,
            api_key_env: "OPENAI_API_KEY".to_owned(),
        }
    }
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

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            allowed_programs: vec!["cargo".to_owned()],
        }
    }
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

impl PatchwrightConfig {
    pub fn load(repo: &Path) -> Result<Self> {
        let path = repo.join("patchwright.toml");
        if !path.is_file() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path)?;
        Self::from_toml_str(&content)
    }

    pub fn from_toml_str(content: &str) -> Result<Self> {
        let config: Self = toml::from_str(content)
            .map_err(|error| PatchwrightError::InvalidInput(error.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        if self.agent.max_steps == 0 {
            return Err(PatchwrightError::InvalidInput(
                "agent.max_steps must be greater than zero".to_owned(),
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
```

- [ ] **Step 5: Wire config into CLI**

Add dependency in `crates/patchwright-cli/Cargo.toml`:

```toml
patchwright-config = { path = "../patchwright-config" }
```

In `crates/patchwright-cli/src/main.rs`, update `config check` branch:

```rust
"config" if args.get(1).map(String::as_str) == Some("check") => run_config_check(&args),
```

Add:

```rust
fn run_config_check(args: &[String]) -> Result<(), String> {
    let repo = match value_after(args, "--repo") {
        Some(path) => accessible_repo_path(&path)?,
        None => env::current_dir().map_err(|error| error.to_string())?,
    };
    let config = patchwright_config::PatchwrightConfig::load(&repo)
        .map_err(|error| error.to_string())?;

    println!("config: ok");
    println!("model base_url: {}", config.model.base_url);
    println!("agent max_steps: {}", config.agent.max_steps);
    println!("policy allowed_programs: {}", config.policy.allowed_programs.join(","));
    Ok(())
}
```

Update `solve_options` to load config before applying CLI overrides:

```rust
let repo = accessible_repo_path(&repo)?;
let config = patchwright_config::PatchwrightConfig::load(&repo)
    .map_err(|error| error.to_string())?;
```

Then set defaults from config:

```rust
base_url: value_after(args, "--base-url").unwrap_or(config.model.base_url),
api_key_env: value_after(args, "--api-key-env").unwrap_or(config.model.api_key_env),
model: value_after(args, "--model").or(config.model.model),
max_steps,
require_patch: if info_only { false } else { config.agent.require_patch },
```

Use `config.policy.allowed_programs` when constructing `Policy::ProjectConfiguredCommands`.

Update `run_verify` to load config and use configured policy. If `rust.clippy = true`, construct a Rust adapter with clippy enabled in the next small follow-up if the current adapter fields are private; for this task, keep default Rust commands and use configured allowed programs.

- [ ] **Step 6: Update README**

Add:

````md
## Project Config

Patchwright reads `patchwright.toml` from the target repo when present:

```toml
[model]
base_url = "https://api.openai.com/v1"
model = "gpt-5.5-pro"
api_key_env = "OPENAI_API_KEY"

[agent]
max_steps = 30
require_patch = true
max_changed_files = 5
max_inserted_lines = 300

[policy]
allowed_programs = ["cargo"]

[rust]
fmt = true
check = true
test = true
clippy = false
```

No API key is accepted as a raw CLI argument; use `api_key_env`.
````

- [ ] **Step 7: Verify config path**

Run:

```bash
cargo test -p patchwright-config
cargo test -p patchwright-cli
cargo run -p patchwright-cli -- config check --repo .
cargo run -p patchwright-cli -- verify --repo .
```

Expected: config tests pass; CLI config check prints `config: ok`; verification still runs Cargo checks in a temporary worktree.

- [ ] **Step 8: Commit**

Run:

```bash
git add Cargo.toml Cargo.lock README.md crates/patchwright-config crates/patchwright-cli
git commit -m "feat(config): load patchwright.toml"
```

## Final Verification

After all seven commits, run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p patchwright-cli -- --version
cargo run -p patchwright-cli -- status
cargo run -p patchwright-cli -- config check --repo .
cargo run -p patchwright-cli -- verify --repo .
cargo run -p patchwright-cli -- solve --repo . --task "summarize" --dry-run --info-only
cargo run -p patchwright-cli -- bench startup
git status -sb
```

Expected:

- Formatting, clippy, and workspace tests pass.
- `verify --repo .` runs `cargo fmt -- --check`, `cargo check`, and `cargo test`.
- `solve --dry-run --info-only` finishes without network.
- Startup benchmark still prints `startup_version_average_micros=<number>`.
- Worktree is clean.

## First Public Demo

Build the binary:

```bash
cargo build -p patchwright-cli
```

Create a tiny broken Rust repo:

```bash
cargo new broken-add --lib
cd broken-add
```

Replace `src/lib.rs`:

```rust
pub fn add(a: i32, b: i32) -> i32 {
    a - b
}

#[cfg(test)]
mod tests {
    use super::add;

    #[test]
    fn adds_numbers() {
        assert_eq!(add(2, 3), 5);
    }
}
```

Commit the broken fixture so Patchwright can create a clean sandbox:

```bash
git add .
git commit -m "test: add broken add fixture"
```

Run:

```bash
patchwright solve \
  --repo . \
  --task "Fix the failing add test with the smallest correct patch." \
  --model gpt-5.5-pro \
  --base-url https://api.openai.com/v1
```

Expected output shape:

```text
solve status: Accepted
changed files:
  src/lib.rs

verification:
  [ok] cargo fmt -- --check
  [ok] cargo check
  [ok] cargo test

summary:
  accepted patch
```

## Self-Review

- Spec coverage: all seven requested commits are represented as separate tasks with exact files, tests, verification commands, and commit messages.
- Scope check: the plan is focused on the v0.1 real-agent path. Docker, embeddings, UI, GitHub app, and non-Rust adapters are explicitly excluded.
- Placeholder scan: no unresolved markers or open-ended implementation placeholders remain.
- Type consistency: `TaskSpec.require_patch`, `ContextPack`, `ModelRequest.context`, and CLI `SolveOptions` are introduced before later tasks use them.
- Risk note: `RustAdapter` currently has private verifier toggles. Task 7 intentionally defers config-driven clippy wiring unless a small public constructor is added during implementation review.
