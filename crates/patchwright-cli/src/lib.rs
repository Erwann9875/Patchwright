#![forbid(unsafe_code)]

mod args;
mod design_render;
mod git_ops;
mod implementation_plan;
mod language_wiring;
mod model_wiring;
mod profile_report;
mod render_utils;
mod review;
mod solve_report;

use args::{
    accessible_repo_path, design_options, has_flag, implement_options, model_provider_name,
    solve_options, value_after, SolveOptions,
};
use design_render::write_design_document;
use git_ops::{apply_sandbox_diff_to_source, git_diff};
use implementation_plan::{out_of_scope_changed_files, StepScope};
use language_wiring::language_adapter;
#[cfg(test)]
use language_wiring::rust_adapter;
use model_wiring::{cli_model, design_model};
use patchwright_config::PatchwrightConfig;
use patchwright_core::agent::{Agent, SolveReport, SolveStatus};
use patchwright_core::policy::Policy;
use patchwright_core::traits::{Indexer, LanguageAdapter, ModelProvider, Verifier};
use patchwright_core::types::{ModelRequest, RepoView, TaskSpec, VerificationStatus};
use patchwright_exec_local::{GitWorktreeSandbox, LocalExecution};
use patchwright_index::{profile_project, BasicIndexer};
use patchwright_model_codex_cli::{CodexCliClient, CodexCliConfig};
use patchwright_verify::PlanVerifier;
use profile_report::render_project_profile;
use review::{build_review_report, render_review_report};
use solve_report::render_solve_report;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

