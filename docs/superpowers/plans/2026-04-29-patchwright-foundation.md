# Patchwright Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first working Patchwright foundation: a fast Rust workspace with core agent contracts, local execution, Rust verifier planning, OpenAI-compatible model boundary, CLI commands, offline tests, and startup/performance checks.

**Architecture:** Use statically linked Rust crates with a language-agnostic core and concrete adapters for local execution, Rust projects, indexing, verification, and OpenAI-compatible model calls. Keep CLI startup work minimal by routing lightweight commands before config, repository, model, or async/network initialization.

**Tech Stack:** Rust 2021 edition, Cargo workspace, standard-library CLI parsing, synchronous process execution, mocked HTTP tests for model behavior, offline fixture repositories, Conventional Commits.

---

## Scope

This plan implements the approved architecture spec's first implementation slice. It does not implement remote execution, dynamic plugins, embeddings, Docker/Firecracker, or unrestricted autonomous shell access.

## File Structure

Create this workspace structure:

```text
Cargo.toml
CONTRIBUTING.md
README.md
crates/
  patchwright-cli/
    Cargo.toml
    src/main.rs
  patchwright-core/
    Cargo.toml
    src/action.rs
    src/agent.rs
    src/error.rs
    src/lib.rs
    src/policy.rs
    src/traits.rs
    src/types.rs
    tests/agent_loop.rs
  patchwright-exec-local/
    Cargo.toml
    src/lib.rs
    tests/local_execution.rs
  patchwright-index/
    Cargo.toml
    src/lib.rs
    tests/basic_index.rs
  patchwright-lang-rust/
    Cargo.toml
    src/lib.rs
    tests/rust_adapter.rs
  patchwright-model-openai/
    Cargo.toml
    src/lib.rs
    tests/openai_client.rs
  patchwright-test-support/
    Cargo.toml
    src/lib.rs
  patchwright-verify/
    Cargo.toml
    src/lib.rs
    tests/verifier.rs
docs/
  superpowers/
    plans/2026-04-29-patchwright-foundation.md
    specs/2026-04-29-patchwright-architecture-design.md
```

Responsibilities:

- `patchwright-core`: domain types, policy, trait contracts, and deterministic agent loop.
- `patchwright-cli`: manual argument routing and lazy adapter construction.
- `patchwright-exec-local`: local filesystem, git, patch, and process execution.
- `patchwright-index`: lightweight file manifest and text search.
- `patchwright-lang-rust`: Cargo/Rust detection and verifier plan generation.
- `patchwright-model-openai`: OpenAI-compatible request/response adapter behind `ModelProvider`.
- `patchwright-test-support`: temporary directory and git fixture helpers for integration tests.
- `patchwright-verify`: verifier runner that executes planned commands and produces structured reports.

## Task 1: Workspace Skeleton and Fast CLI Route

**Files:**
- Create: `Cargo.toml`
- Create: `crates/patchwright-cli/Cargo.toml`
- Create: `crates/patchwright-cli/src/main.rs`
- Create: `crates/patchwright-core/Cargo.toml`
- Create: `crates/patchwright-core/src/lib.rs`
- Modify: `README.md`

- [ ] **Step 1: Write the workspace manifest**

Create `Cargo.toml`:

```toml
[workspace]
members = [
    "crates/patchwright-cli",
    "crates/patchwright-core",
]
resolver = "2"

[workspace.package]
edition = "2021"
license = "MIT"
repository = "https://github.com/Erwann9875/Patchwright"

[workspace.lints.rust]
unsafe_code = "forbid"

[workspace.lints.clippy]
dbg_macro = "deny"
todo = "deny"
```

Add later crates to `workspace.members` in the task that creates each crate. Do not list crates before their `Cargo.toml` exists, because Cargo refuses to load a workspace with missing members.

- [ ] **Step 2: Write minimal core crate files**

Create `crates/patchwright-core/Cargo.toml`:

```toml
[package]
name = "patchwright-core"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true

[lib]
path = "src/lib.rs"

[lints]
workspace = true
```

Create `crates/patchwright-core/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
```

- [ ] **Step 3: Write the first CLI route without heavy initialization**

Create `crates/patchwright-cli/Cargo.toml`:

```toml
[package]
name = "patchwright-cli"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true

[[bin]]
name = "patchwright"
path = "src/main.rs"

[dependencies]
patchwright-core = { path = "../patchwright-core" }

[lints]
workspace = true
```

Create `crates/patchwright-cli/src/main.rs`:

```rust
#![forbid(unsafe_code)]

use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run(env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

fn run<I>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = String>,
{
    let args: Vec<String> = args.into_iter().collect();

    if args.is_empty() || args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_help();
        return Ok(());
    }

    if args.iter().any(|arg| arg == "-V" || arg == "--version") {
        println!("patchwright {}", patchwright_core::VERSION);
        return Ok(());
    }

    match args[0].as_str() {
        "status" => {
            println!("patchwright: ready");
            Ok(())
        }
        "config" if args.get(1).map(String::as_str) == Some("check") => {
            println!("config: no config file required for this command");
            Ok(())
        }
        "bench" if args.get(1).map(String::as_str) == Some("startup") => {
            println!("startup benchmark command registered");
            Ok(())
        }
        "solve" => Err("solve is not wired yet".to_owned()),
        "verify" => Err("verify is not wired yet".to_owned()),
        other => Err(format!("unknown command: {other}")),
    }
}

fn print_help() {
    println!(
        "patchwright\n\nUSAGE:\n    patchwright --version\n    patchwright status\n    patchwright config check\n    patchwright bench startup\n    patchwright solve --repo <path> --task <text>\n    patchwright verify --repo <path>"
    );
}

#[cfg(test)]
mod tests {
    use super::run;

    #[test]
    fn version_route_returns_before_heavy_commands() {
        let result = run(["--version".to_owned()]);
        assert!(result.is_ok());
    }

    #[test]
    fn unknown_command_is_an_error() {
        let result = run(["unknown".to_owned()]);
        assert_eq!(result, Err("unknown command: unknown".to_owned()));
    }
}
```

- [ ] **Step 4: Update README with the project identity**

Modify `README.md`:

```markdown
# Patchwright

Patchwright is a local-first Rust foundation for a software-engineering coding agent. The model proposes candidate actions and patches; compilers, tests, linters, type checkers, benchmarks, policies, and reviewers decide what is true.

The first build focuses on a language-agnostic core, a Rust adapter, local execution, OpenAI-compatible model access, and strict verification.
```

- [ ] **Step 5: Verify the skeleton**

Run: `cargo test --workspace`

Expected: all tests pass.

Run: `cargo run -p patchwright-cli -- --version`

Expected: output starts with `patchwright 0.1.0`.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml README.md crates/patchwright-cli crates/patchwright-core
git commit -m "feat(cli): add workspace skeleton"
```

## Task 2: Core Domain Types and Policy

**Files:**
- Create: `crates/patchwright-core/src/error.rs`
- Create: `crates/patchwright-core/src/types.rs`
- Create: `crates/patchwright-core/src/action.rs`
- Create: `crates/patchwright-core/src/policy.rs`
- Modify: `crates/patchwright-core/src/lib.rs`

- [ ] **Step 1: Write core error handling**

Create `crates/patchwright-core/src/error.rs`:

```rust
use std::error::Error;
use std::fmt::{Display, Formatter};

pub type Result<T> = std::result::Result<T, PatchwrightError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchwrightError {
    InvalidInput(String),
    Io(String),
    CommandFailed(String),
    PolicyDenied(String),
    Verification(String),
    Model(String),
}

impl Display for PatchwrightError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(message)
            | Self::Io(message)
            | Self::CommandFailed(message)
            | Self::PolicyDenied(message)
            | Self::Verification(message)
            | Self::Model(message) => f.write_str(message),
        }
    }
}

impl Error for PatchwrightError {}

impl From<std::io::Error> for PatchwrightError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}
```

- [ ] **Step 2: Write serializable-by-design domain types without adding serde**

Create `crates/patchwright-core/src/types.rs`:

```rust
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RepoPath(pub String);

