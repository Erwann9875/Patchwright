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
