use patchwright_core::action::{Action, Observation};
use patchwright_core::agent::{Agent, SolveStatus};
use patchwright_core::error::{PatchwrightError, Result};
use patchwright_core::policy::Policy;
use patchwright_core::traits::{
    ExecutionBackend, Indexer, LanguageAdapter, ModelProvider, Verifier,
};
use patchwright_core::types::{
    CommandSpec, ContextPack, Counterexample, DetectionScore, DiffSummary, FileQuery, FileSlice,
    LineRange, ModelRequest, ModelResponse, Patch, PatchId, PolicyEvent, RepoPath, RepoView,
    RunReport, ScoredPath, SearchQuery, SearchResults, SnapshotId, TaskSpec, VerificationReport,
    VerificationStatus, VerifierPlan,
};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

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

struct RecordingModel {
    requests: Rc<RefCell<Vec<ModelRequest>>>,
    action: Action,
}

impl ModelProvider for RecordingModel {
    fn propose_action(&mut self, request: ModelRequest) -> Result<ModelResponse> {
        self.requests.borrow_mut().push(request);
        Ok(ModelResponse {
            action: self.action.clone(),
        })
    }
}

#[derive(Default)]
struct FakeExecution {
    reverted: Rc<Cell<usize>>,
    fail_apply_patch: bool,
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
        Ok(SearchResults {
            matches: Vec::new(),
        })
    }

    fn apply_patch(&mut self, _patch: Patch) -> Result<PatchId> {
        if self.fail_apply_patch {
            return Err(PatchwrightError::CommandFailed("apply failed".to_owned()));
        }
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

    fn diff_summary(&self) -> Result<DiffSummary> {
        Ok(DiffSummary::default())
    }

    fn revert(&mut self, _snapshot: SnapshotId) -> Result<()> {
        self.reverted.set(self.reverted.get() + 1);
        Ok(())
    }
}

struct FakeLanguage;

impl LanguageAdapter for FakeLanguage {
    fn detect(&self, _repo: &RepoView) -> DetectionScore {
        DetectionScore(100)
    }

    fn verifier_plan(&self, _task: &TaskSpec, _repo: &RepoView) -> VerifierPlan {
        VerifierPlan {
            commands: Vec::new(),
        }
    }

    fn relevant_files(&self, _task: &TaskSpec, _index: &dyn Indexer) -> Vec<ScoredPath> {
        Vec::new()
    }

    fn summarize_failure(
        &self,
        _report: &RunReport,
    ) -> Vec<patchwright_core::types::Counterexample> {
        Vec::new()
    }
}

struct EmptyIndex;

impl Indexer for EmptyIndex {
    fn list_files(&self, _query: FileQuery) -> Result<Vec<ScoredPath>> {
        Ok(Vec::new())
    }

    fn search_text(&self, _query: SearchQuery) -> Result<SearchResults> {
        Ok(SearchResults {
            matches: Vec::new(),
        })
    }

    fn symbols(&self, _path: &RepoPath) -> Result<Vec<patchwright_core::types::Symbol>> {
        Ok(Vec::new())
    }
}

struct ContextIndex;

impl Indexer for ContextIndex {
    fn list_files(&self, _query: FileQuery) -> Result<Vec<ScoredPath>> {
        Ok(vec![ScoredPath {
            path: RepoPath::new("src/lib.rs"),
            score: 7,
        }])
    }

