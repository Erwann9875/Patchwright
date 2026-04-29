# Model Provider Contract Design

## Goal

Patchwright should avoid API-key OpenAI calls by default and still support many model backends without duplicating prompt, schema, parser, and validation code per provider.

## Architecture

Patchwright keeps one model contract and many thin transports.

```text
patchwright-core
  ModelProvider trait, Action, ModelRequest, ModelResponse

patchwright-model-contract
  prompt rendering
  action JSON schema
  action JSON parser
  shared contract tests

patchwright-model-openai
  OpenAI-compatible HTTP transport only

patchwright-model-codex-cli
  Codex CLI transport only
```

Providers do not define what an action means. A provider turns a `ModelRequest` into raw model output and then parses that output through `patchwright-model-contract`.

## Codex CLI Provider

The Codex CLI provider uses the user's existing ChatGPT/Codex login. It does not ask Patchwright users for an OpenAI API key.

Flow:

```text
ModelRequest
  -> render shared Patchwright prompt
  -> write shared action JSON Schema to a temp file
  -> run codex exec --ephemeral --sandbox read-only --ask-for-approval never --output-schema schema.json -o action.json -
  -> parse action.json with the shared contract parser
  -> return ModelResponse
```

Codex CLI must only propose one structured action. It must not edit files, run tests, or apply patches. Patchwright remains responsible for filesystem actions, verification, reverts, and acceptance.

The provider runs Codex from an isolated temporary directory with `--skip-git-repo-check`, passes all repo context through stdin, and uses `--sandbox read-only` plus `--ask-for-approval never`.

## Config

`patchwright.toml` selects the provider:

```toml
[model]
provider = "codex-cli"

[model.codex_cli]
command = "codex"
model = "gpt-5.1-codex"

[model.openai]
base_url = "https://api.openai.com/v1"
model = "gpt-5.5-pro"
api_key_env = "OPENAI_API_KEY"
```

Defaults:

- `provider = "codex-cli"`
- Codex command is `codex`
- OpenAI-compatible HTTP remains available through `provider = "openai-compatible"`
- `--dry-run` stays fully offline

CLI overrides:

- `--model-provider codex-cli|openai-compatible`
- `--model <name>` overrides the selected provider model
- `--base-url` and `--api-key-env` are OpenAI-compatible overrides and imply `openai-compatible` unless a provider is explicitly set

## Testing

The contract crate tests prompt rendering, schema shape, parser behavior, and path validation once. Provider crates test transport-specific behavior only. The Codex CLI provider uses a fake executable in tests so no real login or network call is required.

## Non-Goals

- No Apps SDK OAuth or GPT Actions OAuth.
- No direct Codex file editing from this provider.
- No provider-specific action schemas.
- No Claude/Ollama implementation in this change; the contract makes those later transports small.
