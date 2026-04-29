use patchwright_core::error::{PatchwrightError, Result};
use patchwright_core::policy::Policy;
use patchwright_core::traits::{ExecutionBackend, Verifier};
use patchwright_core::types::{
    CheckReport, CommandSpec, Counterexample, ExitStatus, FileSlice, LineRange, Patch, PatchId,
    PolicyEvent, RepoPath, RunReport, SearchQuery, SearchResults, SnapshotId, VerificationStatus,
    VerifierPlan,
};
use patchwright_verify::PlanVerifier;

#[test]
fn empty_plan_accepts() -> Result<()> {
    let mut execution = FakeExecutionBackend::default();
    let mut verifier = PlanVerifier;
    let plan = VerifierPlan {
        commands: Vec::new(),
    };

    let report = verifier.verify(&mut execution, &plan, &Policy::FullShellAutonomous)?;

    assert_eq!(report.status, VerificationStatus::Accepted);
    assert!(report.checks.is_empty());
    assert!(report.counterexamples.is_empty());
    assert!(report.policy_events.is_empty());
    assert!(execution.ran_commands.is_empty());

    Ok(())
}

#[test]
fn all_pass_plan_accepts_without_counterexamples() -> Result<()> {
    let mut execution = FakeExecutionBackend::default();
    let mut verifier = PlanVerifier;
    let plan = VerifierPlan {
        commands: vec![
            CommandSpec::new("cargo", ["test"]),
            CommandSpec::new("cargo", ["clippy"]),
        ],
    };

    let report = verifier.verify(&mut execution, &plan, &Policy::FullShellAutonomous)?;

    assert_eq!(report.status, VerificationStatus::Accepted);
    assert_eq!(report.checks.len(), 2);
    assert!(report.checks.iter().all(|check| check.passed));
    assert!(report.counterexamples.is_empty());
    assert_eq!(execution.ran_commands.len(), 2);

    Ok(())
}

#[test]
fn rejects_plan_when_command_fails() -> Result<()> {
    let mut execution = FakeExecutionBackend::default();
    let mut verifier = PlanVerifier;
    let plan = VerifierPlan {
        commands: vec![CommandSpec::new("cargo", ["fail"])],
    };

    let report = verifier.verify(&mut execution, &plan, &Policy::FullShellAutonomous)?;

    assert_eq!(report.status, VerificationStatus::Rejected);
    assert_eq!(report.counterexamples.len(), 1);

    Ok(())
}

#[test]
fn multiple_commands_reject_with_counterexamples_only_for_failures() -> Result<()> {
    let mut execution = FakeExecutionBackend::default();
    let mut verifier = PlanVerifier;
    let plan = VerifierPlan {
        commands: vec![
            CommandSpec::new("cargo", ["test"]),
            CommandSpec::new("cargo", ["fail"]),
            CommandSpec::new("cargo", ["clippy"]),
        ],
    };

    let report = verifier.verify(&mut execution, &plan, &Policy::FullShellAutonomous)?;

    assert_eq!(report.status, VerificationStatus::Rejected);
    assert_eq!(
        report
            .checks
            .iter()
            .map(|check| check.passed)
            .collect::<Vec<_>>(),
        vec![true, false, true]
    );
    assert_eq!(
        report.counterexamples,
        vec![Counterexample {
            source: "cargo".to_owned(),
            detail: "command failed".to_owned(),
        }]
    );
    assert_eq!(execution.ran_commands.len(), 3);

    Ok(())
}

#[test]
fn policy_denial_returns_rejected_report_without_running_command() -> Result<()> {
    let mut execution = FakeExecutionBackend::default();
    let mut verifier = PlanVerifier;
    let command = CommandSpec::new("cargo", ["test"]);
    let plan = VerifierPlan {
        commands: vec![command.clone()],
    };

    let report = verifier.verify(&mut execution, &plan, &Policy::SafeStructuredOnly)?;

    assert_eq!(report.status, VerificationStatus::Rejected);
    assert_eq!(execution.ran_commands, Vec::<CommandSpec>::new());
    assert_eq!(
        report.checks,
        vec![CheckReport {
            name: "cargo test".to_owned(),
            command: Some(command.clone()),
            passed: false,
            summary: "denied".to_owned(),
        }]
    );
    assert_eq!(
        report.counterexamples,
        vec![Counterexample {
            source: "cargo".to_owned(),
            detail: "denied".to_owned(),
        }]
    );
    assert_eq!(
        report.policy_events,
        vec![PolicyEvent {
            command: Some(command),
            allowed: false,
            reason: "denied".to_owned(),
        }]
    );

    Ok(())
}

