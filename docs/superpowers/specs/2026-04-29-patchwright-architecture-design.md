# Patchwright Architecture Design

Date: 2026-04-29

## Summary

Patchwright is a local-first Rust library and CLI for building a software-engineering coding agent. The first workflow is issue-to-patch: given a repository and a task, Patchwright inspects code, proposes structured actions, applies candidate patches, runs verifiers, records counterexamples, and accepts only evidence-backed diffs.

The model is a candidate generator. The source of truth is tool execution: compiler output, tests, type checking, linting, benchmarks, policy checks, and repository diff review.

The first implementation is language-agnostic in the core, with a Rust language adapter first. Execution is local-first through git worktrees and temporary directories, but all execution behavior goes through traits so Docker, remote workers, or CI execution can be added later.

Performance is a hard architectural constraint. Lightweight CLI commands must start in a few milliseconds. Heavy work such as repository scanning, model calls, async runtimes, indexing, and verification must be initialized lazily and only on commands that need them.

## Goals

- Build a well-tested Rust codebase with clear crate boundaries.
- Keep the core language-agnostic and adapter-driven.
- Provide an OpenAI-compatible model implementation first.
- Use structured tools by default, with a policy layer that can later grant broader shell rights.
- Use local execution first, while preserving a clean path to remote execution.
- Treat startup and hot-path latency as API-level constraints.
- Make verification stronger than generation.
- Produce minimal, reviewable patches with structured evidence.

## Non-Goals

- Runtime plugin loading in the first version.
- Docker, Firecracker, or remote workers in the first version.
- Embedding-based retrieval on the default path.
- Training a custom model.
- Supporting every language adapter at launch.
- Fully autonomous unrestricted shell execution by default.

## Workspace Layout

Patchwright should be a Rust workspace with statically linked crates:

```text
patchwright-cli
patchwright-core
patchwright-model-openai
patchwright-exec-local
patchwright-lang-rust
patchwright-index
patchwright-verify
patchwright-test-support
```

`patchwright-core` owns stable domain concepts:

- `TaskSpec`
- `AgentState`
- `Action`
- `Observation`
- `Attempt`
- `Counterexample`
- `Budget`
- `Policy`
- `VerifierPlan`
- scoring and acceptance decisions

The core must not depend on OpenAI-specific APIs, local process details, Cargo-specific behavior, or concrete index implementations.

Adapters implement traits defined by the core:

```rust
trait ModelProvider {
    fn propose_action(&self, request: ModelRequest) -> Result<ModelResponse>;
}

trait ExecutionBackend {
    fn snapshot(&self) -> Result<SnapshotId>;
    fn read_file(&self, path: RepoPath, range: Option<LineRange>) -> Result<FileSlice>;
    fn search(&self, query: SearchQuery) -> Result<SearchResults>;
    fn apply_patch(&self, patch: Patch) -> Result<PatchId>;
    fn run(&self, command: CommandSpec, policy: &Policy) -> Result<RunReport>;
    fn revert(&self, snapshot: SnapshotId) -> Result<()>;
}

trait LanguageAdapter {
    fn detect(&self, repo: &RepoView) -> DetectionScore;
    fn verifier_plan(&self, task: &TaskSpec, repo: &RepoView) -> VerifierPlan;
    fn relevant_files(&self, task: &TaskSpec, index: &dyn Indexer) -> Vec<ScoredPath>;
    fn summarize_failure(&self, report: &RunReport) -> Vec<Counterexample>;
}

trait Indexer {
    fn list_files(&self, query: FileQuery) -> Result<Vec<ScoredPath>>;
    fn search_text(&self, query: SearchQuery) -> Result<SearchResults>;
    fn symbols(&self, path: &RepoPath) -> Result<Vec<Symbol>>;
}
```

The CLI is thin. It parses arguments, loads minimal configuration, constructs adapters lazily, and calls the core.

## First Workflow: Issue to Patch

The first supported workflow is:

```text
task text + repo path
  -> initialize local repo view
  -> retrieve relevant context
  -> choose structured action
  -> execute action
  -> record observation
  -> if a patch changed the repo, verify it
  -> accept, reject, or retry with counterexamples
```

The agent state is explicit and serializable:

```rust
struct AgentState {
    task: TaskSpec,
    repo: RepoState,
    facts: Vec<KnownFact>,
    attempts: Vec<Attempt>,
    counterexamples: Vec<Counterexample>,
    budget: Budget,
}
```

Initial actions should be structured:

