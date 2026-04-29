use crate::action::{Action, Observation};
use crate::error::{PatchwrightError, Result};
use crate::policy::Policy;
use crate::traits::{ExecutionBackend, Indexer, LanguageAdapter, ModelProvider, Verifier};
use crate::types::{
    Attempt, CheckReport, Counterexample, ModelRequest, RepoView, TaskSpec, VerificationReport,
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
            let context = self
                .indexer
                .context_pack(&task, &observations, &counterexamples)?;
            let response = self.model.propose_action(ModelRequest {
                task: task.clone(),
                observations: observations.clone(),
                counterexamples: counterexamples.clone(),
                context: Some(context),
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
                    let patch_id = match self.execution.apply_patch(patch) {
                        Ok(patch_id) => patch_id,
                        Err(error) => {
                            let _ = self.execution.revert(snapshot);
                            return Err(error);
                        }
                    };
                    observations.push(Observation::PatchApplied);
                    let plan = self.language_adapter.verifier_plan(&task, &repo);
                    let verification =
                        match self
                            .verifier
                            .verify(self.execution.as_mut(), &plan, &self.policy)
                        {
                            Ok(verification) => self.apply_acceptance_gate(verification),
                            Err(error) => {
                                let _ = self.execution.revert(snapshot);
                                return Err(error);
                            }
                        };
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
                    let verification = self.apply_acceptance_gate(verification);
                    counterexamples.extend(verification.counterexamples.clone());
                    observations.push(Observation::VerificationCompleted(verification));
                }
                Action::RunTests | Action::RunTypecheck | Action::RunBenchmark => {
                    let plan = self.language_adapter.verifier_plan(&task, &repo);
                    let verification =
                        self.verifier
                            .verify(self.execution.as_mut(), &plan, &self.policy)?;
                    let verification = self.apply_acceptance_gate(verification);
                    counterexamples.extend(verification.counterexamples.clone());
                    observations.push(Observation::VerificationCompleted(verification));
                }
                Action::RevertAttempt(snapshot) => {
                    self.execution.revert(snapshot.clone())?;
                    observations.push(Observation::Reverted(snapshot));
                }
                Action::Finish { summary } => {
                    if task.require_patch {
                        let message = "patch required before finish".to_owned();
                        observations.push(Observation::Error(message));
                        continue;
                    }

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

    fn apply_acceptance_gate(&self, mut verification: VerificationReport) -> VerificationReport {
        let denied_policy_events = verification
            .policy_events
            .iter()
            .filter(|event| !event.allowed)
            .count();
        if denied_policy_events > 0 {
            reject_verification(
                &mut verification,
                "policy gate",
                "denied commands were requested",
                "policy",
            );
        }

        let forbidden_paths = verification
            .diff_summary
            .changed_files
            .iter()
            .filter(|path| !self.policy.allows_repo_path(path))
            .map(|path| path.0.clone())
            .collect::<Vec<_>>();
        if !forbidden_paths.is_empty() {
            reject_verification(
                &mut verification,
                "diff scope",
                &format!("forbidden files modified: {}", forbidden_paths.join(", ")),
                "diff",
            );
        }

        verification
    }
}

fn reject_verification(
    verification: &mut VerificationReport,
    check_name: &str,
    summary: &str,
    source: &str,
) {
    if verification
        .checks
        .iter()
        .any(|check| check.name == check_name && !check.passed && check.summary == summary)
    {
        verification.status = VerificationStatus::Rejected;
        return;
    }

    verification.status = VerificationStatus::Rejected;
    verification.checks.push(CheckReport {
        name: check_name.to_owned(),
        command: None,
        passed: false,
        summary: summary.to_owned(),
    });
    verification.counterexamples.push(Counterexample {
        source: source.to_owned(),
        detail: summary.to_owned(),
    });
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
        match self.try_build() {
            Ok(agent) => agent,
            Err(error) => panic!("{error}"),
        }
    }

    pub fn try_build(self) -> Result<Agent> {
        Ok(Agent {
            model: required(self.model, "model is required")?,
            execution: required(self.execution, "execution backend is required")?,
            language_adapter: required(self.language_adapter, "language adapter is required")?,
            indexer: required(self.indexer, "indexer is required")?,
            verifier: required(self.verifier, "verifier is required")?,
            policy: self.policy.unwrap_or(Policy::SafeStructuredOnly),
            max_steps: self.max_steps.unwrap_or(30),
        })
    }
}

fn required<T>(value: Option<T>, message: &str) -> Result<T> {
    value.ok_or_else(|| PatchwrightError::InvalidInput(message.to_owned()))
}
