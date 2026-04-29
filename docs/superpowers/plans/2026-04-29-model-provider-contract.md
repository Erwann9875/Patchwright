# Model Provider Contract Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a shared model action contract and a Codex CLI provider so Patchwright defaults to ChatGPT/Codex login instead of OpenAI API-key auth.

**Architecture:** `patchwright-model-contract` owns prompts, schema, and action parsing. `patchwright-model-openai` and `patchwright-model-codex-cli` become thin transports implementing the existing `ModelProvider` trait.

**Tech Stack:** Rust workspace crates, `serde_json`, `toml`, Codex CLI `codex exec`.

---

## Task 1: Shared Model Contract

**Files:**
- Create: `crates/patchwright-model-contract/Cargo.toml`
- Create: `crates/patchwright-model-contract/src/lib.rs`
- Create: `crates/patchwright-model-contract/tests/contract.rs`
- Modify: `Cargo.toml`

- [ ] Add the crate to the workspace.
- [ ] Move the action parser from `patchwright-model-openai` into the contract crate.
- [ ] Move prompt rendering into the contract crate as `build_prompt`, `render_exec_prompt`, and `build_openai_prompt`.
- [ ] Add `action_output_schema()` for Codex `--output-schema`.
- [ ] Test parser coverage, prompt coverage, schema coverage, and unsafe path rejection.
- [ ] Commit: `feat(model): add shared action contract`.

## Task 2: Refactor OpenAI Provider

**Files:**
- Modify: `crates/patchwright-model-openai/Cargo.toml`
- Modify: `crates/patchwright-model-openai/src/lib.rs`
- Modify: `crates/patchwright-model-openai/tests/openai_client.rs`

- [ ] Depend on `patchwright-model-contract`.
- [ ] Re-export `build_prompt` and `parse_action_json` for existing tests and callers.
- [ ] Use `build_openai_prompt` for HTTP request body.
- [ ] Remove provider-local prompt/parser code.
- [ ] Run `cargo test -p patchwright-model-openai`.
- [ ] Commit: `refactor(model-openai): use shared action contract`.

## Task 3: Codex CLI Provider

**Files:**
- Create: `crates/patchwright-model-codex-cli/Cargo.toml`
- Create: `crates/patchwright-model-codex-cli/src/lib.rs`
- Create: `crates/patchwright-model-codex-cli/tests/codex_cli.rs`
- Modify: `Cargo.toml`

- [ ] Add `CodexCliConfig { command, model }`.
- [ ] Implement `CodexCliClient: ModelProvider`.
- [ ] Write schema and action output files under a unique temp directory.
- [ ] Invoke `codex exec --ephemeral --sandbox read-only --ask-for-approval never --skip-git-repo-check --output-schema <schema> --json -o <action> -`.
- [ ] Feed the shared exec prompt through stdin.
- [ ] Parse the action file with `patchwright-model-contract`.
- [ ] Test with a fake Codex executable that records args/stdin and writes a valid action.
- [ ] Commit: `feat(model-codex-cli): add Codex CLI provider`.

## Task 4: Config and CLI Selection

**Files:**
- Modify: `crates/patchwright-config/src/lib.rs`
- Modify: `crates/patchwright-config/tests/config.rs`
- Modify: `crates/patchwright-cli/Cargo.toml`
- Modify: `crates/patchwright-cli/src/main.rs`
- Modify: `README.md`

- [ ] Add `provider = "codex-cli"` default.
- [ ] Add `[model.codex_cli]` and `[model.openai]` nested provider configs.
- [ ] Keep existing flat OpenAI fields as compatibility fallbacks.
- [ ] Add `--model-provider`.
- [ ] Make `--base-url` or `--api-key-env` imply `openai-compatible` when no provider is explicit.
- [ ] Build a CLI model enum that wraps dry-run, OpenAI, and Codex providers.
- [ ] Test default Codex selection, OpenAI selection, flag overrides, and dry-run behavior.
- [ ] Commit: `feat(cli): select model provider from config`.

## Task 5: Final Verification

**Files:**
- All changed files.

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] Run `cargo test --workspace`.
- [ ] Run `cargo run -p patchwright-cli -- config check --repo .`.
- [ ] Run `cargo run -p patchwright-cli -- solve --repo . --task "summarize" --dry-run --info-only`.
- [ ] Commit docs if not already committed.