```text
ReadFile
SearchText
ListFiles
InspectSymbol
ApplyPatch
RunVerifier
RunTests
RunTypecheck
RunBenchmark
RevertAttempt
Finish
```

The first version should not expose arbitrary free-form shell access as the default model action. Shell access is mediated by policy.

## Verification and Acceptance

Verification is mandatory after any patch attempt.

A patch can be accepted only when:

- required verifier commands pass
- known tests pass
- generated regression tests pass when available
- type checking and linting pass when configured
- diff scope is acceptable
- no forbidden files are modified
- no denied commands were executed
- the review gate accepts the diff

Verifier output should be structured:

```rust
struct VerificationReport {
    status: VerificationStatus,
    checks: Vec<CheckReport>,
    counterexamples: Vec<Counterexample>,
    diff_summary: DiffSummary,
    policy_events: Vec<PolicyEvent>,
}
```

Raw logs should be captured for inspection, but model prompts should receive compact summaries and concrete counterexamples instead of unbounded log text.

## Counterexample-Guided Repair

Failed verifier output becomes memory for the next attempt.

Examples of useful counterexamples:

- failing test name
- failing input
- expected output
- actual output
- stack trace summary
- likely file and line
- invariant that must remain true

The next model request must include these counterexamples as hard constraints. Vague reflections such as "be more careful" should not be stored.

## Execution Architecture

The first execution backend is local:

- git worktree isolation
- temporary working directories
- clean-source preflight for the first version, until dirty-state overlay support exists
- structured file operations
- allowlisted process runner
- captured stdout and stderr
- timeouts
- portable resource limits where available
- explicit revert and cleanup

Future execution backends should fit the same core boundary:

- `LocalExecutionBackend`
- `RemoteExecutionBackend`
- `DockerExecutionBackend`
- `FirecrackerExecutionBackend`
- `CIExecutionBackend`

Command rights are controlled by policy:

```text
SafeStructuredOnly
ProjectConfiguredCommands
AllowlistedShell
FullShellWithConfirmation
FullShellAutonomous
```

This allows broader rights later without changing the model interface or agent loop.

## Model Architecture

The first model adapter is OpenAI-compatible HTTP.

The core still depends only on `ModelProvider`, so future providers are additional adapters rather than rewrites. The OpenAI-compatible adapter should support:

- base URL configuration
- model name configuration
- API key from environment or config indirection
- request timeout
- deterministic test mode through mocked HTTP
- structured request and response types

Normal tests must not call live model APIs. Live provider tests are opt-in through environment variables.

## Indexing and Retrieval

Indexing is language-agnostic and layered:

```text
file manifest
text search
symbol index
dependency/import graph
test map
optional semantic index later
```

Indexing must be lazy and incremental. The first `solve` can begin with file manifests and text search, then deepen only when needed.

Embeddings are not on the default path for the first version. They add latency, cache complexity, and provider coupling, so they should be introduced later as an optional retrieval backend.

## Rust Language Adapter

The Rust adapter is the first high-quality language implementation.

It should support:

- Cargo workspace detection
- `cargo metadata` parsing
- verifier plan generation
- `cargo fmt -- --check`
- `cargo check`
- `cargo test`
- optional `cargo clippy`
- test selector support where practical
- dependency awareness through `Cargo.lock`
- failure summarization for common Rust compiler and test output

Symbol extraction can use rust-analyzer or tree-sitter after measurement. The initial implementation should not put heavy symbol infrastructure on the CLI startup path.

## Configuration

Configuration should be compact and fast to parse. A representative shape:

```toml
[model]
provider = "openai-compatible"
base_url = "https://api.openai.com/v1"
model = "gpt-5.2"

[execution]
backend = "local"
policy = "project-configured-commands"

[verify.rust]
check = true
test = true
clippy = false
fmt = true

[budgets]
max_steps = 30
max_seconds = 900
max_patch_candidates = 8
```

Configuration loading for lightweight commands must avoid initializing adapters or reading the repository unless required by that command.

## CLI

Initial commands:

```text
patchwright solve --repo . --task "fix failing parser test"
patchwright verify --repo .
patchwright status
patchwright config check
patchwright bench startup
```

`solve` should produce:

- final diff
- attempt log
- verifier report
- accepted or rejected status
- optional machine-readable JSON

Lightweight commands such as `--version`, `status`, and `config check` must avoid heavy initialization.

## Library API

The library API should be explicit:

```rust
let agent = Agent::builder()
    .model(model_provider)
    .execution(execution_backend)
    .language_adapter(rust_adapter)
    .policy(policy)
    .verifier(verifier)
    .build();

let report = agent.solve(TaskSpec::from_text(issue_text))?;
```