pub fn main_entry() -> ExitCode {
    match run(env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

fn run<I>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = String>,
{
    let args: Vec<String> = args.into_iter().collect();

    if args.is_empty() || args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_help();
        return Ok(());
    }

    if args.iter().any(|arg| arg == "-V" || arg == "--version") {
        println!("patchwright {}", patchwright_core::VERSION);
        return Ok(());
    }

    match args[0].as_str() {
        "status" => {
            println!("patchwright: ready");
            Ok(())
        }
        "config" if args.get(1).map(String::as_str) == Some("check") => {
            println!("{}", run_config_check(&args)?);
            Ok(())
        }
        "auth" if args.get(1).map(String::as_str) == Some("login") => run_auth_login(&args),
        "auth" if args.get(1).map(String::as_str) == Some("check") => run_auth_check(&args),
        "bench" if args.get(1).map(String::as_str) == Some("startup") => run_startup_bench(),
        "profile" => run_profile(&args),
        "design" => run_design(&args),
        "implement" => run_implement(&args),
        "review" => run_review(&args),
        "solve" => run_solve(&args),
        "verify" => run_verify(&args),
        other => Err(format!("unknown command: {other}")),
    }
}

fn run_profile(args: &[String]) -> Result<(), String> {
    let repo = command_repo(args, 1, "profile")?;
    let profile = profile_project(&repo).map_err(|error| error.to_string())?;
    println!("{}", render_project_profile(&profile));
    Ok(())
}

fn run_solve(args: &[String]) -> Result<(), String> {
    let options = solve_options(args)?;
    let report = execute_solve(&options, None)?;
    println!("{}", render_solve_report(&report));

    Ok(())
}

fn run_implement(args: &[String]) -> Result<(), String> {
    let (options, scope) = implement_options(args)?;
    let report = execute_solve(&options, scope.as_ref())?;
    println!("{}", render_solve_report(&report));

    Ok(())
}

fn run_review(args: &[String]) -> Result<(), String> {
    let repo = review_repo(args)?;
    let diff = git_diff(&repo)?;
    let report = build_review_report(&diff);
    println!("{}", render_review_report(&report));
    Ok(())
}

fn execute_solve(options: &SolveOptions, scope: Option<&StepScope>) -> Result<SolveReport, String> {
    let model = cli_model(options)?;

    let sandbox = GitWorktreeSandbox::create(&options.repo).map_err(|error| error.to_string())?;
    let sandbox_repo = sandbox.root().to_path_buf();
    let execution = LocalExecution::new(&sandbox_repo);
    let language_adapter = language_adapter(
        &options.rust,
        &options.verify_commands,
        &RepoView {
            root: sandbox_repo.clone(),
        },
    )?;
    let indexer = BasicIndexer::new(&sandbox_repo);
    let verifier = PlanVerifier;
    let policy = Policy::ProjectConfiguredCommands {
        allowed_programs: options.allowed_programs.clone(),
    };
    let mut agent = Agent::builder()
        .model(model)
        .execution(execution)
        .language_adapter(language_adapter)
        .indexer(indexer)
        .verifier(verifier)
        .policy(policy)
        .max_steps(options.max_steps)
        .max_changed_files(options.max_changed_files)
        .max_inserted_lines(options.max_inserted_lines)
        .try_build()
        .map_err(|error| error.to_string())?;

    let report = agent
        .solve(
            TaskSpec::from_text(sandbox_repo.clone(), options.task.clone())
                .with_require_patch(options.require_patch),
        )
        .map_err(|error| error.to_string())?;
    if report.status == SolveStatus::Accepted {
        if let Some(scope) = scope {
            let outside = out_of_scope_changed_files(&report, scope);
            if !outside.is_empty() {
                return Err(format!(
                    "accepted patch changed files outside selected plan step: {}",
                    outside
                        .iter()
                        .map(|path| path.0.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
        apply_sandbox_diff_to_source(&sandbox_repo, &options.repo)?;
    }
    Ok(report)
}

fn run_design(args: &[String]) -> Result<(), String> {
    let options = design_options(args)?;
    let mut model = design_model(&options)?;
    let indexer = BasicIndexer::new(&options.repo);
    let task = TaskSpec::from_text(options.repo.clone(), options.task.clone());
    let context = indexer
        .context_pack(&task, &[], &[])
        .map_err(|error| error.to_string())?;
    let design = model
        .propose_design(ModelRequest {
            task,
            observations: Vec::new(),
            counterexamples: Vec::new(),
            context: Some(context),
        })
        .map_err(|error| error.to_string())?;
    let path = write_design_document(&options.repo, &options.task, &design)?;

    println!("design written: {}", path.display());

    Ok(())
}

fn run_config_check(args: &[String]) -> Result<String, String> {
    let repo = config_check_repo(args)?;
    let config = PatchwrightConfig::load(&repo).map_err(|error| error.to_string())?;

    Ok(format!(
        "config: ok\nmodel provider: {}\nmodel base_url: {}\nagent max_steps: {}\npolicy allowed_programs: {}",
        model_provider_name(&config.model.provider),
        config.model.openai.base_url.as_deref().unwrap_or(&config.model.base_url),
        config.agent.max_steps,
        config.policy.allowed_programs.join(",")
    ))
}

fn run_auth_login(args: &[String]) -> Result<(), String> {
    let config = auth_codex_config(args)?;
    CodexCliClient::new(config)
        .login()
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn run_auth_check(args: &[String]) -> Result<(), String> {
    let config = auth_codex_config(args)?;
    CodexCliClient::new(config)
        .check_auth()
        .map_err(|error| error.to_string())?;
    println!("auth: ok");
    Ok(())
}

fn auth_codex_config(args: &[String]) -> Result<CodexCliConfig, String> {
    let repo = command_repo(args, 2, "auth")?;
    let config = PatchwrightConfig::load(&repo).map_err(|error| error.to_string())?;

    Ok(CodexCliConfig {
        command: config.model.codex_cli.command,
        model: config.model.codex_cli.model.or(config.model.model),
        timeout_seconds: config.model.codex_cli.timeout_seconds,
    })
}

fn config_check_repo(args: &[String]) -> Result<PathBuf, String> {
    command_repo(args, 2, "config check")
}

fn review_repo(args: &[String]) -> Result<PathBuf, String> {
    if !has_flag(args, "--diff") {
        return Err("review requires --diff".to_owned());
    }

    let mut filtered = Vec::with_capacity(args.len() - 1);
    for arg in args {
        if arg != "--diff" {
            filtered.push(arg.clone());
        }
    }

    command_repo(&filtered, 1, "review")
}

fn command_repo(
    args: &[String],
    start_index: usize,
    command_name: &str,
) -> Result<PathBuf, String> {
    let mut index = start_index;
    let mut repo = None;

    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "--repo" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(format!("{command_name} requires --repo <path>"));
                };
                if value.starts_with("--") {
                    return Err(format!("{command_name} requires --repo <path>"));
                }
                repo = Some(value.clone());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(format!("unknown {command_name} flag: {value}"));
            }
            value => {
                return Err(format!("unexpected {command_name} argument: {value}"));
            }
        }
    }

    match repo {
        Some(repo) => accessible_repo_path(&repo),
        None => env::current_dir().map_err(|error| error.to_string()),
    }
}

fn run_verify(args: &[String]) -> Result<(), String> {
    let Some(repo) = value_after(args, "--repo") else {
        return Err("verify requires --repo <path>".to_owned());
    };

    let repo = accessible_repo_path(&repo)?;
    let config = PatchwrightConfig::load(&repo).map_err(|error| error.to_string())?;
    let sandbox = GitWorktreeSandbox::create(&repo).map_err(|error| error.to_string())?;
    let sandbox_repo = sandbox.root().to_path_buf();
    let repo_view = RepoView {
        root: sandbox_repo.clone(),
    };
    let adapter = language_adapter(&config.rust, &config.verify.commands, &repo_view)?;
    if adapter.detect(&repo_view).0 == 0 {
        return Err("no supported language adapter detected".to_owned());
    }

    let task = TaskSpec::from_text(sandbox_repo.clone(), "verify");
    let plan = adapter.verifier_plan(&task, &repo_view);
    println!("verification plan:");
    for command in &plan.commands {
        println!("  {} {}", command.program, command.args.join(" "));
    }

    let mut execution = LocalExecution::new(&sandbox_repo);
    let mut verifier = PlanVerifier;
    let policy = Policy::ProjectConfiguredCommands {
        allowed_programs: config.policy.allowed_programs,
    };
    let report = verifier
        .verify(&mut execution, &plan, &policy)
        .map_err(|error| error.to_string())?;

    for check in &report.checks {
        let status = if check.passed { "ok" } else { "failed" };
        println!("  [{status}] {}: {}", check.name, check.summary);
    }

    if report.status != VerificationStatus::Accepted {
        return Err(format!(
            "verification rejected: {}",
            report
                .counterexamples
                .first()
                .map(|counterexample| counterexample.detail.as_str())
                .unwrap_or("no counterexample reported")
        ));
    }

    println!("verification: accepted");

    Ok(())
}

fn run_startup_bench() -> Result<(), String> {
    let exe = env::current_exe().map_err(|error| error.to_string())?;
    let iterations = 20;
    let mut total_nanos = 0u128;

    for _ in 0..iterations {
        let start = std::time::Instant::now();
        let output = std::process::Command::new(&exe)
            .arg("--version")
            .output()
            .map_err(|error| error.to_string())?;
        if !output.status.success() {
            return Err("startup benchmark child command failed".to_owned());
        }
        total_nanos += start.elapsed().as_nanos();
    }

    let average_micros = startup_average_micros(total_nanos, iterations);
    println!("startup_version_average_micros={average_micros}");
    Ok(())
}

fn startup_average_micros(total_nanos: u128, iterations: u128) -> u128 {
    total_nanos / iterations / 1_000
}

fn print_help() {
    println!(
        "patchwright\n\nUSAGE:\n    patchwright --version\n    patchwright status\n    patchwright auth login [--repo <path>]\n    patchwright auth check [--repo <path>]\n    patchwright config check [--repo <path>]\n    patchwright bench startup\n    patchwright profile [--repo <path>]\n    patchwright design --repo <path> --task <text> [--dry-run] [--model-provider codex-cli|openai-compatible] [--model <name>] [--base-url <url>] [--api-key-env <name>]\n    patchwright implement --repo <path> --from <plan> --step <id> [--dry-run] [--model-provider codex-cli|openai-compatible] [--model <name>] [--base-url <url>] [--api-key-env <name>] [--max-steps <n>] [--info-only] [--allow-out-of-scope]\n    patchwright review [--repo <path>] --diff\n    patchwright solve --repo <path> --task <text> [--dry-run] [--model-provider codex-cli|openai-compatible] [--model <name>] [--base-url <url>] [--api-key-env <name>] [--max-steps <n>] [--info-only]\n    patchwright verify --repo <path>"
    );
}

#[cfg(test)]
mod tests;