impl RepoPath {
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSlice {
    pub path: RepoPath,
    pub start_line: usize,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    pub pattern: String,
    pub root: Option<RepoPath>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatch {
    pub path: RepoPath,
    pub line: usize,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResults {
    pub matches: Vec<SearchMatch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<RepoPath>,
    pub stdin: Option<String>,
    pub timeout: Duration,
}

impl CommandSpec {
    pub fn new(program: impl Into<String>, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
            cwd: None,
            stdin: None,
            timeout: Duration::from_secs(60),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitStatus {
    pub code: Option<i32>,
    pub success: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReport {
    pub command: CommandSpec,
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patch {
    pub unified_diff: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SnapshotId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PatchId(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSpec {
    pub text: String,
    pub repo_path: PathBuf,
}

impl TaskSpec {
    pub fn from_text(repo_path: PathBuf, text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            repo_path,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoState {
    pub root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownFact {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Counterexample {
    pub source: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attempt {
    pub patch_id: Option<PatchId>,
    pub verification: VerificationReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Budget {
    pub max_steps: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationStatus {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckReport {
    pub name: String,
    pub command: Option<CommandSpec>,
    pub passed: bool,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiffSummary {
    pub changed_files: Vec<RepoPath>,
    pub inserted_lines: usize,
    pub deleted_lines: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyEvent {
    pub command: Option<CommandSpec>,
    pub allowed: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationReport {
    pub status: VerificationStatus,
    pub checks: Vec<CheckReport>,
    pub counterexamples: Vec<Counterexample>,
    pub diff_summary: DiffSummary,
    pub policy_events: Vec<PolicyEvent>,
}

impl VerificationReport {
    pub fn accepted() -> Self {
        Self {
            status: VerificationStatus::Accepted,
            checks: Vec::new(),
            counterexamples: Vec::new(),
            diff_summary: DiffSummary::default(),
            policy_events: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifierPlan {
    pub commands: Vec<CommandSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DetectionScore(pub u8);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoredPath {
    pub path: RepoPath,
    pub score: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileQuery {
    pub root: Option<RepoPath>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub kind: String,
    pub path: RepoPath,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoView {
    pub root: PathBuf,
}
```

- [ ] **Step 3: Write structured actions and observations**

Create `crates/patchwright-core/src/action.rs`:

```rust
use crate::types::{
    FileQuery, FileSlice, LineRange, Patch, RepoPath, RunReport, SearchQuery, SearchResults,
    SnapshotId, VerificationReport,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    ReadFile {
        path: RepoPath,
        range: Option<LineRange>,
    },
    SearchText(SearchQuery),
    ListFiles(FileQuery),
    ApplyPatch(Patch),
    RunVerifier,
    RunTests,
    RunTypecheck,
    RunBenchmark,
    RevertAttempt(SnapshotId),
    Finish {
        summary: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observation {
    FileRead(FileSlice),
    SearchCompleted(SearchResults),
    FilesListed(Vec<RepoPath>),
    PatchApplied,
    CommandCompleted(RunReport),
    VerificationCompleted(VerificationReport),
    Reverted(SnapshotId),
    Finished(String),
    Error(String),
}
```

- [ ] **Step 4: Write policy types and unit tests**

Create `crates/patchwright-core/src/policy.rs`:

```rust
use crate::types::CommandSpec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Policy {
    SafeStructuredOnly,
    ProjectConfiguredCommands { allowed_programs: Vec<String> },
    AllowlistedShell { allowed_programs: Vec<String> },
    FullShellWithConfirmation,
    FullShellAutonomous,
}

impl Policy {
    pub fn allows(&self, command: &CommandSpec) -> bool {
        match self {
            Self::SafeStructuredOnly => false,
            Self::ProjectConfiguredCommands { allowed_programs }
            | Self::AllowlistedShell { allowed_programs } => {
                allowed_programs.iter().any(|program| program == &command.program)
            }
            Self::FullShellWithConfirmation => false,
            Self::FullShellAutonomous => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::policy::Policy;
    use crate::types::CommandSpec;

    #[test]
    fn safe_structured_policy_denies_processes() {
        let policy = Policy::SafeStructuredOnly;
        let command = CommandSpec::new("cargo", ["test"]);
        assert!(!policy.allows(&command));
    }

    #[test]
    fn project_configured_policy_allows_listed_programs() {
        let policy = Policy::ProjectConfiguredCommands {
            allowed_programs: vec!["cargo".to_owned()],
        };
        let command = CommandSpec::new("cargo", ["test"]);
        assert!(policy.allows(&command));
    }
}
```

- [ ] **Step 5: Export the modules**

Modify `crates/patchwright-core/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

pub mod action;
pub mod error;
pub mod policy;
pub mod types;

pub use error::{PatchwrightError, Result};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
```

- [ ] **Step 6: Verify core types**

Run: `cargo test -p patchwright-core`

Expected: policy tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/patchwright-core
git commit -m "feat(core): add domain types and policy"
```

## Task 3: Core Trait Boundaries and Simulated Agent Loop

**Files:**
- Create: `crates/patchwright-core/src/traits.rs`
- Create: `crates/patchwright-core/src/agent.rs`
- Create: `crates/patchwright-core/tests/agent_loop.rs`
- Modify: `crates/patchwright-core/src/lib.rs`

- [ ] **Step 1: Write the failing simulation test**

Create `crates/patchwright-core/tests/agent_loop.rs`:

```rust
use patchwright_core::action::{Action, Observation};
use patchwright_core::agent::{Agent, SolveStatus};
use patchwright_core::error::{PatchwrightError, Result};
use patchwright_core::policy::Policy;
use patchwright_core::traits::{
    ExecutionBackend, Indexer, LanguageAdapter, ModelProvider, Verifier,
};
use patchwright_core::types::{
    CommandSpec, DetectionScore, FileQuery, FileSlice, LineRange, ModelRequest, ModelResponse,
    Patch, PatchId, RepoPath, RepoView, RunReport, ScoredPath, SearchQuery, SearchResults,
    SnapshotId, TaskSpec, VerificationReport, VerifierPlan,
};
use std::path::PathBuf;

struct ScriptedModel {
    actions: Vec<Action>,
}

impl ModelProvider for ScriptedModel {
    fn propose_action(&mut self, _request: ModelRequest) -> Result<ModelResponse> {
        if self.actions.is_empty() {
            return Err(PatchwrightError::Model("script exhausted".to_owned()));
        }
        Ok(ModelResponse {
            action: self.actions.remove(0),
        })
    }
}

#[derive(Default)]
struct FakeExecution {
    reverted: bool,
}

impl ExecutionBackend for FakeExecution {
    fn snapshot(&mut self) -> Result<SnapshotId> {
        Ok(SnapshotId("snap-1".to_owned()))
    }

    fn read_file(&self, path: RepoPath, _range: Option<LineRange>) -> Result<FileSlice> {
        Ok(FileSlice {
            path,
            start_line: 1,
            content: "fn main() {}\n".to_owned(),
        })
    }

    fn search(&self, _query: SearchQuery) -> Result<SearchResults> {
        Ok(SearchResults { matches: Vec::new() })
    }

    fn apply_patch(&mut self, _patch: Patch) -> Result<PatchId> {
        Ok(PatchId("patch-1".to_owned()))
    }

    fn run(&mut self, command: CommandSpec, _policy: &Policy) -> Result<RunReport> {
        Ok(RunReport {
            command,
            status: patchwright_core::types::ExitStatus {
                code: Some(0),
                success: true,
            },
            stdout: String::new(),
            stderr: String::new(),
        })
    }

    fn revert(&mut self, _snapshot: SnapshotId) -> Result<()> {
        self.reverted = true;
        Ok(())
    }
}

struct FakeLanguage;

impl LanguageAdapter for FakeLanguage {
    fn detect(&self, _repo: &RepoView) -> DetectionScore {
        DetectionScore(100)
    }

    fn verifier_plan(&self, _task: &TaskSpec, _repo: &RepoView) -> VerifierPlan {
        VerifierPlan { commands: Vec::new() }
    }

    fn relevant_files(
        &self,
        _task: &TaskSpec,
        _index: &dyn Indexer,
    ) -> Vec<ScoredPath> {
        Vec::new()
    }

    fn summarize_failure(&self, _report: &RunReport) -> Vec<patchwright_core::types::Counterexample> {
        Vec::new()
    }
}

struct EmptyIndex;

impl Indexer for EmptyIndex {
    fn list_files(&self, _query: FileQuery) -> Result<Vec<ScoredPath>> {
        Ok(Vec::new())
    }

    fn search_text(&self, _query: SearchQuery) -> Result<SearchResults> {
        Ok(SearchResults { matches: Vec::new() })
    }

    fn symbols(&self, _path: &RepoPath) -> Result<Vec<patchwright_core::types::Symbol>> {
        Ok(Vec::new())
    }
}

struct AcceptingVerifier;

impl Verifier for AcceptingVerifier {
    fn verify(
        &mut self,
        _execution: &mut dyn ExecutionBackend,
        _plan: &VerifierPlan,
        _policy: &Policy,
    ) -> Result<VerificationReport> {
        Ok(VerificationReport::accepted())
    }
}

#[test]
fn accepts_patch_after_successful_verification() {
    let model = ScriptedModel {
        actions: vec![Action::ApplyPatch(Patch {
            unified_diff: "diff --git a/a b/a\n".to_owned(),
        })],
    };

    let mut agent = Agent::builder()
        .model(model)
        .execution(FakeExecution::default())
        .language_adapter(FakeLanguage)
        .indexer(EmptyIndex)
        .verifier(AcceptingVerifier)
        .policy(Policy::SafeStructuredOnly)
        .max_steps(3)
        .build();

    let report = agent
        .solve(TaskSpec::from_text(PathBuf::from("."), "change code"))
        .expect("simulation should solve");

    assert_eq!(report.status, SolveStatus::Accepted);
    assert_eq!(report.attempts.len(), 1);
    assert!(matches!(
        report.observations.last(),
        Some(Observation::VerificationCompleted(_))
    ));
}
```

Run: `cargo test -p patchwright-core --test agent_loop`

Expected: compilation fails because traits, agent, and model request types do not exist.

- [ ] **Step 2: Add model request and response types**

Modify `crates/patchwright-core/src/types.rs` by appending:

```rust
use crate::action::Observation;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRequest {
    pub task: TaskSpec,
    pub observations: Vec<Observation>,
    pub counterexamples: Vec<Counterexample>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelResponse {
    pub action: crate::action::Action,
}
```

- [ ] **Step 3: Define adapter traits**

Create `crates/patchwright-core/src/traits.rs`:

```rust
use crate::error::Result;
use crate::policy::Policy;
use crate::types::{
    CommandSpec, DetectionScore, FileQuery, FileSlice, LineRange, ModelRequest, ModelResponse,
    Patch, PatchId, RepoPath, RepoView, RunReport, ScoredPath, SearchQuery, SearchResults,
    SnapshotId, Symbol, TaskSpec, VerificationReport, VerifierPlan,
};

pub trait ModelProvider {
    fn propose_action(&mut self, request: ModelRequest) -> Result<ModelResponse>;
}

pub trait ExecutionBackend {
    fn snapshot(&mut self) -> Result<SnapshotId>;
    fn read_file(&self, path: RepoPath, range: Option<LineRange>) -> Result<FileSlice>;
    fn search(&self, query: SearchQuery) -> Result<SearchResults>;
    fn apply_patch(&mut self, patch: Patch) -> Result<PatchId>;
    fn run(&mut self, command: CommandSpec, policy: &Policy) -> Result<RunReport>;
    fn revert(&mut self, snapshot: SnapshotId) -> Result<()>;
}

pub trait LanguageAdapter {
    fn detect(&self, repo: &RepoView) -> DetectionScore;
    fn verifier_plan(&self, task: &TaskSpec, repo: &RepoView) -> VerifierPlan;
    fn relevant_files(&self, task: &TaskSpec, index: &dyn Indexer) -> Vec<ScoredPath>;
    fn summarize_failure(&self, report: &RunReport) -> Vec<crate::types::Counterexample>;
}

pub trait Indexer {
    fn list_files(&self, query: FileQuery) -> Result<Vec<ScoredPath>>;
    fn search_text(&self, query: SearchQuery) -> Result<SearchResults>;
    fn symbols(&self, path: &RepoPath) -> Result<Vec<Symbol>>;
}

pub trait Verifier {
    fn verify(
        &mut self,
        execution: &mut dyn ExecutionBackend,
        plan: &VerifierPlan,
        policy: &Policy,
    ) -> Result<VerificationReport>;
}
```

- [ ] **Step 4: Implement the deterministic agent loop**

Create `crates/patchwright-core/src/agent.rs`:

```rust
use crate::action::{Action, Observation};
use crate::error::Result;
use crate::policy::Policy;
use crate::traits::{ExecutionBackend, Indexer, LanguageAdapter, ModelProvider, Verifier};
use crate::types::{
    Attempt, Counterexample, ModelRequest, RepoView, TaskSpec, VerificationReport,
    VerificationStatus,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SolveStatus {
    Accepted,
    Finished,
    BudgetExhausted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolveReport {
    pub status: SolveStatus,
    pub summary: String,
    pub attempts: Vec<Attempt>,
    pub observations: Vec<Observation>,
    pub counterexamples: Vec<Counterexample>,
}

pub struct Agent {
    model: Box<dyn ModelProvider>,
    execution: Box<dyn ExecutionBackend>,
    language_adapter: Box<dyn LanguageAdapter>,
    indexer: Box<dyn Indexer>,
    verifier: Box<dyn Verifier>,
    policy: Policy,
    max_steps: usize,
}

impl Agent {
    pub fn builder() -> AgentBuilder {
        AgentBuilder::default()
    }

    pub fn solve(&mut self, task: TaskSpec) -> Result<SolveReport> {
        let mut observations = Vec::new();
        let mut attempts = Vec::new();
        let mut counterexamples = Vec::new();
        let repo = RepoView {
            root: task.repo_path.clone(),
        };

        for _step in 0..self.max_steps {
            let response = self.model.propose_action(ModelRequest {
                task: task.clone(),
                observations: observations.clone(),
                counterexamples: counterexamples.clone(),
            })?;

            match response.action {
                Action::ReadFile { path, range } => {
                    let slice = self.execution.read_file(path, range)?;
                    observations.push(Observation::FileRead(slice));
                }
                Action::SearchText(query) => {
                    let results = self.execution.search(query)?;
                    observations.push(Observation::SearchCompleted(results));
                }
                Action::ListFiles(query) => {
                    let files = self
                        .indexer
                        .list_files(query)?
                        .into_iter()
                        .map(|scored| scored.path)
                        .collect();
                    observations.push(Observation::FilesListed(files));
                }
                Action::ApplyPatch(patch) => {
                    let snapshot = self.execution.snapshot()?;
                    let patch_id = self.execution.apply_patch(patch)?;
                    observations.push(Observation::PatchApplied);
                    let plan = self.language_adapter.verifier_plan(&task, &repo);
                    let verification =
                        self.verifier
                            .verify(self.execution.as_mut(), &plan, &self.policy)?;
                    counterexamples.extend(verification.counterexamples.clone());
                    let accepted = verification.status == VerificationStatus::Accepted;
                    observations.push(Observation::VerificationCompleted(verification.clone()));
                    attempts.push(Attempt {
                        patch_id: Some(patch_id),
                        verification,
                    });

                    if accepted {
                        return Ok(SolveReport {
                            status: SolveStatus::Accepted,
                            summary: "accepted patch".to_owned(),
                            attempts,
                            observations,
                            counterexamples,
                        });
                    }

                    self.execution.revert(snapshot.clone())?;
                    observations.push(Observation::Reverted(snapshot));
                }
                Action::RunVerifier => {
                    let plan = self.language_adapter.verifier_plan(&task, &repo);
                    let verification =
                        self.verifier
                            .verify(self.execution.as_mut(), &plan, &self.policy)?;
                    counterexamples.extend(verification.counterexamples.clone());
                    observations.push(Observation::VerificationCompleted(verification));
                }
                Action::RunTests | Action::RunTypecheck | Action::RunBenchmark => {
                    let plan = self.language_adapter.verifier_plan(&task, &repo);
                    let verification =
                        self.verifier
                            .verify(self.execution.as_mut(), &plan, &self.policy)?;
                    observations.push(Observation::VerificationCompleted(verification));
                }
                Action::RevertAttempt(snapshot) => {
                    self.execution.revert(snapshot.clone())?;
                    observations.push(Observation::Reverted(snapshot));
                }
                Action::Finish { summary } => {
                    observations.push(Observation::Finished(summary.clone()));
                    return Ok(SolveReport {
                        status: SolveStatus::Finished,
                        summary,
                        attempts,
                        observations,
                        counterexamples,
                    });
                }
            }
        }

        Ok(SolveReport {
            status: SolveStatus::BudgetExhausted,
            summary: "step budget exhausted".to_owned(),
            attempts,
            observations,
            counterexamples,
        })
    }
}

#[derive(Default)]
pub struct AgentBuilder {
    model: Option<Box<dyn ModelProvider>>,
    execution: Option<Box<dyn ExecutionBackend>>,
    language_adapter: Option<Box<dyn LanguageAdapter>>,
    indexer: Option<Box<dyn Indexer>>,
    verifier: Option<Box<dyn Verifier>>,
    policy: Option<Policy>,
    max_steps: Option<usize>,
}

impl AgentBuilder {
    pub fn model(mut self, model: impl ModelProvider + 'static) -> Self {
        self.model = Some(Box::new(model));
        self
    }

    pub fn execution(mut self, execution: impl ExecutionBackend + 'static) -> Self {
        self.execution = Some(Box::new(execution));
        self
    }

    pub fn language_adapter(mut self, language_adapter: impl LanguageAdapter + 'static) -> Self {
        self.language_adapter = Some(Box::new(language_adapter));
        self
    }

    pub fn indexer(mut self, indexer: impl Indexer + 'static) -> Self {
        self.indexer = Some(Box::new(indexer));
        self
    }

    pub fn verifier(mut self, verifier: impl Verifier + 'static) -> Self {
        self.verifier = Some(Box::new(verifier));
        self
    }

    pub fn policy(mut self, policy: Policy) -> Self {
        self.policy = Some(policy);
        self
    }

    pub fn max_steps(mut self, max_steps: usize) -> Self {
        self.max_steps = Some(max_steps);
        self
    }

    pub fn build(self) -> Agent {
        Agent {
            model: self.model.unwrap_or_else(|| panic!("model is required")),
            execution: self
                .execution
                .unwrap_or_else(|| panic!("execution backend is required")),
            language_adapter: self
                .language_adapter
                .unwrap_or_else(|| panic!("language adapter is required")),
            indexer: self.indexer.unwrap_or_else(|| panic!("indexer is required")),
            verifier: self.verifier.unwrap_or_else(|| panic!("verifier is required")),
            policy: self.policy.unwrap_or(Policy::SafeStructuredOnly),
            max_steps: self.max_steps.unwrap_or(30),
        }
    }
}
```

- [ ] **Step 5: Export the new modules**

Modify `crates/patchwright-core/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

pub mod action;
pub mod agent;
pub mod error;
pub mod policy;
pub mod traits;
pub mod types;

pub use error::{PatchwrightError, Result};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
```

- [ ] **Step 6: Verify the core loop**

Run: `cargo test -p patchwright-core --test agent_loop`

Expected: `accepts_patch_after_successful_verification` passes.

Run: `cargo test -p patchwright-core`

Expected: all core tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/patchwright-core
git commit -m "feat(core): add agent contracts and loop"
```

## Task 4: Test Support for Temporary Git Repositories

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/patchwright-test-support/Cargo.toml`
- Create: `crates/patchwright-test-support/src/lib.rs`

- [ ] **Step 1: Create the crate manifest**

Modify root `Cargo.toml` to add the new crate:

```toml
[workspace]
members = [
    "crates/patchwright-cli",
    "crates/patchwright-core",
    "crates/patchwright-test-support",
]
```

Create `crates/patchwright-test-support/Cargo.toml`:

```toml
[package]
name = "patchwright-test-support"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true

[lib]
path = "src/lib.rs"

[lints]
workspace = true
```

- [ ] **Step 2: Write temporary repo helpers**

Create `crates/patchwright-test-support/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    pub fn new(name: &str) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "patchwright-{name}-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap_or_else(|error| {
            panic!("failed to create temp repo {}: {error}", root.display())
        });

        run_git(&root, ["init"]);
        run_git(&root, ["config", "user.email", "patchwright@example.invalid"]);
        run_git(&root, ["config", "user.name", "Patchwright Tests"]);

        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn write(&self, relative: &str, content: &str) {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|error| {
                panic!("failed to create {}: {error}", parent.display())
            });
        }
        fs::write(&path, content)
            .unwrap_or_else(|error| panic!("failed to write {}: {error}", path.display()));
    }

    pub fn commit_all(&self, message: &str) {
        run_git(&self.root, ["add", "."]);
        run_git(&self.root, ["commit", "-m", message]);
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        if self.root.exists() {
            fs::remove_dir_all(&self.root).unwrap_or_else(|error| {
                panic!("failed to remove temp repo {}: {error}", self.root.display())
            });
        }
    }
}

fn run_git<const N: usize>(root: &Path, args: [&str; N]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("failed to run git in {}: {error}", root.display()));

    if !output.status.success() {
        panic!(
            "git failed in {}\nstdout:\n{}\nstderr:\n{}",
            root.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
```

- [ ] **Step 3: Verify the helper crate**

Run: `cargo test -p patchwright-test-support`

Expected: crate compiles and has zero tests.

- [ ] **Step 4: Commit**

```bash
git add crates/patchwright-test-support
git commit -m "test(support): add temporary git repositories"
```

## Task 5: Local Execution Backend

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/patchwright-exec-local/Cargo.toml`
- Create: `crates/patchwright-exec-local/src/lib.rs`
- Create: `crates/patchwright-exec-local/tests/local_execution.rs`

- [ ] **Step 1: Create the crate manifest**

Modify root `Cargo.toml` to add the new crate:

```toml
[workspace]
members = [
    "crates/patchwright-cli",
    "crates/patchwright-core",
    "crates/patchwright-test-support",
    "crates/patchwright-exec-local",
]
```

Create `crates/patchwright-exec-local/Cargo.toml`:

```toml
[package]
name = "patchwright-exec-local"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true

[lib]
path = "src/lib.rs"

[dependencies]
patchwright-core = { path = "../patchwright-core" }

[dev-dependencies]
patchwright-test-support = { path = "../patchwright-test-support" }

[lints]
workspace = true
```

- [ ] **Step 2: Write failing integration tests**

Create `crates/patchwright-exec-local/tests/local_execution.rs`:

```rust
use patchwright_core::policy::Policy;
use patchwright_core::traits::ExecutionBackend;
use patchwright_core::types::{CommandSpec, LineRange, Patch, RepoPath, SearchQuery};
use patchwright_exec_local::LocalExecution;
use patchwright_test_support::TempRepo;

#[test]
fn reads_file_ranges_and_searches_text() {
    let repo = TempRepo::new("exec-read-search");
    repo.write("src/lib.rs", "alpha\nbeta\ngamma\n");
    repo.commit_all("seed");

    let backend = LocalExecution::new(repo.root());
    let slice = backend
        .read_file(RepoPath::new("src/lib.rs"), Some(LineRange { start: 2, end: 2 }))
        .unwrap();
    assert_eq!(slice.content, "beta\n");

    let results = backend
        .search(SearchQuery {
            pattern: "gamma".to_owned(),
            root: None,
        })
        .unwrap();
    assert_eq!(results.matches.len(), 1);
    assert_eq!(results.matches[0].line, 3);
}

#[test]
fn applies_patch_and_reverts_to_snapshot() {
    let repo = TempRepo::new("exec-patch-revert");
    repo.write("file.txt", "before\n");
    repo.commit_all("seed");

    let mut backend = LocalExecution::new(repo.root());
    let snapshot = backend.snapshot().unwrap();
    backend
        .apply_patch(Patch {
            unified_diff: concat!(
                "diff --git a/file.txt b/file.txt\n",
                "index 2bb6b3e..c7c5fd2 100644\n",
                "--- a/file.txt\n",
                "+++ b/file.txt\n",
                "@@ -1 +1 @@\n",
                "-before\n",
                "+after\n"
            )
            .to_owned(),
        })
        .unwrap();

    let changed = backend
        .read_file(RepoPath::new("file.txt"), None)
        .unwrap()
        .content;
    assert_eq!(changed, "after\n");

    backend.revert(snapshot).unwrap();
    let restored = backend
        .read_file(RepoPath::new("file.txt"), None)
        .unwrap()
        .content;
    assert_eq!(restored, "before\n");
}

#[test]
fn policy_denies_unlisted_commands() {
    let repo = TempRepo::new("exec-policy");
    repo.write("file.txt", "content\n");
    repo.commit_all("seed");

    let mut backend = LocalExecution::new(repo.root());
    let result = backend.run(
        CommandSpec::new("git", ["status"]),
        &Policy::ProjectConfiguredCommands {
            allowed_programs: vec!["cargo".to_owned()],
        },
    );

    assert!(result.is_err());
}
```

Run: `cargo test -p patchwright-exec-local`

Expected: compilation fails because `LocalExecution` does not exist.

- [ ] **Step 3: Implement local read, search, patch, run, and revert**

Create `crates/patchwright-exec-local/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

use patchwright_core::error::{PatchwrightError, Result};
use patchwright_core::policy::Policy;
use patchwright_core::traits::ExecutionBackend;
use patchwright_core::types::{
    CommandSpec, ExitStatus, FileSlice, LineRange, Patch, PatchId, RepoPath, RunReport,
    SearchMatch, SearchQuery, SearchResults, SnapshotId,
};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct LocalExecution {
    root: PathBuf,
}

impl LocalExecution {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    fn resolve(&self, path: &RepoPath) -> Result<PathBuf> {
        if path.0.contains("..") {
            return Err(PatchwrightError::InvalidInput(format!(
                "repo path cannot contain '..': {}",
                path.0
            )));
        }
        Ok(self.root.join(&path.0))
    }
}

impl ExecutionBackend for LocalExecution {
    fn snapshot(&mut self) -> Result<SnapshotId> {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["rev-parse", "HEAD"])
            .output()?;
        if !output.status.success() {
            return Err(PatchwrightError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ));
        }
        Ok(SnapshotId(
            String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        ))
    }

    fn read_file(&self, path: RepoPath, range: Option<LineRange>) -> Result<FileSlice> {
        let content = fs::read_to_string(self.resolve(&path)?)?;
        let mut start_line = 1;
        let selected = if let Some(range) = range {
            start_line = range.start;
            content
                .lines()
                .enumerate()
                .filter_map(|(index, line)| {
                    let line_number = index + 1;
                    (line_number >= range.start && line_number <= range.end)
                        .then(|| format!("{line}\n"))
                })
                .collect()
        } else {
            content
        };

        Ok(FileSlice {
            path,
            start_line,
            content: selected,
        })
    }

    fn search(&self, query: SearchQuery) -> Result<SearchResults> {
        let search_root = query
            .root
            .as_ref()
            .map_or_else(|| Ok(self.root.clone()), |path| self.resolve(path))?;
        let mut matches = Vec::new();
        search_dir(&self.root, &search_root, &query.pattern, &mut matches)?;
        Ok(SearchResults { matches })
    }

    fn apply_patch(&mut self, patch: Patch) -> Result<PatchId> {
        let mut child = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["apply", "--whitespace=nowarn"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(patch.unified_diff.as_bytes())?;
        }

        let output = child.wait_with_output()?;
        if !output.status.success() {
            return Err(PatchwrightError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ));
        }

        Ok(PatchId("local-git-apply".to_owned()))
    }

    fn run(&mut self, command: CommandSpec, policy: &Policy) -> Result<RunReport> {
        if !policy.allows(&command) {
            return Err(PatchwrightError::PolicyDenied(format!(
                "policy denied command: {}",
                command.program
            )));
        }

        let command_for_report = command.clone();
        let cwd = command
            .cwd
            .as_ref()
            .map_or_else(|| Ok(self.root.clone()), |path| self.resolve(path))?;
        let mut child = Command::new(&command.program)
            .args(&command.args)
            .current_dir(cwd)
            .stdin(if command.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(stdin) = command.stdin.as_deref() {
            if let Some(child_stdin) = child.stdin.as_mut() {
                child_stdin.write_all(stdin.as_bytes())?;
            }
        }

        let started = std::time::Instant::now();
        loop {
            if child.try_wait()?.is_some() {
                let output = child.wait_with_output()?;
                return Ok(RunReport {
                    command: command_for_report,
                    status: ExitStatus {
                        code: output.status.code(),
                        success: output.status.success(),
                    },
                    stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                });
            }

            if started.elapsed() >= command.timeout {
                child.kill()?;
                let output = child.wait_with_output()?;
                let mut stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                if !stderr.is_empty() && !stderr.ends_with('\n') {
                    stderr.push('\n');
                }
                stderr.push_str("command timed out");
                return Ok(RunReport {
                    command: command_for_report,
                    status: ExitStatus {
                        code: output.status.code(),
                        success: false,
                    },
                    stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                    stderr,
                });
            }

            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    fn revert(&mut self, snapshot: SnapshotId) -> Result<()> {
        let reset = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["reset", "--hard", &snapshot.0])
            .output()?;
        if !reset.status.success() {
            return Err(PatchwrightError::CommandFailed(
                String::from_utf8_lossy(&reset.stderr).into_owned(),
            ));
        }

        let clean = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["clean", "-fd"])
            .output()?;
        if !clean.status.success() {
            return Err(PatchwrightError::CommandFailed(
                String::from_utf8_lossy(&clean.stderr).into_owned(),
            ));
        }

        Ok(())
    }
}

fn search_dir(
    repo_root: &Path,
    current: &Path,
    pattern: &str,
    matches: &mut Vec<SearchMatch>,
) -> Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        if name.to_string_lossy() == ".git" {
            continue;
        }
        if path.is_dir() {
            search_dir(repo_root, &path, pattern, matches)?;
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        for (index, line) in content.lines().enumerate() {
            if line.contains(pattern) {
                let relative = path
                    .strip_prefix(repo_root)
                    .map_err(|error| PatchwrightError::InvalidInput(error.to_string()))?;
                matches.push(SearchMatch {
                    path: RepoPath::new(relative.to_string_lossy().replace('\\', "/")),
                    line: index + 1,
                    text: line.to_owned(),
                });
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Verify local execution**

Run: `cargo test -p patchwright-exec-local`

Expected: all local execution tests pass.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/patchwright-exec-local
git commit -m "feat(exec-local): add local execution backend"
```

## Task 6: Lightweight Indexer

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/patchwright-index/Cargo.toml`
- Create: `crates/patchwright-index/src/lib.rs`
- Create: `crates/patchwright-index/tests/basic_index.rs`

- [ ] **Step 1: Create manifest and failing tests**

Modify root `Cargo.toml` to add the new crate:

```toml
[workspace]
members = [
    "crates/patchwright-cli",
    "crates/patchwright-core",
    "crates/patchwright-test-support",
    "crates/patchwright-exec-local",
    "crates/patchwright-index",
]
```

Create `crates/patchwright-index/Cargo.toml`:

```toml
[package]
name = "patchwright-index"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true

[lib]
path = "src/lib.rs"

[dependencies]
patchwright-core = { path = "../patchwright-core" }

[dev-dependencies]
patchwright-test-support = { path = "../patchwright-test-support" }

[lints]
workspace = true
```

Create `crates/patchwright-index/tests/basic_index.rs`:

```rust
use patchwright_core::traits::Indexer;
use patchwright_core::types::{FileQuery, SearchQuery};
use patchwright_index::BasicIndexer;
use patchwright_test_support::TempRepo;

#[test]
fn lists_files_and_searches_text() {
    let repo = TempRepo::new("index-basic");
    repo.write("src/lib.rs", "pub fn target() {}\n");
    repo.write("README.md", "Patchwright\n");
    repo.commit_all("seed");

    let indexer = BasicIndexer::new(repo.root());
    let files = indexer.list_files(FileQuery::default()).unwrap();
    assert!(files.iter().any(|file| file.path.0 == "src/lib.rs"));
    assert!(files.iter().any(|file| file.path.0 == "README.md"));

    let results = indexer
        .search_text(SearchQuery {
            pattern: "target".to_owned(),
            root: None,
        })
        .unwrap();
    assert_eq!(results.matches.len(), 1);
}
```

Run: `cargo test -p patchwright-index`

Expected: compilation fails because `BasicIndexer` does not exist.

- [ ] **Step 2: Implement the basic indexer**

Create `crates/patchwright-index/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

use patchwright_core::error::{PatchwrightError, Result};
use patchwright_core::traits::Indexer;
use patchwright_core::types::{
    FileQuery, RepoPath, ScoredPath, SearchMatch, SearchQuery, SearchResults, Symbol,
};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct BasicIndexer {
    root: PathBuf,
}

impl BasicIndexer {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }
}

impl Indexer for BasicIndexer {
    fn list_files(&self, _query: FileQuery) -> Result<Vec<ScoredPath>> {
        let mut files = Vec::new();
        collect_files(&self.root, &self.root, &mut files)?;
        files.sort_by(|a, b| a.path.0.cmp(&b.path.0));
        Ok(files)
    }

    fn search_text(&self, query: SearchQuery) -> Result<SearchResults> {
        let mut matches = Vec::new();
        search_files(&self.root, &self.root, &query.pattern, &mut matches)?;
        Ok(SearchResults { matches })
    }

    fn symbols(&self, _path: &RepoPath) -> Result<Vec<Symbol>> {
        Ok(Vec::new())
    }
}

fn collect_files(root: &Path, current: &Path, files: &mut Vec<ScoredPath>) -> Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_name().to_string_lossy() == ".git" {
            continue;
        }
        if path.is_dir() {
            collect_files(root, &path, files)?;
            continue;
        }
        let relative = relative_path(root, &path)?;
        files.push(ScoredPath {
            path: relative,
            score: 1,
        });
    }
    Ok(())
}

fn search_files(
    root: &Path,
    current: &Path,
    pattern: &str,
    matches: &mut Vec<SearchMatch>,
) -> Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_name().to_string_lossy() == ".git" {
            continue;
        }
        if path.is_dir() {
            search_files(root, &path, pattern, matches)?;
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        for (index, line) in content.lines().enumerate() {
            if line.contains(pattern) {
                matches.push(SearchMatch {
                    path: relative_path(root, &path)?,
                    line: index + 1,
                    text: line.to_owned(),
                });
            }
        }
    }
    Ok(())
}

fn relative_path(root: &Path, path: &Path) -> Result<RepoPath> {
    let relative = path
        .strip_prefix(root)
        .map_err(|error| PatchwrightError::InvalidInput(error.to_string()))?;
    Ok(RepoPath::new(relative.to_string_lossy().replace('\\', "/")))
}
```

- [ ] **Step 3: Verify indexer**

Run: `cargo test -p patchwright-index`

Expected: all indexer tests pass.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/patchwright-index
git commit -m "feat(index): add basic repository indexer"
```

## Task 7: Rust Language Adapter

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/patchwright-lang-rust/Cargo.toml`
- Create: `crates/patchwright-lang-rust/src/lib.rs`
- Create: `crates/patchwright-lang-rust/tests/rust_adapter.rs`

- [ ] **Step 1: Create manifest and failing tests**

Modify root `Cargo.toml` to add the new crate:

```toml
[workspace]
members = [
    "crates/patchwright-cli",
    "crates/patchwright-core",
    "crates/patchwright-test-support",
    "crates/patchwright-exec-local",
    "crates/patchwright-index",
    "crates/patchwright-lang-rust",
]
```

Create `crates/patchwright-lang-rust/Cargo.toml`:

```toml
[package]
name = "patchwright-lang-rust"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true

[lib]
path = "src/lib.rs"

[dependencies]
patchwright-core = { path = "../patchwright-core" }

[dev-dependencies]
patchwright-index = { path = "../patchwright-index" }
patchwright-test-support = { path = "../patchwright-test-support" }

[lints]
workspace = true
```

Create `crates/patchwright-lang-rust/tests/rust_adapter.rs`:

```rust
use patchwright_core::traits::LanguageAdapter;
use patchwright_core::types::{RepoView, TaskSpec};
use patchwright_lang_rust::RustAdapter;
use patchwright_test_support::TempRepo;
use std::path::PathBuf;

#[test]
fn detects_cargo_projects_and_builds_verifier_plan() {
    let repo = TempRepo::new("lang-rust");
    repo.write(
        "Cargo.toml",
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    repo.write("src/lib.rs", "pub fn answer() -> u8 { 42 }\n");
    repo.commit_all("seed");

    let adapter = RustAdapter::default();
    let view = RepoView {
        root: repo.root().to_path_buf(),
    };
    assert_eq!(adapter.detect(&view).0, 100);

    let plan = adapter.verifier_plan(
        &TaskSpec::from_text(PathBuf::from(repo.root()), "verify fixture"),
        &view,
    );
    let commands: Vec<String> = plan.commands.iter().map(|cmd| cmd.args.join(" ")).collect();
    assert!(commands.contains(&"fmt -- --check".to_owned()));
    assert!(commands.contains(&"check".to_owned()));
    assert!(commands.contains(&"test".to_owned()));
}
```

Run: `cargo test -p patchwright-lang-rust`

Expected: compilation fails because `RustAdapter` does not exist.

- [ ] **Step 2: Implement Rust detection and verifier planning**

Create `crates/patchwright-lang-rust/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

use patchwright_core::traits::{Indexer, LanguageAdapter};
use patchwright_core::types::{
    CommandSpec, Counterexample, DetectionScore, RepoView, RunReport, ScoredPath, TaskSpec,
    VerifierPlan,
};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct RustAdapter {
    pub fmt: bool,
    pub check: bool,
    pub test: bool,
    pub clippy: bool,
}

impl Default for RustAdapter {
    fn default() -> Self {
        Self {
            fmt: true,
            check: true,
            test: true,
            clippy: false,
        }
    }
}

impl LanguageAdapter for RustAdapter {
    fn detect(&self, repo: &RepoView) -> DetectionScore {
        if repo.root.join("Cargo.toml").exists() {
            DetectionScore(100)
        } else {
            DetectionScore(0)
        }
    }

    fn verifier_plan(&self, _task: &TaskSpec, _repo: &RepoView) -> VerifierPlan {
        let mut commands = Vec::new();
        if self.fmt {
            commands.push(cargo(["fmt", "--check"]));
        }
        if self.check {
            commands.push(cargo(["check"]));
        }
        if self.test {
            commands.push(cargo(["test"]));
        }
        if self.clippy {
            commands.push(cargo(["clippy", "--", "-D", "warnings"]));
        }
        VerifierPlan { commands }
    }

    fn relevant_files(&self, task: &TaskSpec, index: &dyn Indexer) -> Vec<ScoredPath> {
        index
            .list_files(Default::default())
            .unwrap_or_default()
            .into_iter()
            .map(|mut file| {
                if task.text.contains("test") && file.path.0.contains("test") {
                    file.score += 10;
                }
                if file.path.0.ends_with(".rs") {
                    file.score += 5;
                }
                file
            })
            .collect()
    }

    fn summarize_failure(&self, report: &RunReport) -> Vec<Counterexample> {
        if report.status.success {
            return Vec::new();
        }
        let detail = if report.stderr.trim().is_empty() {
            report.stdout.lines().take(20).collect::<Vec<_>>().join("\n")
        } else {
            report.stderr.lines().take(20).collect::<Vec<_>>().join("\n")
        };
        vec![Counterexample {
            source: report.command.program.clone(),
            detail,
        }]
    }
}

fn cargo<const N: usize>(args: [&str; N]) -> CommandSpec {
    let mut command = CommandSpec::new("cargo", args);
    command.timeout = Duration::from_secs(300);
    command
}
```

- [ ] **Step 3: Verify Rust adapter**

Run: `cargo test -p patchwright-lang-rust`

Expected: all Rust adapter tests pass.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/patchwright-lang-rust
git commit -m "feat(lang-rust): add cargo verifier planning"
```

## Task 8: Verifier Runner

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/patchwright-verify/Cargo.toml`
- Create: `crates/patchwright-verify/src/lib.rs`
- Create: `crates/patchwright-verify/tests/verifier.rs`

- [ ] **Step 1: Create manifest and failing tests**

Modify root `Cargo.toml` to add the new crate:

```toml
[workspace]
members = [
    "crates/patchwright-cli",
    "crates/patchwright-core",
    "crates/patchwright-test-support",
    "crates/patchwright-exec-local",
    "crates/patchwright-index",
    "crates/patchwright-lang-rust",
    "crates/patchwright-verify",
]
```

Create `crates/patchwright-verify/Cargo.toml`:

```toml
[package]
name = "patchwright-verify"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true

[lib]
path = "src/lib.rs"

[dependencies]
patchwright-core = { path = "../patchwright-core" }

[lints]
workspace = true
```

Create `crates/patchwright-verify/tests/verifier.rs`:

```rust
use patchwright_core::policy::Policy;
use patchwright_core::traits::{ExecutionBackend, Verifier};
use patchwright_core::types::{
    CommandSpec, ExitStatus, FileSlice, LineRange, Patch, PatchId, RepoPath, RunReport,
    SearchQuery, SearchResults, SnapshotId, VerificationStatus, VerifierPlan,
};
use patchwright_verify::PlanVerifier;

#[derive(Default)]
struct FakeExecution;

impl ExecutionBackend for FakeExecution {
    fn snapshot(&mut self) -> patchwright_core::Result<SnapshotId> {
        Ok(SnapshotId("snap".to_owned()))
    }

    fn read_file(
        &self,
        path: RepoPath,
        _range: Option<LineRange>,
    ) -> patchwright_core::Result<FileSlice> {
        Ok(FileSlice {
            path,
            start_line: 1,
            content: String::new(),
        })
    }

    fn search(&self, _query: SearchQuery) -> patchwright_core::Result<SearchResults> {
        Ok(SearchResults { matches: Vec::new() })
    }

    fn apply_patch(&mut self, _patch: Patch) -> patchwright_core::Result<PatchId> {
        Ok(PatchId("patch".to_owned()))
    }

    fn run(
        &mut self,
        command: CommandSpec,
        _policy: &Policy,
    ) -> patchwright_core::Result<RunReport> {
        let failed = command.args.iter().any(|arg| arg == "fail");
        Ok(RunReport {
            command,
            status: ExitStatus {
                code: Some(if failed { 1 } else { 0 }),
                success: !failed,
            },
            stdout: String::new(),
            stderr: if failed { "failed command".to_owned() } else { String::new() },
        })
    }

    fn revert(&mut self, _snapshot: SnapshotId) -> patchwright_core::Result<()> {
        Ok(())
    }
}

#[test]
fn rejects_when_any_check_fails() {
    let mut verifier = PlanVerifier;
    let mut execution = FakeExecution;
    let report = verifier
        .verify(
            &mut execution,
            &VerifierPlan {
                commands: vec![CommandSpec::new("cargo", ["fail"])],
            },
            &Policy::FullShellAutonomous,
        )
        .unwrap();

    assert_eq!(report.status, VerificationStatus::Rejected);
    assert_eq!(report.counterexamples.len(), 1);
}
```

Run: `cargo test -p patchwright-verify`

Expected: compilation fails because `PlanVerifier` does not exist.

- [ ] **Step 2: Implement verifier**

Create `crates/patchwright-verify/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

use patchwright_core::error::Result;
use patchwright_core::policy::Policy;
use patchwright_core::traits::{ExecutionBackend, Verifier};
use patchwright_core::types::{
    CheckReport, Counterexample, DiffSummary, PolicyEvent, VerificationReport,
    VerificationStatus, VerifierPlan,
};

#[derive(Debug, Clone, Copy)]
pub struct PlanVerifier;

impl Verifier for PlanVerifier {
    fn verify(
        &mut self,
        execution: &mut dyn ExecutionBackend,
        plan: &VerifierPlan,
        policy: &Policy,
    ) -> Result<VerificationReport> {
        let mut checks = Vec::new();
        let mut counterexamples = Vec::new();
        let mut policy_events = Vec::new();

        for command in &plan.commands {
            let allowed = policy.allows(command);
            policy_events.push(PolicyEvent {
                command: Some(command.clone()),
                allowed,
                reason: if allowed {
                    "allowed".to_owned()
                } else {
                    "denied".to_owned()
                },
            });

            let report = execution.run(command.clone(), policy)?;
            let summary = if report.status.success {
                "passed".to_owned()
            } else if report.stderr.trim().is_empty() {
                report.stdout.lines().take(20).collect::<Vec<_>>().join("\n")
            } else {
                report.stderr.lines().take(20).collect::<Vec<_>>().join("\n")
            };

            if !report.status.success {
                counterexamples.push(Counterexample {
                    source: command.program.clone(),
                    detail: summary.clone(),
                });
            }

            checks.push(CheckReport {
                name: format!("{} {}", command.program, command.args.join(" ")),
                command: Some(command.clone()),
                passed: report.status.success,
                summary,
            });
        }

        let accepted = checks.iter().all(|check| check.passed);
        Ok(VerificationReport {
            status: if accepted {
                VerificationStatus::Accepted
            } else {
                VerificationStatus::Rejected
            },
            checks,
            counterexamples,
            diff_summary: DiffSummary::default(),
            policy_events,
        })
    }
}
```

- [ ] **Step 3: Verify the verifier**

Run: `cargo test -p patchwright-verify`

Expected: verifier tests pass.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/patchwright-verify
git commit -m "feat(verify): add verifier runner"
```

## Task 9: OpenAI-Compatible Model Adapter

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/patchwright-model-openai/Cargo.toml`
- Create: `crates/patchwright-model-openai/src/lib.rs`
- Create: `crates/patchwright-model-openai/tests/openai_client.rs`

- [ ] **Step 1: Create manifest and request-building test**

Modify root `Cargo.toml` to add the new crate:

```toml
[workspace]
members = [
    "crates/patchwright-cli",
    "crates/patchwright-core",
    "crates/patchwright-test-support",
    "crates/patchwright-exec-local",
    "crates/patchwright-index",
    "crates/patchwright-lang-rust",
    "crates/patchwright-verify",
    "crates/patchwright-model-openai",
]
```

Create `crates/patchwright-model-openai/Cargo.toml`:

```toml
[package]
name = "patchwright-model-openai"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true

[lib]
path = "src/lib.rs"

[dependencies]
patchwright-core = { path = "../patchwright-core" }
serde_json = "1"
ureq = { version = "2", features = ["json"] }

[lints]
workspace = true
```

Create `crates/patchwright-model-openai/tests/openai_client.rs`:

```rust
use patchwright_core::action::Action;
use patchwright_core::traits::ModelProvider;
use patchwright_core::types::{ModelRequest, TaskSpec};
use patchwright_model_openai::{parse_action_json, OpenAiCompatibleClient, OpenAiConfig};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::thread;

#[test]
fn dry_run_client_returns_finish_action_without_network() {
    let mut client = OpenAiCompatibleClient::dry_run(OpenAiConfig {
        base_url: "https://api.openai.com/v1".to_owned(),
        model: "gpt-test".to_owned(),
        api_key_env: "OPENAI_API_KEY".to_owned(),
        timeout_seconds: 30,
    });

    let response = client
        .propose_action(ModelRequest {
            task: TaskSpec::from_text(PathBuf::from("."), "summarize"),
            observations: Vec::new(),
            counterexamples: Vec::new(),
        })
        .unwrap();

    assert!(matches!(response.action, Action::Finish { .. }));
}

#[test]
fn parses_finish_action_json() {
    let action = parse_action_json(r#"{"action":"finish","summary":"done"}"#).unwrap();
    assert_eq!(
        action,
        Action::Finish {
            summary: "done".to_owned()
        }
    );
}

#[test]
fn live_mode_uses_openai_compatible_chat_completion_shape() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let read = stream.read(&mut request).unwrap();
        let request_text = String::from_utf8_lossy(&request[..read]);
        let request_lower = request_text.to_ascii_lowercase();
        assert!(request_lower.contains("post /chat/completions http/1.1"));
        assert!(request_lower.contains("authorization: bearer test-key"));
        assert!(request_text.contains("\"model\":\"gpt-test\""));

        let body = r#"{"choices":[{"message":{"content":"{\"action\":\"finish\",\"summary\":\"from http\"}"}}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).unwrap();
    });

    std::env::set_var("PATCHWRIGHT_TEST_API_KEY", "test-key");
    let mut client = OpenAiCompatibleClient::new(OpenAiConfig {
        base_url: format!("http://{addr}"),
        model: "gpt-test".to_owned(),
        api_key_env: "PATCHWRIGHT_TEST_API_KEY".to_owned(),
        timeout_seconds: 5,
    });

    let response = client
        .propose_action(ModelRequest {
            task: TaskSpec::from_text(PathBuf::from("."), "summarize"),
            observations: Vec::new(),
            counterexamples: Vec::new(),
        })
        .unwrap();

    assert_eq!(
        response.action,
        Action::Finish {
            summary: "from http".to_owned()
        }
    );
    server.join().unwrap();
}
```

Run: `cargo test -p patchwright-model-openai`

Expected: compilation fails because adapter types do not exist.

- [ ] **Step 2: Implement an offline-safe adapter boundary**

Create `crates/patchwright-model-openai/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

use patchwright_core::action::Action;
use patchwright_core::error::{PatchwrightError, Result};
use patchwright_core::traits::ModelProvider;
use patchwright_core::types::{ModelRequest, ModelResponse};
use serde_json::Value;
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
        match self.mode {
            Mode::DryRun => Ok(ModelResponse {
                action: Action::Finish {
                    summary: format!(
                        "dry-run model {} received task: {}",
                        self.config.model, request.task.text
                    ),
                },
            }),
            Mode::Http => self.propose_action_http(request),
        }
    }
}

impl OpenAiCompatibleClient {
    fn propose_action_http(&self, request: ModelRequest) -> Result<ModelResponse> {
        let api_key = std::env::var(&self.config.api_key_env).map_err(|_| {
            PatchwrightError::Model(format!(
                "missing API key environment variable {}",
                self.config.api_key_env
            ))
        })?;
        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );
        let authorization = format!("Bearer {api_key}");
        let body = serde_json::json!({
            "model": self.config.model,
            "temperature": 0,
            "messages": [
                {
                    "role": "system",
                    "content": "You are Patchwright's action proposer. Return only JSON: {\"action\":\"finish\",\"summary\":\"...\"} for this foundation build."
                },
                {
                    "role": "user",
                    "content": format!(
                        "Task: {}\nObservations: {}\nCounterexamples: {}",
                        request.task.text,
                        request.observations.len(),
                        request.counterexamples.len()
                    )
                }
            ]
        });

        let response: Value = ureq::post(&url)
            .set("authorization", &authorization)
            .set("content-type", "application/json")
            .timeout(Duration::from_secs(self.config.timeout_seconds))
            .send_json(body)
            .map_err(|error| PatchwrightError::Model(error.to_string()))?
            .into_json()
            .map_err(|error| PatchwrightError::Model(error.to_string()))?;

        let content = response
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                PatchwrightError::Model("OpenAI-compatible response did not contain message content".to_owned())
            })?;

        Ok(ModelResponse {
            action: parse_action_json(content)?,
        })
    }
}

pub fn parse_action_json(content: &str) -> Result<Action> {
    let value: Value =
        serde_json::from_str(content).map_err(|error| PatchwrightError::Model(error.to_string()))?;
    match value.get("action").and_then(Value::as_str) {
        Some("finish") => Ok(Action::Finish {
            summary: value
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or("finished")
                .to_owned(),
        }),
        Some(other) => Err(PatchwrightError::Model(format!(
            "unsupported model action: {other}"
        ))),
        None => Err(PatchwrightError::Model(
            "model action JSON missing action field".to_owned(),
        )),
    }
}
```

- [ ] **Step 3: Verify the adapter boundary**

Run: `cargo test -p patchwright-model-openai`

Expected: dry-run, parser, and local mocked HTTP tests pass without external network access.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/patchwright-model-openai
git commit -m "feat(model-openai): add openai-compatible client boundary"
```

## Task 10: CLI Wiring for Verify and Solve MVP

**Files:**
- Modify: `crates/patchwright-cli/Cargo.toml`
- Modify: `crates/patchwright-cli/src/main.rs`

- [ ] **Step 1: Add adapter dependencies**

Modify `crates/patchwright-cli/Cargo.toml` dependencies:

```toml
[dependencies]
patchwright-core = { path = "../patchwright-core" }
patchwright-exec-local = { path = "../patchwright-exec-local" }
patchwright-index = { path = "../patchwright-index" }
patchwright-lang-rust = { path = "../patchwright-lang-rust" }
patchwright-model-openai = { path = "../patchwright-model-openai" }
patchwright-verify = { path = "../patchwright-verify" }
```

- [ ] **Step 2: Add CLI tests for parse behavior**

Append to `crates/patchwright-cli/src/main.rs` test module:

```rust
#[test]
fn solve_requires_repo_and_task() {
    let result = run(["solve".to_owned()]);
    assert_eq!(
        result,
        Err("solve requires --repo <path> and --task <text>".to_owned())
    );
}

#[test]
fn verify_requires_repo() {
    let result = run(["verify".to_owned()]);
    assert_eq!(result, Err("verify requires --repo <path>".to_owned()));
}
```

Run: `cargo test -p patchwright-cli`

Expected: tests fail because current errors are still generic.

- [ ] **Step 3: Replace solve and verify routing**

Modify `crates/patchwright-cli/src/main.rs` imports:

```rust
use patchwright_core::agent::{Agent, SolveStatus};
use patchwright_core::policy::Policy;
use patchwright_core::traits::LanguageAdapter;
use patchwright_core::types::{RepoView, TaskSpec};
use patchwright_exec_local::LocalExecution;
use patchwright_index::BasicIndexer;
use patchwright_lang_rust::RustAdapter;
use patchwright_model_openai::{OpenAiCompatibleClient, OpenAiConfig};
use patchwright_verify::PlanVerifier;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
```

Replace the `solve` and `verify` match arms:

```rust
"solve" => run_solve(&args),
"verify" => run_verify(&args),
```

Append helper functions before the test module:

```rust
fn run_solve(args: &[String]) -> Result<(), String> {
    let repo = value_after(args, "--repo")
        .ok_or_else(|| "solve requires --repo <path> and --task <text>".to_owned())?;
    let task = value_after(args, "--task")
        .ok_or_else(|| "solve requires --repo <path> and --task <text>".to_owned())?;

    let repo_path = PathBuf::from(repo);
    let model = OpenAiCompatibleClient::dry_run(OpenAiConfig {
        base_url: "https://api.openai.com/v1".to_owned(),
        model: "dry-run".to_owned(),
        api_key_env: "OPENAI_API_KEY".to_owned(),
        timeout_seconds: 30,
    });
    let execution = LocalExecution::new(&repo_path);
    let language_adapter = RustAdapter::default();
    let indexer = BasicIndexer::new(&repo_path);
    let verifier = PlanVerifier;

    let mut agent = Agent::builder()
        .model(model)
        .execution(execution)
        .language_adapter(language_adapter)
        .indexer(indexer)
        .verifier(verifier)
        .policy(Policy::ProjectConfiguredCommands {
            allowed_programs: vec!["cargo".to_owned()],
        })
        .max_steps(3)
        .build();

    let report = agent
        .solve(TaskSpec::from_text(repo_path, task))
        .map_err(|error| error.to_string())?;

    println!("solve status: {:?}", report.status);
    if report.status != SolveStatus::Accepted {
        println!("{}", report.summary);
    }
    Ok(())
}

fn run_verify(args: &[String]) -> Result<(), String> {
    let repo = value_after(args, "--repo").ok_or_else(|| "verify requires --repo <path>".to_owned())?;
    let repo_path = PathBuf::from(repo);
    let adapter = RustAdapter::default();
    let view = RepoView {
        root: repo_path.clone(),
    };
    if adapter.detect(&view).0 == 0 {
        return Err("no supported language adapter detected".to_owned());
    }
    let plan = adapter.verifier_plan(
        &TaskSpec::from_text(repo_path.clone(), "verify repository"),
        &view,
    );
    println!("verification plan:");
    for command in plan.commands {
        println!("  {} {}", command.program, command.args.join(" "));
    }
    Ok(())
}

fn value_after(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find_map(|window| (window[0] == flag).then(|| window[1].clone()))
}
```

- [ ] **Step 4: Verify CLI wiring**

Run: `cargo test -p patchwright-cli`

Expected: CLI tests pass.

Run: `cargo run -p patchwright-cli -- verify --repo .`

Expected: if the current repo is not a Rust workspace yet, command prints `no supported language adapter detected` and exits non-zero. After Task 1's workspace exists, command prints and runs Cargo commands.

Run: `cargo run -p patchwright-cli -- solve --repo . --task "summarize"`

Expected: dry-run model finishes without network.

- [ ] **Step 5: Commit**

```bash
git add crates/patchwright-cli
git commit -m "feat(cli): wire verify and solve mvp"
```

## Task 11: Startup Benchmark Command and CI

**Files:**
- Modify: `crates/patchwright-cli/src/main.rs`
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Add a real startup benchmark command**

Replace the `bench startup` match body in `crates/patchwright-cli/src/main.rs`:

```rust
"bench" if args.get(1).map(String::as_str) == Some("startup") => run_startup_bench(),
```

Append:

```rust
fn run_startup_bench() -> Result<(), String> {
    let exe = env::current_exe().map_err(|error| error.to_string())?;
    let iterations = 20;
    let mut total_nanos = 0u128;

    for _ in 0..iterations {
        let start = std::time::Instant::now();
        let output = std::process::Command::new(&exe)
            .arg("--version")
            .output()
            .map_err(|error| error.to_string())?;
        if !output.status.success() {
            return Err("startup benchmark child command failed".to_owned());
        }
        total_nanos += start.elapsed().as_nanos();
    }

    let average_micros = total_nanos / iterations;
    println!("startup_version_average_micros={average_micros}");
    Ok(())
}
```

- [ ] **Step 2: Add CI workflow**

Create `.github/workflows/ci.yml`:

```yaml
name: ci

on:
  pull_request:
  push:
    branches:
      - main

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Format
        run: cargo fmt --all -- --check
      - name: Clippy
        run: cargo clippy --workspace --all-targets -- -D warnings
      - name: Test
        run: cargo test --workspace
      - name: Startup smoke benchmark
        run: cargo run -p patchwright-cli -- bench startup
```

- [ ] **Step 3: Verify formatting, linting, tests, and startup smoke**

Run: `cargo fmt --all -- --check`

Expected: formatting check passes.

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: clippy passes.

Run: `cargo test --workspace`

Expected: all tests pass.

Run: `cargo run -p patchwright-cli -- bench startup`

Expected: prints `startup_version_average_micros=<number>`.

- [ ] **Step 4: Commit**

```bash
git add crates/patchwright-cli .github/workflows/ci.yml
git commit -m "perf(cli): add startup benchmark smoke test"
```

## Task 12: Final Documentation and Verification

**Files:**
- Modify: `README.md`
- Modify: `CONTRIBUTING.md` only if commands in this plan change the workflow

- [ ] **Step 1: Update README with initial commands**

Modify `README.md`:

````markdown
# Patchwright

Patchwright is a local-first Rust foundation for a software-engineering coding agent. The model proposes candidate actions and patches; compilers, tests, linters, type checkers, benchmarks, policies, and reviewers decide what is true.

The first build focuses on a language-agnostic core, a Rust adapter, local execution, OpenAI-compatible model access, and strict verification.

## Development

Run the full local verification suite:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p patchwright-cli -- bench startup
```

Run basic commands:

```bash
cargo run -p patchwright-cli -- --version
cargo run -p patchwright-cli -- status
cargo run -p patchwright-cli -- config check
cargo run -p patchwright-cli -- verify --repo .
cargo run -p patchwright-cli -- solve --repo . --task "summarize"
```
````

- [ ] **Step 2: Run final verification**

Run: `cargo fmt --all -- --check`

Expected: formatting check passes.

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: clippy passes.

Run: `cargo test --workspace`

Expected: all tests pass.

Run: `cargo run -p patchwright-cli -- --version`

Expected: output starts with `patchwright 0.1.0`.

Run: `cargo run -p patchwright-cli -- bench startup`

Expected: prints `startup_version_average_micros=<number>`.

- [ ] **Step 3: Inspect the final diff**

Run: `git status --short`

Expected: only intended files are modified or added.

Run: `git diff --stat`

Expected: changes are scoped to the Rust workspace, docs, and CI.

- [ ] **Step 4: Commit**

```bash
git add README.md CONTRIBUTING.md Cargo.toml crates .github
git commit -m "docs(readme): document foundation commands"
```

## Self-Review Checklist

- [ ] Workspace layout from the architecture spec is represented.
- [ ] Core owns domain types, policy, trait contracts, and agent loop.
- [ ] CLI starts with lightweight routes before adapter construction.
- [ ] Local execution supports read, search, apply patch, run command, snapshot, and revert.
- [ ] Rust adapter detects Cargo projects and creates verifier plans.
- [ ] Agent loop has an offline simulation test.
- [ ] Model adapter is OpenAI-compatible in shape and offline-safe in default tests.
- [ ] Verification is structured and produces counterexamples on failure.
- [ ] Startup benchmark command exists and runs through CI.
- [ ] Normal test suite performs no network calls.
- [ ] Commit messages follow `CONTRIBUTING.md`.