This keeps the CLI as one consumer of the core rather than the center of the design.

## Performance Requirements

Startup and hot-path latency are architectural requirements.

The CLI startup path:

```text
main
  -> parse argv
  -> route command
  -> for lightweight commands, return immediately
  -> for solve, initialize only required adapters lazily
```

Forbidden on the global startup path:

- repository scans
- embedding loads
- async runtime creation
- model client initialization
- language adapter initialization
- dynamic plugin discovery
- config graph expansion

Targets:

- `patchwright --version`: single-digit millisecond target
- `patchwright status`: low single-digit to low tens of milliseconds, depending on required filesystem checks
- `patchwright config check`: low single-digit to low tens of milliseconds for small configs
- solve setup before model or verifier I/O: measured and budgeted
- index lookup, patch application, diff summary, and action dispatch: benchmarked

Dependency choices must be measured. If a dependency hurts startup, it must be kept off the lightweight path, gated behind a feature, or replaced.

Benchmarking should include:

- CLI cold-start benchmark
- config parse benchmark
- action dispatch benchmark
- patch apply benchmark
- search/index benchmark
- failure-log summarization benchmark

CI should use hard gates for deterministic microbenchmarks and trend/reporting for wall-clock startup until the CI environment is stable enough for reliable thresholds.

## Testing Strategy

Testing follows crate boundaries:

- core: deterministic unit tests for state transitions, scoring, budgets, action validation, and acceptance
- local execution: temp-repo integration tests for read, search, apply, run, revert, cleanup, timeout, and policy denial
- OpenAI-compatible model adapter: mocked HTTP contract tests
- Rust adapter: fixture repos for detection, verifier planning, Cargo command selection, and failure parsing
- index: fixture repos for manifest, text search, and symbol behavior
- CLI: command routing tests, startup smoke tests, and config behavior

Agent loop tests should use simulation:

- scripted model provider returns fixed actions
- fake execution backend returns fixed observations
- tests cover accept, reject, retry, revert, budget exhaustion, and policy denial

Normal tests must be offline. Network and live model tests are opt-in.

## GitHub Workflow Conventions

The GitHub-facing contributor guide lives in `CONTRIBUTING.md`. This section records the architectural decision behind that workflow.

Patchwright uses Conventional Commits for commit messages and pull request titles:

```text
<type>(<scope>): <summary>
```

Examples:

```text
feat(core): add agent state machine
fix(exec-local): preserve worktree cleanup on command timeout
perf(cli): avoid config parsing for version command
docs(architecture): add patchwright architecture design
test(lang-rust): cover cargo verifier planning
```

Allowed initial types:

- `feat`: user-visible feature or capability
- `fix`: bug fix
- `perf`: performance improvement
- `refactor`: internal change with no intended behavior change
- `test`: test-only change
- `docs`: documentation-only change
- `ci`: CI workflow change
- `build`: build system or dependency change
- `chore`: maintenance that does not fit another type
- `revert`: explicit revert

Scopes should be short and map to crates or owned areas, such as `core`, `cli`, `exec-local`, `model-openai`, `lang-rust`, `index`, `verify`, `architecture`, `github`, or `ci`.

Commit summaries should be imperative, lowercase after the colon unless naming a proper noun, and should not end with a period. Breaking changes use the Conventional Commits `!` marker and explain the break in the body:

```text
feat(core)!: change action execution contract

BREAKING CHANGE: ExecutionBackend::run now requires a Policy argument.
```

Pull requests should use the same format for the PR title:

```text
feat(lang-rust): add cargo verifier planning
```

PR descriptions should include:

- summary of the change
- verification commands and results
- performance impact when relevant
- risk or rollback notes when relevant
- linked issue when one exists

Each PR should prefer one logical change. Squash commits should keep the PR title as the final commit message unless the PR intentionally contains multiple conventional commits.

## First Implementation Slice

The first implementation should build the spine before adding advanced intelligence:

1. Workspace and crate layout.
2. Core domain types and traits.
3. Thin CLI with `--version`, `status`, and `config check`.
4. Local execution backend with read, search, apply patch, run command, snapshot, and revert.
5. Rust adapter with Cargo detection and verifier plan.
6. Simulated agent loop test using fake model and fake execution.
7. OpenAI-compatible model adapter behind the trait.
8. `solve` MVP with structured action loop and verification.
9. Startup and core microbenchmarks.

This order proves boundaries, performance discipline, and testability before relying on live model behavior.
