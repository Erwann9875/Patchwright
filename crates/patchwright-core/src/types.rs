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
    pub fn new(
        program: impl Into<String>,
        args: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
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