#[test]
fn policy_events_record_allowed_and_denied_commands() -> Result<()> {
    let mut execution = FakeExecutionBackend::default();
    let mut verifier = PlanVerifier;
    let allowed_command = CommandSpec::new("cargo", ["test"]);
    let denied_command = CommandSpec::new("git", ["status"]);
    let plan = VerifierPlan {
        commands: vec![allowed_command.clone(), denied_command.clone()],
    };

    let report = verifier.verify(
        &mut execution,
        &plan,
        &Policy::ProjectConfiguredCommands {
            allowed_programs: vec!["cargo".to_owned()],
        },
    )?;

    assert_eq!(report.status, VerificationStatus::Rejected);
    assert_eq!(execution.ran_commands, vec![allowed_command.clone()]);
    assert_eq!(
        report.policy_events,
        vec![
            PolicyEvent {
                command: Some(allowed_command),
                allowed: true,
                reason: "allowed".to_owned(),
            },
            PolicyEvent {
                command: Some(denied_command),
                allowed: false,
                reason: "denied".to_owned(),
            },
        ]
    );

    Ok(())
}

#[test]
fn failure_summary_falls_back_to_stdout_when_stderr_is_whitespace() -> Result<()> {
    let mut execution = FakeExecutionBackend {
        failure_stdout: "stdout failure".to_owned(),
        failure_stderr: " \n\t ".to_owned(),
        ..FakeExecutionBackend::default()
    };
    let mut verifier = PlanVerifier;
    let plan = VerifierPlan {
        commands: vec![CommandSpec::new("cargo", ["fail"])],
    };

    let report = verifier.verify(&mut execution, &plan, &Policy::FullShellAutonomous)?;

    assert_eq!(report.checks[0].summary, "stdout failure");
    assert_eq!(report.counterexamples[0].detail, "stdout failure");

    Ok(())
}

#[test]
fn failure_summary_is_bounded_to_twenty_lines() -> Result<()> {
    let stderr = (1..=25)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let expected = (1..=20)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut execution = FakeExecutionBackend {
        failure_stderr: stderr,
        ..FakeExecutionBackend::default()
    };
    let mut verifier = PlanVerifier;
    let plan = VerifierPlan {
        commands: vec![CommandSpec::new("cargo", ["fail"])],
    };

    let report = verifier.verify(&mut execution, &plan, &Policy::FullShellAutonomous)?;

    assert_eq!(report.checks[0].summary, expected);
    assert_eq!(report.checks[0].summary.lines().count(), 20);

    Ok(())
}

#[test]
fn backend_error_is_propagated() {
    let mut execution = FakeExecutionBackend {
        fail_with_error: true,
        ..FakeExecutionBackend::default()
    };
    let mut verifier = PlanVerifier;
    let plan = VerifierPlan {
        commands: vec![CommandSpec::new("cargo", ["test"])],
    };

    let result = verifier.verify(&mut execution, &plan, &Policy::FullShellAutonomous);

    assert_eq!(
        result,
        Err(PatchwrightError::CommandFailed(
            "backend failure".to_owned()
        ))
    );
}

#[derive(Debug)]
struct FakeExecutionBackend {
    ran_commands: Vec<CommandSpec>,
    failure_stdout: String,
    failure_stderr: String,
    fail_with_error: bool,
}

impl Default for FakeExecutionBackend {
    fn default() -> Self {
        Self {
            ran_commands: Vec::new(),
            failure_stdout: String::new(),
            failure_stderr: "command failed".to_owned(),
            fail_with_error: false,
        }
    }
}

impl ExecutionBackend for FakeExecutionBackend {
    fn snapshot(&mut self) -> Result<SnapshotId> {
        Ok(SnapshotId("fake".to_owned()))
    }

    fn read_file(&self, path: RepoPath, _range: Option<LineRange>) -> Result<FileSlice> {
        Ok(FileSlice {
            path,
            start_line: 1,
            content: String::new(),
        })
    }

    fn search(&self, _query: SearchQuery) -> Result<SearchResults> {
        Ok(SearchResults {
            matches: Vec::new(),
        })
    }

    fn apply_patch(&mut self, _patch: Patch) -> Result<PatchId> {
        Ok(PatchId("fake".to_owned()))
    }

    fn run(&mut self, command: CommandSpec, _policy: &Policy) -> Result<RunReport> {
        if self.fail_with_error {
            return Err(PatchwrightError::CommandFailed(
                "backend failure".to_owned(),
            ));
        }

        let success = !command.args.iter().any(|arg| arg == "fail");
        self.ran_commands.push(command.clone());

        Ok(RunReport {
            command,
            status: ExitStatus {
                code: Some(if success { 0 } else { 1 }),
                success,
            },
            stdout: if success {
                String::new()
            } else {
                self.failure_stdout.clone()
            },
            stderr: if success {
                String::new()
            } else {
                self.failure_stderr.clone()
            },
        })
    }

    fn revert(&mut self, _snapshot: SnapshotId) -> Result<()> {
        Ok(())
    }
}
