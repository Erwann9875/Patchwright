#![forbid(unsafe_code)]

use patchwright_core::agent::{Agent, SolveStatus};
use patchwright_core::policy::Policy;
use patchwright_core::traits::LanguageAdapter;
use patchwright_core::types::{RepoView, TaskSpec};
use patchwright_exec_local::LocalExecution;
use patchwright_index::BasicIndexer;
use patchwright_lang_rust::RustAdapter;
use patchwright_model_openai::{OpenAiCompatibleClient, OpenAiConfig};
use patchwright_verify::PlanVerifier;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
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
            println!("config: no config file required for this command");
            Ok(())
        }
        "bench" if args.get(1).map(String::as_str) == Some("startup") => run_startup_bench(),
        "solve" => run_solve(&args),
        "verify" => run_verify(&args),
        other => Err(format!("unknown command: {other}")),
    }
}

fn run_solve(args: &[String]) -> Result<(), String> {
    let Some(repo) = value_after(args, "--repo") else {
        return Err("solve requires --repo <path> and --task <text>".to_owned());
    };
    let Some(task) = value_after(args, "--task") else {
        return Err("solve requires --repo <path> and --task <text>".to_owned());
    };

    let repo = accessible_repo_path(&repo)?;
    let model = OpenAiCompatibleClient::dry_run(OpenAiConfig {
        base_url: "http://127.0.0.1:9".to_owned(),
        model: "dry-run".to_owned(),
        api_key_env: "OPENAI_API_KEY".to_owned(),
        timeout_seconds: 30,
    });
    let execution = LocalExecution::new(&repo);
    let language_adapter = RustAdapter::default();
    let indexer = BasicIndexer::new(&repo);
    let verifier = PlanVerifier;
    let policy = Policy::ProjectConfiguredCommands {
        allowed_programs: vec!["cargo".to_owned()],
    };
    let mut agent = Agent::builder()
        .model(model)
        .execution(execution)
        .language_adapter(language_adapter)
        .indexer(indexer)
        .verifier(verifier)
        .policy(policy)
        .max_steps(3)
        .try_build()
        .map_err(|error| error.to_string())?;

    let report = agent
        .solve(TaskSpec::from_text(repo, task))
        .map_err(|error| error.to_string())?;
    println!("solve status: {:?}", report.status);
    if report.status != SolveStatus::Accepted {
        println!("{}", report.summary);
    }

    Ok(())
}

fn run_verify(args: &[String]) -> Result<(), String> {
    let Some(repo) = value_after(args, "--repo") else {
        return Err("verify requires --repo <path>".to_owned());
    };

    let repo = accessible_repo_path(&repo)?;
    let adapter = RustAdapter::default();
    let repo_view = RepoView { root: repo.clone() };
    if adapter.detect(&repo_view).0 == 0 {
        return Err("no supported language adapter detected".to_owned());
    }

    let task = TaskSpec::from_text(repo, "verify");
    let plan = adapter.verifier_plan(&task, &repo_view);
    println!("verification plan:");
    for command in plan.commands {
        println!("  {} {}", command.program, command.args.join(" "));
    }

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

fn value_after(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .and_then(|window| {
            let value = &window[1];
            (!value.starts_with('-')).then(|| value.clone())
        })
}

fn accessible_repo_path(path: &str) -> Result<PathBuf, String> {
    fs::canonicalize(path).map_err(|_| format!("repo path is not accessible: {path}"))
}

fn print_help() {
    println!(
        "patchwright\n\nUSAGE:\n    patchwright --version\n    patchwright status\n    patchwright config check\n    patchwright bench startup\n    patchwright solve --repo <path> --task <text>\n    patchwright verify --repo <path>"
    );
}

#[cfg(test)]
mod tests {
    use super::run;

    #[test]
    fn version_route_returns_before_heavy_commands() {
        let result = run(["--version".to_owned()]);
        assert!(result.is_ok());
    }

    #[test]
    fn unknown_command_is_an_error() {
        let result = run(["unknown".to_owned()]);
        assert_eq!(result, Err("unknown command: unknown".to_owned()));
    }

    #[test]
    fn solve_requires_repo_and_task() {
        let result = run(["solve".to_owned()]);
        assert_eq!(
            result,
            Err("solve requires --repo <path> and --task <text>".to_owned())
        );
    }

    #[test]
    fn verify_requires_repo() {
        let result = run(["verify".to_owned()]);
        assert_eq!(result, Err("verify requires --repo <path>".to_owned()));
    }

    #[test]
    fn solve_rejects_flag_like_repo_value() {
        let result = run([
            "solve".to_owned(),
            "--repo".to_owned(),
            "--task".to_owned(),
            "summarize".to_owned(),
        ]);
        assert_eq!(
            result,
            Err("solve requires --repo <path> and --task <text>".to_owned())
        );
    }

    #[test]
    fn verify_rejects_flag_like_repo_value() {
        let result = run([
            "verify".to_owned(),
            "--repo".to_owned(),
            "--task".to_owned(),
        ]);
        assert_eq!(result, Err("verify requires --repo <path>".to_owned()));
    }

    #[test]
    fn solve_reports_inaccessible_repo_path() {
        let result = run([
            "solve".to_owned(),
            "--repo".to_owned(),
            "missing-repo-for-cli-test".to_owned(),
            "--task".to_owned(),
            "summarize".to_owned(),
        ]);
        assert_eq!(
            result,
            Err("repo path is not accessible: missing-repo-for-cli-test".to_owned())
        );
    }

    #[test]
    fn verify_reports_inaccessible_repo_path() {
        let result = run([
            "verify".to_owned(),
            "--repo".to_owned(),
            "missing-repo-for-cli-test".to_owned(),
        ]);
        assert_eq!(
            result,
            Err("repo path is not accessible: missing-repo-for-cli-test".to_owned())
        );
    }

    #[test]
    fn startup_benchmark_reports_micros_not_nanos() {
        assert_eq!(super::startup_average_micros(40_000, 20), 2);
    }
}
