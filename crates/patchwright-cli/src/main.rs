#![forbid(unsafe_code)]

use patchwright_config::{PatchwrightConfig, RustConfig};
use patchwright_core::agent::{Agent, SolveStatus};
use patchwright_core::policy::Policy;
use patchwright_core::traits::{LanguageAdapter, Verifier};
use patchwright_core::types::{RepoView, TaskSpec, VerificationStatus};
use patchwright_exec_local::{GitWorktreeSandbox, LocalExecution};
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
            println!("{}", run_config_check(&args)?);
            Ok(())
        }
        "bench" if args.get(1).map(String::as_str) == Some("startup") => run_startup_bench(),
        "solve" => run_solve(&args),
        "verify" => run_verify(&args),
        other => Err(format!("unknown command: {other}")),
    }
}

fn run_solve(args: &[String]) -> Result<(), String> {
    let options = solve_options(args)?;

    let model_name = match options.model.clone() {
        Some(model_name) => model_name,
        None if options.dry_run => "dry-run".to_owned(),
        None => return Err("solve real mode requires --model <name> or --dry-run".to_owned()),
    };
    let sandbox = GitWorktreeSandbox::create(&options.repo).map_err(|error| error.to_string())?;
    let sandbox_repo = sandbox.root().to_path_buf();
    let config = OpenAiConfig {
        base_url: options.base_url,
        model: model_name,
        api_key_env: options.api_key_env,
        timeout_seconds: 30,
    };
    let model = if options.dry_run {
        OpenAiCompatibleClient::dry_run(config)
    } else {
        OpenAiCompatibleClient::new(config)
    };
    let execution = LocalExecution::new(&sandbox_repo);
    let language_adapter = rust_adapter(&options.rust);
    let indexer = BasicIndexer::new(&sandbox_repo);
    let verifier = PlanVerifier;
    let policy = Policy::ProjectConfiguredCommands {
        allowed_programs: options.allowed_programs,
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
            TaskSpec::from_text(sandbox_repo, options.task)
                .with_require_patch(options.require_patch),
        )
        .map_err(|error| error.to_string())?;
    println!("solve status: {:?}", report.status);
    if report.status != SolveStatus::Accepted {
        println!("{}", report.summary);
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SolveOptions {
    repo: PathBuf,
    task: String,
    dry_run: bool,
    model: Option<String>,
    base_url: String,
    api_key_env: String,
    max_steps: usize,
    require_patch: bool,
    max_changed_files: usize,
    max_inserted_lines: usize,
    allowed_programs: Vec<String>,
    rust: RustConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SolveFlags {
    repo: String,
    task: String,
    dry_run: bool,
    model: Option<String>,
    base_url: Option<String>,
    api_key_env: Option<String>,
    max_steps: Option<usize>,
    info_only: bool,
}

fn solve_options(args: &[String]) -> Result<SolveOptions, String> {
    let flags = SolveFlags::parse(args)?;
    let repo = accessible_repo_path(&flags.repo)?;
    let config = PatchwrightConfig::load(&repo).map_err(|error| error.to_string())?;

    Ok(SolveOptions {
        repo,
        task: flags.task,
        dry_run: flags.dry_run,
        model: flags.model.or(config.model.model),
        base_url: flags.base_url.unwrap_or(config.model.base_url),
        api_key_env: flags.api_key_env.unwrap_or(config.model.api_key_env),
        max_steps: flags.max_steps.unwrap_or(config.agent.max_steps),
        require_patch: if flags.info_only {
            false
        } else {
            config.agent.require_patch
        },
        max_changed_files: config.agent.max_changed_files,
        max_inserted_lines: config.agent.max_inserted_lines,
        allowed_programs: config.policy.allowed_programs,
        rust: config.rust,
    })
}

impl SolveFlags {
    fn parse(args: &[String]) -> Result<Self, String> {
        validate_solve_args(args)?;

        let Some(repo) = value_after(args, "--repo") else {
            return Err("solve requires --repo <path> and --task <text>".to_owned());
        };
        let Some(task) = value_after(args, "--task") else {
            return Err("solve requires --repo <path> and --task <text>".to_owned());
        };

        let dry_run = has_flag(args, "--dry-run");
        let model = optional_value_if_present(args, "--model")?;

        let max_steps = match value_after(args, "--max-steps") {
            Some(value) => value
                .parse::<usize>()
                .ok()
                .filter(|value| *value > 0)
                .map(Some)
                .ok_or_else(|| "solve requires --max-steps to be a positive integer".to_owned())?,
            None if has_flag(args, "--max-steps") => {
                return Err("solve requires --max-steps to be a positive integer".to_owned());
            }
            None => None,
        };

        Ok(Self {
            repo,
            task,
            dry_run,
            model,
            base_url: optional_value_if_present(args, "--base-url")?,
            api_key_env: optional_value_if_present(args, "--api-key-env")?,
            max_steps,
            info_only: has_flag(args, "--info-only"),
        })
    }
}

fn validate_solve_args(args: &[String]) -> Result<(), String> {
    for arg in args.iter().skip(1) {
        if arg == "--api-key" || arg.starts_with("--api-key=") {
            return Err("raw API keys are not accepted; use --api-key-env <name>".to_owned());
        }
    }

    let mut index = 1;
    while index < args.len() {
        let arg = &args[index];
        if !arg.starts_with("--") {
            return Err(format!("unexpected solve argument: {arg}"));
        }

        if is_solve_value_flag(arg) {
            index += if args
                .get(index + 1)
                .is_some_and(|value| !value.starts_with("--"))
            {
                2
            } else {
                1
            };
            continue;
        }

        if is_solve_bool_flag(arg) {
            index += 1;
            continue;
        }

        return Err(format!("unknown solve flag: {arg}"));
    }

    Ok(())
}

fn is_solve_value_flag(arg: &str) -> bool {
    matches!(
        arg,
        "--repo" | "--task" | "--model" | "--base-url" | "--api-key-env" | "--max-steps"
    )
}

fn is_solve_bool_flag(arg: &str) -> bool {
    matches!(arg, "--dry-run" | "--info-only")
}

fn run_config_check(args: &[String]) -> Result<String, String> {
    let repo = config_check_repo(args)?;
    let config = PatchwrightConfig::load(&repo).map_err(|error| error.to_string())?;

    Ok(format!(
        "config: ok\nmodel base_url: {}\nagent max_steps: {}\npolicy allowed_programs: {}",
        config.model.base_url,
        config.agent.max_steps,
        config.policy.allowed_programs.join(",")
    ))
}

fn config_check_repo(args: &[String]) -> Result<PathBuf, String> {
    let mut index = 2;
    let mut repo = None;

    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "--repo" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("config check requires --repo <path>".to_owned());
                };
                if value.starts_with("--") {
                    return Err("config check requires --repo <path>".to_owned());
                }
                repo = Some(value.clone());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(format!("unknown config check flag: {value}"));
            }
            value => {
                return Err(format!("unexpected config check argument: {value}"));
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
    let adapter = rust_adapter(&config.rust);
    let repo_view = RepoView {
        root: sandbox_repo.clone(),
    };
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

fn rust_adapter(config: &RustConfig) -> RustAdapter {
    RustAdapter::new(config.fmt, config.check, config.test, config.clippy)
}

fn value_after(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .and_then(|window| {
            let value = &window[1];
            (!value.starts_with('-')).then(|| value.clone())
        })
}

fn optional_value_if_present(args: &[String], flag: &str) -> Result<Option<String>, String> {
    match value_after(args, flag) {
        Some(value) => Ok(Some(value)),
        None if has_flag(args, flag) => Err(format!("solve requires {flag} <value>")),
        None => Ok(None),
    }
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn accessible_repo_path(path: &str) -> Result<PathBuf, String> {
    let repo =
        fs::canonicalize(path).map_err(|_| format!("repo path is not accessible: {path}"))?;
    if !repo.is_dir() {
        return Err(format!("repo path is not a directory: {path}"));
    }
    Ok(repo)
}

fn print_help() {
    println!(
        "patchwright\n\nUSAGE:\n    patchwright --version\n    patchwright status\n    patchwright config check [--repo <path>]\n    patchwright bench startup\n    patchwright solve --repo <path> --task <text> [--dry-run] [--model <name>] [--base-url <url>] [--api-key-env <name>] [--max-steps <n>] [--info-only]\n    patchwright verify --repo <path>"
    );
}

#[cfg(test)]
mod tests {
    use super::run;
    use patchwright_config::RustConfig;
    use patchwright_core::traits::LanguageAdapter;
    use patchwright_core::types::{RepoView, TaskSpec};
    use patchwright_test_support::TempRepo;
    use std::path::PathBuf;

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
    fn config_check_default_succeeds() {
        let result = run(["config".to_owned(), "check".to_owned()]);
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn config_check_reads_repo_patchwright_toml() {
        let repo = TempRepo::new("cli-config-check-reads-config");
        repo.write(
            "patchwright.toml",
            "[model]\nbase_url = \"http://127.0.0.1:8080/v1\"\n[agent]\nmax_steps = 9\n[policy]\nallowed_programs = [\"cargo\", \"rustc\"]\n",
        );

        let output = super::run_config_check(&[
            "config".to_owned(),
            "check".to_owned(),
            "--repo".to_owned(),
            repo.root().to_string_lossy().into_owned(),
        ])
        .unwrap();

        assert!(output.contains("config: ok"));
        assert!(output.contains("model base_url: http://127.0.0.1:8080/v1"));
        assert!(output.contains("agent max_steps: 9"));
        assert!(output.contains("policy allowed_programs: cargo,rustc"));
    }

    #[test]
    fn config_check_rejects_file_repo_path() {
        let repo = TempRepo::new("cli-config-check-rejects-file-repo-path");
        repo.write("config-target.txt", "not a directory\n");
        let file = repo.root().join("config-target.txt");

        let result = run([
            "config".to_owned(),
            "check".to_owned(),
            "--repo".to_owned(),
            file.to_string_lossy().into_owned(),
        ]);

        assert!(matches!(result, Err(message) if message.contains("repo path is not a directory")));
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
    fn solve_dry_run_accepts_model_flags_without_network() {
        let repo = TempRepo::new("cli-solve-dry-run-model-flags");
        repo.write(
            "Cargo.toml",
            "[package]\nname = \"cli_solve_dry_run_model_flags\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        );
        repo.write("src/lib.rs", "pub fn ok() {}\n");
        repo.commit_all("seed valid rust crate");

        let result = run([
            "solve".to_owned(),
            "--repo".to_owned(),
            repo.root().to_string_lossy().into_owned(),
            "--task".to_owned(),
            "summarize".to_owned(),
            "--dry-run".to_owned(),
            "--model".to_owned(),
            "dry-test".to_owned(),
            "--base-url".to_owned(),
            "http://127.0.0.1:9".to_owned(),
            "--max-steps".to_owned(),
            "2".to_owned(),
            "--api-key-env".to_owned(),
            "PATCHWRIGHT_TEST_KEY".to_owned(),
        ]);

        assert_eq!(result, Ok(()));
    }

    #[test]
    fn solve_dry_run_uses_config_model_and_agent_defaults() {
        let repo = TempRepo::new("cli-solve-dry-run-config");
        repo.write(
            "Cargo.toml",
            "[package]\nname = \"cli_solve_dry_run_config\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        );
        repo.write("src/lib.rs", "pub fn ok() {}\n");
        repo.write(
            "patchwright.toml",
            "[model]\nbase_url = \"http://127.0.0.1:9/v1\"\nmodel = \"configured-dry-run\"\n[agent]\nmax_steps = 1\nrequire_patch = false\nmax_changed_files = 2\nmax_inserted_lines = 50\n",
        );
        repo.commit_all("seed valid rust crate with config");

        let options = super::solve_options(&[
            "solve".to_owned(),
            "--repo".to_owned(),
            repo.root().to_string_lossy().into_owned(),
            "--task".to_owned(),
            "summarize".to_owned(),
            "--dry-run".to_owned(),
        ])
        .unwrap();

        assert_eq!(options.model, Some("configured-dry-run".to_owned()));
        assert_eq!(options.base_url, "http://127.0.0.1:9/v1");
        assert_eq!(options.max_steps, 1);
        assert!(!options.require_patch);
        assert_eq!(options.max_changed_files, 2);
        assert_eq!(options.max_inserted_lines, 50);

        let result = run([
            "solve".to_owned(),
            "--repo".to_owned(),
            repo.root().to_string_lossy().into_owned(),
            "--task".to_owned(),
            "summarize".to_owned(),
            "--dry-run".to_owned(),
        ]);

        assert_eq!(result, Ok(()));
    }

    #[test]
    fn solve_cli_flags_override_config_defaults() {
        let repo = TempRepo::new("cli-solve-flags-override-config");
        repo.write(
            "patchwright.toml",
            "[model]\nbase_url = \"http://configured.invalid/v1\"\nmodel = \"configured-model\"\napi_key_env = \"CONFIGURED_KEY\"\n[agent]\nmax_steps = 3\n[policy]\nallowed_programs = [\"cargo\", \"rustc\"]\n",
        );

        let options = super::solve_options(&[
            "solve".to_owned(),
            "--repo".to_owned(),
            repo.root().to_string_lossy().into_owned(),
            "--task".to_owned(),
            "summarize".to_owned(),
            "--dry-run".to_owned(),
            "--model".to_owned(),
            "flag-model".to_owned(),
            "--base-url".to_owned(),
            "http://flag.invalid/v1".to_owned(),
            "--api-key-env".to_owned(),
            "FLAG_KEY".to_owned(),
            "--max-steps".to_owned(),
            "8".to_owned(),
        ])
        .unwrap();

        assert_eq!(options.model, Some("flag-model".to_owned()));
        assert_eq!(options.base_url, "http://flag.invalid/v1");
        assert_eq!(options.api_key_env, "FLAG_KEY");
        assert_eq!(options.max_steps, 8);
        assert_eq!(options.allowed_programs, vec!["cargo", "rustc"]);
    }

    #[test]
    fn rust_config_controls_verifier_plan_commands() {
        let adapter = super::rust_adapter(&RustConfig {
            fmt: false,
            check: true,
            test: false,
            clippy: true,
        });

        let task = TaskSpec::from_text(PathBuf::new(), "verify rust config");
        let repo = RepoView {
            root: PathBuf::new(),
        };
        let plan = adapter.verifier_plan(&task, &repo);
        let commands = plan
            .commands
            .iter()
            .map(|command| format!("{} {}", command.program, command.args.join(" ")))
            .collect::<Vec<_>>();

        assert_eq!(commands, vec!["cargo check", "cargo clippy -- -D warnings"]);
    }

    #[test]
    fn solve_real_mode_requires_model_name() {
        let result = run([
            "solve".to_owned(),
            "--repo".to_owned(),
            ".".to_owned(),
            "--task".to_owned(),
            "summarize".to_owned(),
        ]);

        assert_eq!(
            result,
            Err("solve real mode requires --model <name> or --dry-run".to_owned())
        );
    }

    #[test]
    fn solve_rejects_invalid_max_steps() {
        let result = run([
            "solve".to_owned(),
            "--repo".to_owned(),
            ".".to_owned(),
            "--task".to_owned(),
            "summarize".to_owned(),
            "--dry-run".to_owned(),
            "--max-steps".to_owned(),
            "0".to_owned(),
        ]);

        assert_eq!(
            result,
            Err("solve requires --max-steps to be a positive integer".to_owned())
        );
    }

    #[test]
    fn solve_rejects_equals_style_raw_api_key() {
        let result = run([
            "solve".to_owned(),
            "--repo".to_owned(),
            ".".to_owned(),
            "--task".to_owned(),
            "summarize".to_owned(),
            "--dry-run".to_owned(),
            "--api-key=sk-test-secret".to_owned(),
        ]);

        assert_eq!(
            result,
            Err("raw API keys are not accepted; use --api-key-env <name>".to_owned())
        );
    }

    #[test]
    fn solve_rejects_unknown_flag() {
        let result = run([
            "solve".to_owned(),
            "--repo".to_owned(),
            ".".to_owned(),
            "--task".to_owned(),
            "summarize".to_owned(),
            "--dry-run".to_owned(),
            "--unknown-flag".to_owned(),
        ]);

        assert_eq!(result, Err("unknown solve flag: --unknown-flag".to_owned()));
    }

    #[test]
    fn solve_rejects_unknown_equals_flag() {
        let result = run([
            "solve".to_owned(),
            "--repo".to_owned(),
            ".".to_owned(),
            "--task".to_owned(),
            "summarize".to_owned(),
            "--dry-run".to_owned(),
            "--base-urll=http://example.invalid".to_owned(),
        ]);

        assert_eq!(
            result,
            Err("unknown solve flag: --base-urll=http://example.invalid".to_owned())
        );
    }

    #[test]
    fn solve_rejects_value_after_boolean_flag() {
        let result = run([
            "solve".to_owned(),
            "--repo".to_owned(),
            ".".to_owned(),
            "--task".to_owned(),
            "summarize".to_owned(),
            "--dry-run".to_owned(),
            "false".to_owned(),
        ]);

        assert_eq!(result, Err("unexpected solve argument: false".to_owned()));
    }

    #[test]
    fn solve_rejects_value_after_info_only_flag() {
        let result = run([
            "solve".to_owned(),
            "--repo".to_owned(),
            ".".to_owned(),
            "--task".to_owned(),
            "summarize".to_owned(),
            "--dry-run".to_owned(),
            "--info-only".to_owned(),
            "false".to_owned(),
        ]);

        assert_eq!(result, Err("unexpected solve argument: false".to_owned()));
    }

    #[test]
    fn solve_rejects_trailing_operand() {
        let result = run([
            "solve".to_owned(),
            "--repo".to_owned(),
            ".".to_owned(),
            "--task".to_owned(),
            "summarize".to_owned(),
            "--dry-run".to_owned(),
            "extra".to_owned(),
        ]);

        assert_eq!(result, Err("unexpected solve argument: extra".to_owned()));
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
    fn verify_runs_real_cargo_checks() {
        let repo = TempRepo::new("cli-verify-runs-checks");
        repo.write(
            "Cargo.toml",
            "[package]\nname = \"cli_verify_runs_checks\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        );
        repo.write("src/lib.rs", "pub fn broken( {\n");
        repo.commit_all("seed invalid rust crate");

        let result = run([
            "verify".to_owned(),
            "--repo".to_owned(),
            repo.root().to_string_lossy().into_owned(),
        ]);

        assert!(matches!(result, Err(message) if message.contains("verification rejected")));
    }

    #[test]
    fn verify_respects_configured_allowed_programs() {
        let repo = TempRepo::new("cli-verify-denies-cargo");
        repo.write(
            "Cargo.toml",
            "[package]\nname = \"cli_verify_denies_cargo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        );
        repo.write("src/lib.rs", "pub fn ok() {}\n");
        repo.write(
            "patchwright.toml",
            "[policy]\nallowed_programs = [\"git\"]\n",
        );
        repo.commit_all("seed valid rust crate with restrictive config");

        let result = run([
            "verify".to_owned(),
            "--repo".to_owned(),
            repo.root().to_string_lossy().into_owned(),
        ]);

        assert!(matches!(result, Err(message) if message.contains("verification rejected")));
    }

    #[test]
    fn startup_benchmark_reports_micros_not_nanos() {
        assert_eq!(super::startup_average_micros(40_000, 20), 2);
    }
}
