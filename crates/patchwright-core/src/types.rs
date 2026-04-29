use crate::action::Observation;
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
        Self::from_text(repo_path, text).with_require_patch(true)
    }

    pub fn with_require_patch(mut self, require_patch: bool) -> Self {
        self.require_patch = require_patch;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskMode {
    Design,
    Plan,
    Implement,
    Review,
    Verify,
    InfoOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchitectureDesign {
    pub title: String,
    pub goal: String,
    pub current_architecture: Vec<ArchitectureFinding>,
    pub assumptions: Vec<String>,
    pub non_goals: Vec<String>,
    pub options: Vec<DesignOption>,
    pub recommendation: RecommendedDesign,
    pub file_impact: Vec<FileImpact>,
    pub implementation_plan: Vec<PlanStep>,
    pub test_strategy: TestStrategy,
    pub migration_plan: Option<String>,
    pub rollback_plan: Option<String>,
    pub risks: Vec<Risk>,
    pub open_questions: Vec<String>,
    pub acceptance_criteria: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchitectureFinding {
    pub summary: String,
    pub evidence: Vec<EvidenceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignOption {
    pub name: String,
    pub summary: String,
    pub pros: Vec<String>,
    pub cons: Vec<String>,
    pub evidence: Vec<EvidenceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecommendedDesign {
    pub option_name: String,
    pub rationale: String,
    pub evidence: Vec<EvidenceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileImpact {
    pub path: RepoPath,
    pub change_summary: String,
    pub risk: Option<String>,
    pub evidence: Vec<EvidenceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanStep {
    pub id: String,
    pub title: String,
    pub description: String,
    pub depends_on: Vec<String>,
    pub target_files: Vec<RepoPath>,
    pub acceptance_criteria: Vec<String>,
    pub verification_commands: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestStrategy {
    pub unit: Vec<String>,
    pub integration: Vec<String>,
    pub end_to_end: Vec<String>,
    pub manual: Vec<String>,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Risk {
    pub title: String,
    pub impact: String,
    pub mitigation: String,
    pub evidence: Vec<EvidenceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceRef {
    pub path: RepoPath,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
    pub reason: String,
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContextPack {
    pub files: Vec<ScoredPath>,
    pub likely_tests: Vec<RepoPath>,
    pub manifests: Vec<RepoPath>,
    pub recent_observations: Vec<Observation>,
    pub counterexamples: Vec<Counterexample>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRequest {
    pub task: TaskSpec,
    pub observations: Vec<Observation>,
    pub counterexamples: Vec<Counterexample>,
    pub context: Option<ContextPack>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelResponse {
    pub action: crate::action::Action,
}
