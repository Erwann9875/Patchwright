#![forbid(unsafe_code)]

use patchwright_core::policy::Policy;
use patchwright_core::traits::{ExecutionBackend, Verifier};
use patchwright_core::types::{
    CheckReport, Counterexample, DiffSummary, PolicyEvent, VerificationReport, VerificationStatus,
    VerifierPlan,
};
use patchwright_core::Result;

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
                reason: if allowed { "allowed" } else { "denied" }.to_owned(),
            });

            if !allowed {
                let summary = "denied".to_owned();
                checks.push(CheckReport {
                    name: command_name(command),
                    command: Some(command.clone()),
                    passed: false,
                    summary: summary.clone(),
                });
                counterexamples.push(Counterexample {
                    source: command.program.clone(),
                    detail: summary,
                });
                continue;
            }

            let report = execution.run(command.clone(), policy)?;
            let passed = report.status.success;
            let summary = if passed {
                "passed".to_owned()
            } else {
                failure_summary(&report.stderr, &report.stdout)
            };

            checks.push(CheckReport {
                name: command_name(command),
                command: Some(command.clone()),
                passed,
                summary: summary.clone(),
            });

            if !passed {
                counterexamples.push(Counterexample {
                    source: command.program.clone(),
                    detail: summary,
                });
            }
        }

        let status = if checks.iter().all(|check| check.passed) {
            VerificationStatus::Accepted
        } else {
            VerificationStatus::Rejected
        };

        Ok(VerificationReport {
            status,
            checks,
            counterexamples,
            diff_summary: DiffSummary::default(),
            policy_events,
        })
    }
}

fn command_name(command: &patchwright_core::types::CommandSpec) -> String {
    format!("{} {}", command.program, command.args.join(" "))
}

fn failure_summary(stderr: &str, stdout: &str) -> String {
    let output = if stderr.trim().is_empty() {
        stdout
    } else {
        stderr
    };

    output.lines().take(20).collect::<Vec<_>>().join("\n")
}