    fn search_text(&self, _query: SearchQuery) -> Result<SearchResults> {
        Ok(SearchResults {
            matches: Vec::new(),
        })
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

struct ForbiddenDiffVerifier;

impl Verifier for ForbiddenDiffVerifier {
    fn verify(
        &mut self,
        _execution: &mut dyn ExecutionBackend,
        _plan: &VerifierPlan,
        _policy: &Policy,
    ) -> Result<VerificationReport> {
        Ok(VerificationReport {
            status: VerificationStatus::Accepted,
            checks: Vec::new(),
            counterexamples: Vec::new(),
            diff_summary: DiffSummary {
                changed_files: vec![RepoPath::new(".env")],
                inserted_lines: 1,
                deleted_lines: 0,
            },
            policy_events: Vec::new(),
        })
    }
}

struct RejectingVerifier;

impl Verifier for RejectingVerifier {
    fn verify(
        &mut self,
        _execution: &mut dyn ExecutionBackend,
        _plan: &VerifierPlan,
        _policy: &Policy,
    ) -> Result<VerificationReport> {
        Ok(VerificationReport {
            status: VerificationStatus::Rejected,
            checks: Vec::new(),
            counterexamples: vec![Counterexample {
                source: "test".to_owned(),
                detail: "still fails".to_owned(),
            }],
            diff_summary: DiffSummary::default(),
            policy_events: Vec::<PolicyEvent>::new(),
        })
    }
}

struct FailingVerifier;

impl Verifier for FailingVerifier {
    fn verify(
        &mut self,
        _execution: &mut dyn ExecutionBackend,
        _plan: &VerifierPlan,
        _policy: &Policy,
    ) -> Result<VerificationReport> {
        Err(PatchwrightError::Verification(
            "verifier crashed".to_owned(),
        ))
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
        .try_build()
        .expect("agent should build");

    let report = agent
        .solve(TaskSpec::code_change(PathBuf::from("."), "change code"))
        .expect("simulation should solve");

    assert_eq!(report.status, SolveStatus::Accepted);
    assert_eq!(report.attempts.len(), 1);
    assert!(matches!(
        report.observations.last(),
        Some(Observation::VerificationCompleted(_))
    ));
}

#[test]
fn rejected_verification_reverts_and_stores_counterexamples() {
    let reverted = Rc::new(Cell::new(0));
    let model = ScriptedModel {
        actions: vec![
            Action::ApplyPatch(Patch {
                unified_diff: "diff --git a/a b/a\n".to_owned(),
            }),
            Action::Finish {
                summary: "done".to_owned(),
            },
        ],
    };

    let mut agent = Agent::builder()
        .model(model)
        .execution(FakeExecution {
            reverted: Rc::clone(&reverted),
            fail_apply_patch: false,
        })
        .language_adapter(FakeLanguage)
        .indexer(EmptyIndex)
        .verifier(RejectingVerifier)
        .policy(Policy::SafeStructuredOnly)
        .max_steps(3)
        .try_build()
        .expect("agent should build");

    let report = agent
        .solve(TaskSpec::from_text(PathBuf::from("."), "change code"))
        .expect("simulation should finish after rejected patch");

    assert_eq!(reverted.get(), 1);
    assert_eq!(report.attempts.len(), 1);
    assert_eq!(report.counterexamples.len(), 1);
    assert!(matches!(
        report.observations.last(),
        Some(Observation::Finished(_))
    ));
}

#[test]
fn accepted_verification_with_forbidden_diff_is_rejected_and_reverted() {
    let reverted = Rc::new(Cell::new(0));
    let model = ScriptedModel {
        actions: vec![Action::ApplyPatch(Patch {
            unified_diff: "diff --git a/.env b/.env\n".to_owned(),
        })],
    };

    let mut agent = Agent::builder()
        .model(model)
        .execution(FakeExecution {
            reverted: Rc::clone(&reverted),
            fail_apply_patch: false,
        })
        .language_adapter(FakeLanguage)
        .indexer(EmptyIndex)
        .verifier(ForbiddenDiffVerifier)
        .policy(Policy::FullShellAutonomous)
        .max_steps(1)
        .try_build()
        .expect("agent should build");

    let report = agent
        .solve(TaskSpec::from_text(PathBuf::from("."), "change code"))
        .expect("simulation should return rejected attempt");

    assert_eq!(report.status, SolveStatus::BudgetExhausted);
    assert_eq!(reverted.get(), 1);
    assert_eq!(report.attempts.len(), 1);
    assert_eq!(
        report.attempts[0].verification.status,
        VerificationStatus::Rejected
    );
    assert_eq!(
        report.counterexamples,
        vec![Counterexample {
            source: "diff".to_owned(),
            detail: "forbidden files modified: .env".to_owned(),
        }]
    );
}

#[test]
fn verifier_error_reverts_before_returning_error() {
    let reverted = Rc::new(Cell::new(0));
    let model = ScriptedModel {
        actions: vec![Action::ApplyPatch(Patch {
            unified_diff: "diff --git a/a b/a\n".to_owned(),
        })],
    };

    let mut agent = Agent::builder()
        .model(model)
        .execution(FakeExecution {
            reverted: Rc::clone(&reverted),
            fail_apply_patch: false,
        })
        .language_adapter(FakeLanguage)
        .indexer(EmptyIndex)
        .verifier(FailingVerifier)
        .policy(Policy::SafeStructuredOnly)
        .max_steps(3)
        .try_build()
        .expect("agent should build");

    let result = agent.solve(TaskSpec::from_text(PathBuf::from("."), "change code"));

    assert_eq!(reverted.get(), 1);
    assert_eq!(
        result,
        Err(PatchwrightError::Verification(
            "verifier crashed".to_owned()
        ))
    );
}

#[test]
fn apply_patch_error_reverts_before_returning_original_error() {
    let reverted = Rc::new(Cell::new(0));
    let model = ScriptedModel {
        actions: vec![Action::ApplyPatch(Patch {
            unified_diff: "diff --git a/a b/a\n".to_owned(),
        })],
    };

    let mut agent = Agent::builder()
        .model(model)
        .execution(FakeExecution {
            reverted: Rc::clone(&reverted),
            fail_apply_patch: true,
        })
        .language_adapter(FakeLanguage)
        .indexer(EmptyIndex)
        .verifier(AcceptingVerifier)
        .policy(Policy::SafeStructuredOnly)
        .max_steps(3)
        .try_build()
        .expect("agent should build");

    let result = agent.solve(TaskSpec::from_text(PathBuf::from("."), "change code"));

    assert_eq!(reverted.get(), 1);
    assert_eq!(
        result,
        Err(PatchwrightError::CommandFailed("apply failed".to_owned()))
    );
}

#[test]
fn budget_exhaustion_returns_budget_exhausted() {
    let model = ScriptedModel {
        actions: vec![Action::ListFiles(FileQuery::default())],
    };

    let mut agent = Agent::builder()
        .model(model)
        .execution(FakeExecution::default())
        .language_adapter(FakeLanguage)
        .indexer(EmptyIndex)
        .verifier(AcceptingVerifier)
        .policy(Policy::SafeStructuredOnly)
        .max_steps(1)
        .try_build()
        .expect("agent should build");

    let report = agent
        .solve(TaskSpec::from_text(PathBuf::from("."), "inspect code"))
        .expect("simulation should return a report");

    assert_eq!(report.status, SolveStatus::BudgetExhausted);
    assert_eq!(report.summary, "step budget exhausted");
}

#[test]
fn finish_action_returns_finished() {
    let model = ScriptedModel {
        actions: vec![Action::Finish {
            summary: "no change needed".to_owned(),
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
        .solve(TaskSpec::from_text(PathBuf::from("."), "inspect code"))
        .expect("simulation should finish");

    assert_eq!(report.status, SolveStatus::Finished);
    assert_eq!(report.summary, "no change needed");
    assert!(matches!(
        report.observations.last(),
        Some(Observation::Finished(summary)) if summary == "no change needed"
    ));
}

#[test]
fn model_request_includes_index_context_pack() {
    let requests = Rc::new(RefCell::new(Vec::new()));
    let model = RecordingModel {
        requests: Rc::clone(&requests),
        action: Action::Finish {
            summary: "done".to_owned(),
        },
    };

    let mut agent = Agent::builder()
        .model(model)
        .execution(FakeExecution::default())
        .language_adapter(FakeLanguage)
        .indexer(ContextIndex)
        .verifier(AcceptingVerifier)
        .policy(Policy::SafeStructuredOnly)
        .max_steps(1)
        .try_build()
        .expect("agent should build");

    agent
        .solve(TaskSpec::from_text(PathBuf::from("."), "inspect code"))
        .expect("simulation should finish");

    let requests = requests.borrow();
    let context = requests
        .first()
        .and_then(|request| request.context.as_ref())
        .expect("model request should include index context");

    assert_eq!(
        context,
        &ContextPack {
            files: vec![ScoredPath {
                path: RepoPath::new("src/lib.rs"),
                score: 7,
            }],
            likely_tests: Vec::new(),
            manifests: Vec::new(),
            recent_observations: Vec::new(),
            counterexamples: Vec::new(),
        }
    );
}

#[test]
fn code_change_task_rejects_immediate_finish() {
    let model = ScriptedModel {
        actions: vec![Action::Finish {
            summary: "done".to_owned(),
        }],
    };

    let mut agent = Agent::builder()
        .model(model)
        .execution(FakeExecution::default())
        .language_adapter(FakeLanguage)
        .indexer(EmptyIndex)
        .verifier(AcceptingVerifier)
        .policy(Policy::SafeStructuredOnly)
        .max_steps(1)
        .try_build()
        .expect("agent should build");

    let report = agent
        .solve(TaskSpec::code_change(PathBuf::from("."), "change code"))
        .expect("simulation should return patch-required report");

    assert_eq!(report.status, SolveStatus::BudgetExhausted);
    assert_eq!(report.summary, "step budget exhausted");
    assert!(report.attempts.is_empty());
    assert!(report.observations.iter().any(
        |observation| matches!(observation, Observation::Error(message) if message.contains("patch required"))
    ));
}

#[test]
fn code_change_task_can_recover_after_invalid_finish() {
    let model = ScriptedModel {
        actions: vec![
            Action::Finish {
                summary: "done".to_owned(),
            },
            Action::ApplyPatch(Patch {
                unified_diff: "diff --git a/a b/a\n".to_owned(),
            }),
        ],
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
        .solve(TaskSpec::code_change(PathBuf::from("."), "change code"))
        .expect("simulation should recover and accept patch");

    assert_eq!(report.status, SolveStatus::Accepted);
    assert_eq!(report.attempts.len(), 1);
    assert_eq!(
        report.attempts[0].verification.status,
        VerificationStatus::Accepted
    );
    assert!(report.observations.iter().any(
        |observation| matches!(observation, Observation::Error(message) if message.contains("patch required"))
    ));
}

#[test]
fn info_only_task_can_finish_without_patch() {
    let model = ScriptedModel {
        actions: vec![Action::Finish {
            summary: "no change needed".to_owned(),
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
    assert_eq!(report.summary, "no change needed");
    assert!(matches!(
        report.observations.last(),
        Some(Observation::Finished(summary)) if summary == "no change needed"
    ));
}

#[test]
fn try_build_reports_missing_required_fields() {
    let result = Agent::builder()
        .execution(FakeExecution::default())
        .language_adapter(FakeLanguage)
        .indexer(EmptyIndex)
        .verifier(AcceptingVerifier)
        .try_build();

    assert_eq!(
        result.err(),
        Some(PatchwrightError::InvalidInput(
            "model is required".to_owned()
        ))
    );
}
