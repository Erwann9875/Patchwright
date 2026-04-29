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
