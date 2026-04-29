#![forbid(unsafe_code)]

use patchwright_core::traits::{Indexer, LanguageAdapter};
use patchwright_core::types::{
    CommandSpec, Counterexample, DetectionScore, FileQuery, RepoView, RunReport, ScoredPath,
    TaskSpec, VerifierPlan,
};
use std::time::Duration;

const CARGO_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone)]
pub struct RustAdapter {
    fmt: bool,
    check: bool,
    test: bool,
    clippy: bool,
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

impl RustAdapter {
    pub fn new(fmt: bool, check: bool, test: bool, clippy: bool) -> Self {
        Self {
            fmt,
            check,
            test,
            clippy,
        }
    }
}

impl LanguageAdapter for RustAdapter {
    fn detect(&self, repo: &RepoView) -> DetectionScore {
        if repo.root.join("Cargo.toml").is_file() {
            DetectionScore(100)
        } else {
            DetectionScore(0)
        }
    }

    fn verifier_plan(&self, _task: &TaskSpec, _repo: &RepoView) -> VerifierPlan {
        let mut commands = Vec::new();

        if self.fmt {
            commands.push(cargo(["fmt", "--", "--check"]));
        }
        if self.check {
            commands.push(cargo(["check"]));
        }
        if self.clippy {
            commands.push(cargo(["clippy", "--", "-D", "warnings"]));
        }
        if self.test {
            commands.push(cargo(["test"]));
        }

        VerifierPlan { commands }
    }

    fn relevant_files(&self, task: &TaskSpec, index: &dyn Indexer) -> Vec<ScoredPath> {
        let task_mentions_test = task.text.to_lowercase().contains("test");
        let Ok(mut files) = index.list_files(FileQuery::default()) else {
            return Vec::new();
        };

        for file in &mut files {
            if file.path.0.ends_with(".rs") {
                file.score += 5;
            }
            if task_mentions_test && file.path.0.contains("test") {
                file.score += 10;
            }
        }

        files
    }

    fn summarize_failure(&self, report: &RunReport) -> Vec<Counterexample> {
        if report.status.success {
            return Vec::new();
        }

        let detail = if report.stderr.trim().is_empty() {
            report.stdout.as_str()
        } else {
            report.stderr.as_str()
        }
        .lines()
        .take(20)
        .collect::<Vec<_>>()
        .join("\n");

        vec![Counterexample {
            source: report.command.program.clone(),
            detail,
        }]
    }
}

fn cargo<const N: usize>(args: [&str; N]) -> CommandSpec {
    let mut command = CommandSpec::new("cargo", args);
    command.timeout = CARGO_TIMEOUT;
    command
}

#[cfg(test)]
mod tests {
    use super::RustAdapter;
    use patchwright_core::traits::LanguageAdapter;
    use patchwright_core::types::{RepoView, TaskSpec};
    use std::path::PathBuf;

    #[test]
    fn verifier_plan_includes_clippy_with_denied_warnings_when_enabled() {
        let adapter = RustAdapter::new(false, false, false, true);
        let task = TaskSpec::from_text(PathBuf::new(), "verify clippy");
        let repo = RepoView {
            root: PathBuf::new(),
        };

        let plan = adapter.verifier_plan(&task, &repo);

        let command = &plan.commands[0];
        assert_eq!(command.program, "cargo");
        assert_eq!(command.args, ["clippy", "--", "-D", "warnings"]);
        assert_eq!(command.timeout.as_secs(), 300);
    }

    #[test]
    fn verifier_plan_uses_valid_cargo_fmt_check_shape() {
        let adapter = RustAdapter::new(true, false, false, false);
        let task = TaskSpec::from_text(PathBuf::new(), "verify fmt");
        let repo = RepoView {
            root: PathBuf::new(),
        };

        let plan = adapter.verifier_plan(&task, &repo);

        let command = &plan.commands[0];
        assert_eq!(command.program, "cargo");
        assert_eq!(command.args, ["fmt", "--", "--check"]);
    }
}
