use patchwright_config::{RustConfig, VerifyCommandConfig};
use patchwright_core::traits::{Indexer, LanguageAdapter};
use patchwright_core::types::{
    CommandSpec, Counterexample, DetectionScore, FileQuery, RepoView, RunReport, ScoredPath,
    TaskSpec, VerifierPlan,
};
use patchwright_lang_rust::RustAdapter;

pub(crate) fn rust_adapter(config: &RustConfig) -> RustAdapter {
    RustAdapter::new(config.fmt, config.check, config.test, config.clippy)
}

pub(crate) fn language_adapter(
    rust: &RustConfig,
    verify_commands: &[VerifyCommandConfig],
    repo: &RepoView,
) -> Result<CliLanguageAdapter, String> {
    let rust_adapter = rust_adapter(rust);
    if rust_adapter.detect(repo).0 > 0 {
        return Ok(CliLanguageAdapter::Rust(rust_adapter));
    }

    if !verify_commands.is_empty() {
        return Ok(CliLanguageAdapter::Configured(ConfiguredVerifyAdapter {
            commands: configured_command_specs(verify_commands)?,
        }));
    }

    Ok(CliLanguageAdapter::Rust(rust_adapter))
}

pub(crate) enum CliLanguageAdapter {
    Rust(RustAdapter),
    Configured(ConfiguredVerifyAdapter),
}

impl LanguageAdapter for CliLanguageAdapter {
    fn detect(&self, repo: &RepoView) -> DetectionScore {
        match self {
            Self::Rust(adapter) => adapter.detect(repo),
            Self::Configured(adapter) => adapter.detect(repo),
        }
    }

    fn verifier_plan(&self, task: &TaskSpec, repo: &RepoView) -> VerifierPlan {
        match self {
            Self::Rust(adapter) => adapter.verifier_plan(task, repo),
            Self::Configured(adapter) => adapter.verifier_plan(task, repo),
        }
    }

    fn relevant_files(&self, task: &TaskSpec, index: &dyn Indexer) -> Vec<ScoredPath> {
        match self {
            Self::Rust(adapter) => adapter.relevant_files(task, index),
            Self::Configured(adapter) => adapter.relevant_files(task, index),
        }
    }

    fn summarize_failure(&self, report: &RunReport) -> Vec<Counterexample> {
        match self {
            Self::Rust(adapter) => adapter.summarize_failure(report),
            Self::Configured(adapter) => adapter.summarize_failure(report),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ConfiguredVerifyAdapter {
    commands: Vec<CommandSpec>,
}

impl LanguageAdapter for ConfiguredVerifyAdapter {
    fn detect(&self, _repo: &RepoView) -> DetectionScore {
        DetectionScore(1)
    }

    fn verifier_plan(&self, _task: &TaskSpec, _repo: &RepoView) -> VerifierPlan {
        VerifierPlan {
            commands: self.commands.clone(),
        }
    }

    fn relevant_files(&self, _task: &TaskSpec, index: &dyn Indexer) -> Vec<ScoredPath> {
        index
            .list_files(FileQuery::default())
            .unwrap_or_default()
            .into_iter()
            .take(20)
            .collect()
    }

    fn summarize_failure(&self, report: &RunReport) -> Vec<Counterexample> {
        if report.status.success {
            return Vec::new();
        }

        let output = if report.stderr.trim().is_empty() {
            report.stdout.as_str()
        } else {
            report.stderr.as_str()
        };

        vec![Counterexample {
            source: report.command.program.clone(),
            detail: output.lines().take(20).collect::<Vec<_>>().join("\n"),
        }]
    }
}

fn configured_command_specs(commands: &[VerifyCommandConfig]) -> Result<Vec<CommandSpec>, String> {
    commands.iter().map(configured_command_spec).collect()
}

fn configured_command_spec(command: &VerifyCommandConfig) -> Result<CommandSpec, String> {
    let words = split_command_words(&command.command)?;
    let Some((program, args)) = words.split_first() else {
        return Err(format!("verify command '{}' is empty", command.name));
    };

    Ok(CommandSpec::new(program, args.iter()))
}

fn split_command_words(command: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote = None;

    for character in command.chars() {
        match (quote, character) {
            (Some(active), next) if next == active => quote = None,
            (Some(_), next) => current.push(next),
            (None, '"' | '\'') => quote = Some(character),
            (None, next) if next.is_whitespace() => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            (None, next) => current.push(next),
        }
    }

    if let Some(active) = quote {
        return Err(format!("verify command has unterminated {active} quote"));
    }
    if !current.is_empty() {
        words.push(current);
    }

    Ok(words)
}
