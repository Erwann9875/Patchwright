use super::run;
use patchwright_config::{ModelProviderKind, RustConfig};
use patchwright_core::agent::{SolveReport, SolveStatus};
use patchwright_core::traits::LanguageAdapter;
use patchwright_core::types::{
    Attempt, CheckReport, Counterexample, DiffSummary, RepoPath, RepoView, TaskSpec,
    VerificationReport, VerificationStatus,
};
use patchwright_test_support::TempRepo;
use std::fs;
use std::path::{Path, PathBuf};

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
    assert!(output.contains("model provider: codex-cli"));
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
fn auth_login_invokes_configured_codex_login() {
    let repo = TempRepo::new("cli-auth-login");
    let script = fake_auth_codex_script(repo.root(), true);
    repo.write(
        "patchwright.toml",
        &format!(
            "[model.codex_cli]\ncommand = \"{}\"\n",
            toml_escape_path(&script)
        ),
    );

    let result = run([
        "auth".to_owned(),
        "login".to_owned(),
        "--repo".to_owned(),
        repo.root().to_string_lossy().into_owned(),
    ]);

    assert_eq!(result, Ok(()));
    let args = fs::read_to_string(repo.root().join("auth-args.txt")).unwrap();
    assert!(args.contains("login"));
}

#[test]
fn auth_check_fails_clearly_when_codex_is_missing() {
    let repo = TempRepo::new("cli-auth-check-missing");
    repo.write(
        "patchwright.toml",
        "[model.codex_cli]\ncommand = \"missing-patchwright-codex-test-bin\"\n",
    );

    let result = run([
        "auth".to_owned(),
        "check".to_owned(),
        "--repo".to_owned(),
        repo.root().to_string_lossy().into_owned(),
    ]);

    assert!(
        matches!(result, Err(ref message) if message.contains("failed to start codex command")),
        "unexpected result: {result:?}"
    );
}

#[test]
fn auth_check_succeeds_with_structured_codex_output() {
    let repo = TempRepo::new("cli-auth-check-ok");
    let script = fake_auth_codex_script(repo.root(), true);
    repo.write(
        "patchwright.toml",
        &format!(
            "[model.codex_cli]\ncommand = \"{}\"\n",
            toml_escape_path(&script)
        ),
    );

    let result = run([
        "auth".to_owned(),
        "check".to_owned(),
        "--repo".to_owned(),
        repo.root().to_string_lossy().into_owned(),
    ]);

    assert_eq!(result, Ok(()));
    let args = fs::read_to_string(repo.root().join("auth-args.txt")).unwrap();
    assert!(args.contains("exec"));
    assert!(args.contains("--output-schema"));
}

#[test]
fn auth_check_fails_when_codex_exec_cannot_use_login() {
    let repo = TempRepo::new("cli-auth-check-not-logged-in");
    let script = fake_auth_codex_script(repo.root(), false);
    repo.write(
        "patchwright.toml",
        &format!(
            "[model.codex_cli]\ncommand = \"{}\"\n",
            toml_escape_path(&script)
        ),
    );

    let result = run([
        "auth".to_owned(),
        "check".to_owned(),
        "--repo".to_owned(),
        repo.root().to_string_lossy().into_owned(),
    ]);

    assert!(
        matches!(result, Err(ref message) if message.contains("codex auth check failed")),
        "unexpected result: {result:?}"
    );
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

    assert_eq!(options.model_provider, ModelProviderKind::CodexCli);
    assert_eq!(options.openai.model, "configured-dry-run");
    assert_eq!(options.openai.base_url, "http://127.0.0.1:9/v1");
    assert_eq!(
        options.codex_cli.model,
        Some("configured-dry-run".to_owned())
    );
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

    assert_eq!(options.model_provider, ModelProviderKind::OpenAiCompatible);
    assert_eq!(options.openai.model, "flag-model");
    assert_eq!(options.openai.base_url, "http://flag.invalid/v1");
    assert_eq!(options.openai.api_key_env, "FLAG_KEY");
    assert_eq!(options.max_steps, 8);
    assert_eq!(options.allowed_programs, vec!["cargo", "rustc"]);
}

#[test]
fn solve_defaults_to_codex_cli_provider_without_api_key_config() {
    let repo = TempRepo::new("cli-solve-defaults-to-codex-cli");
    let options = super::solve_options(&[
        "solve".to_owned(),
        "--repo".to_owned(),
        repo.root().to_string_lossy().into_owned(),
        "--task".to_owned(),
        "summarize".to_owned(),
    ])
    .unwrap();

    assert_eq!(options.model_provider, ModelProviderKind::CodexCli);
    assert_eq!(options.codex_cli.command, "codex");
    assert_eq!(options.codex_cli.model, None);
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
fn openai_compatible_real_mode_requires_model_name() {
    let result = run([
        "solve".to_owned(),
        "--repo".to_owned(),
        ".".to_owned(),
        "--task".to_owned(),
        "summarize".to_owned(),
        "--model-provider".to_owned(),
        "openai-compatible".to_owned(),
    ]);

    assert_eq!(
        result,
        Err("OpenAI-compatible solve requires --model <name> or model.openai.model".to_owned())
    );
}

#[test]
fn solve_rejects_openai_flags_with_explicit_codex_provider() {
    let result = run([
        "solve".to_owned(),
        "--repo".to_owned(),
        ".".to_owned(),
        "--task".to_owned(),
        "summarize".to_owned(),
        "--model-provider".to_owned(),
        "codex-cli".to_owned(),
        "--base-url".to_owned(),
        "http://example.invalid/v1".to_owned(),
    ]);

    assert_eq!(
        result,
        Err("OpenAI-compatible flags require --model-provider openai-compatible".to_owned())
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
fn verify_uses_configured_commands_without_rust_adapter() {
    let repo = TempRepo::new("cli-verify-generic-command");
    repo.write("package.json", "{}\n");
    let script = fake_verify_script(repo.root(), true);
    let script_text = toml_escape_path(&script);
    repo.write(
        "patchwright.toml",
        &format!(
            "[policy]\nallowed_programs = [\"{script_text}\"]\n\n[[verify.commands]]\nname = \"test\"\ncommand = \"{script_text}\"\n"
        ),
    );
    repo.commit_all("seed generic verification repo");

    let result = run([
        "verify".to_owned(),
        "--repo".to_owned(),
        repo.root().to_string_lossy().into_owned(),
    ]);

    assert_eq!(result, Ok(()));
}

#[test]
fn verify_denies_unallowlisted_configured_commands() {
    let repo = TempRepo::new("cli-verify-generic-command-denied");
    repo.write("package.json", "{}\n");
    let script = fake_verify_script(repo.root(), true);
    let script_text = toml_escape_path(&script);
    repo.write(
        "patchwright.toml",
        &format!(
            "[policy]\nallowed_programs = [\"cargo\"]\n\n[[verify.commands]]\nname = \"test\"\ncommand = \"{script_text}\"\n"
        ),
    );
    repo.commit_all("seed denied generic verification repo");

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

#[test]
fn solve_report_for_accepted_patch_shows_changed_files_and_checks() {
    let report = SolveReport {
        status: SolveStatus::Accepted,
        summary: "Fixed add implementation from subtraction to addition.".to_owned(),
        attempts: vec![Attempt {
            patch_id: None,
            verification: VerificationReport {
                status: VerificationStatus::Accepted,
                checks: vec![
                    CheckReport {
                        name: "cargo fmt -- --check".to_owned(),
                        command: None,
                        passed: true,
                        summary: "passed".to_owned(),
                    },
                    CheckReport {
                        name: "cargo test".to_owned(),
                        command: None,
                        passed: true,
                        summary: "passed".to_owned(),
                    },
                ],
                counterexamples: Vec::new(),
                diff_summary: DiffSummary {
                    changed_files: vec![RepoPath::new("src/lib.rs")],
                    inserted_lines: 1,
                    deleted_lines: 1,
                },
                policy_events: Vec::new(),
            },
        }],
        observations: Vec::new(),
        counterexamples: Vec::new(),
    };

    let output = super::render_solve_report(&report);

    assert!(output.contains("Patchwright solve result"));
    assert!(output.contains("Status: Accepted"));
    assert!(output.contains("Changed files:\n  src/lib.rs"));
    assert!(output.contains("Verification:\n  [ok] cargo fmt -- --check"));
    assert!(output.contains("  [ok] cargo test"));
    assert!(output.contains("Attempts: 1"));
    assert!(output.contains("Summary:\n  Fixed add implementation"));
}

#[test]
fn solve_report_for_rejection_shows_first_counterexample() {
    let report = SolveReport {
        status: SolveStatus::BudgetExhausted,
        summary: "step budget exhausted".to_owned(),
        attempts: Vec::new(),
        observations: Vec::new(),
        counterexamples: vec![Counterexample {
            source: "cargo".to_owned(),
            detail: "assertion failed: expected 5, got -1\nsecond line".to_owned(),
        }],
    };

    let output = super::render_solve_report(&report);

    assert!(output.contains("Status: BudgetExhausted"));
    assert!(output.contains("Attempts: 0"));
    assert!(output.contains("Counterexamples:"));
    assert!(output.contains("assertion failed: expected 5, got -1"));
    assert!(!output.contains("second line"));
}

#[test]
fn applies_sandbox_diff_back_to_source_repo() {
    let repo = TempRepo::new("cli-apply-sandbox-diff");
    repo.write("src/lib.rs", "pub fn value() -> i32 { 1 }\n");
    repo.commit_all("seed source");
    let sandbox = patchwright_exec_local::GitWorktreeSandbox::create(repo.root()).unwrap();

    fs::write(
        sandbox.root().join("src/lib.rs"),
        "pub fn value() -> i32 { 2 }\n",
    )
    .unwrap();

    super::apply_sandbox_diff_to_source(sandbox.root(), repo.root()).unwrap();

    assert!(fs::read_to_string(repo.root().join("src/lib.rs"))
        .unwrap()
        .contains("pub fn value() -> i32 { 2 }"));
}

#[test]
fn design_dry_run_writes_markdown_plan_without_source_changes() {
    let repo = TempRepo::new("cli-design-dry-run");
    repo.write(
        "Cargo.toml",
        "[package]\nname = \"cli_design_dry_run\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    repo.write("src/lib.rs", "pub fn value() -> i32 { 1 }\n");
    repo.commit_all("seed source");

    let result = run([
        "design".to_owned(),
        "--repo".to_owned(),
        repo.root().to_string_lossy().into_owned(),
        "--task".to_owned(),
        "Add team billing".to_owned(),
        "--dry-run".to_owned(),
    ]);

    assert_eq!(result, Ok(()));
    assert_eq!(
        fs::read_to_string(repo.root().join("src/lib.rs")).unwrap(),
        "pub fn value() -> i32 { 1 }\n"
    );

    let plans_dir = repo.root().join("docs/patchwright/plans");
    let plans = fs::read_dir(plans_dir)
        .unwrap()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(plans.len(), 1);

    let content = fs::read_to_string(plans[0].path()).unwrap();
    assert!(content.contains("# Feature Design: Add Team Billing"));
    assert!(content.contains("## Current Architecture"));
    assert!(content.contains("## Implementation Plan"));
    assert!(content.contains("src/lib.rs"));
}

#[test]
fn implement_options_load_selected_plan_step_as_scoped_task() {
    let repo = TempRepo::new("cli-implement-options");
    repo.write(
        "Cargo.toml",
        "[package]\nname = \"cli_implement_options\"\n",
    );
    repo.write("src/lib.rs", "pub fn ok() {}\n");
    repo.write(
        "docs/patchwright/plans/feature.md",
        implementation_graph_markdown(),
    );

    let (options, scope) = super::implement_options(&[
        "implement".to_owned(),
        "--repo".to_owned(),
        repo.root().to_string_lossy().into_owned(),
        "--from".to_owned(),
        "docs/patchwright/plans/feature.md".to_owned(),
        "--step".to_owned(),
        "step-1".to_owned(),
        "--dry-run".to_owned(),
    ])
    .unwrap();

    assert!(options.task.contains("Step ID: step-1"));
    assert!(options.task.contains("Title: Add domain type"));
    assert!(!options.task.contains("Wire API"));
    assert!(options.require_patch);
    assert_eq!(
        scope.unwrap().target_files,
        vec![RepoPath::new("src/domain.rs")]
    );
}

#[test]
fn implement_command_can_run_info_only_dry_run_from_plan() {
    let repo = TempRepo::new("cli-implement-dry-run");
    repo.write(
        "Cargo.toml",
        "[package]\nname = \"cli_implement_dry_run\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    repo.write("src/lib.rs", "pub fn ok() {}\n");
    repo.write(
        "docs/patchwright/plans/feature.md",
        implementation_graph_markdown(),
    );
    repo.commit_all("seed implementation plan");

    let result = run([
        "implement".to_owned(),
        "--repo".to_owned(),
        repo.root().to_string_lossy().into_owned(),
        "--from".to_owned(),
        "docs/patchwright/plans/feature.md".to_owned(),
        "--step".to_owned(),
        "step-1".to_owned(),
        "--dry-run".to_owned(),
        "--info-only".to_owned(),
    ]);

    assert_eq!(result, Ok(()));
}

#[test]
fn implement_scope_detects_changed_files_outside_selected_step() {
    let report = SolveReport {
        status: SolveStatus::Accepted,
        summary: "accepted".to_owned(),
        attempts: vec![Attempt {
            patch_id: None,
            verification: VerificationReport {
                status: VerificationStatus::Accepted,
                checks: Vec::new(),
                counterexamples: Vec::new(),
                diff_summary: DiffSummary {
                    changed_files: vec![
                        RepoPath::new("src/domain.rs"),
                        RepoPath::new("src/api.rs"),
                    ],
                    inserted_lines: 2,
                    deleted_lines: 0,
                },
                policy_events: Vec::new(),
            },
        }],
        observations: Vec::new(),
        counterexamples: Vec::new(),
    };

    let scope = super::StepScope {
        target_files: vec![RepoPath::new("src/domain.rs")],
    };

    assert_eq!(
        super::out_of_scope_changed_files(&report, &scope),
        vec![RepoPath::new("src/api.rs")]
    );
}

#[test]
fn profile_command_runs_without_model_call() {
    let repo = TempRepo::new("cli-profile");
    repo.write("Cargo.toml", "[package]\nname = \"cli_profile\"\n");
    repo.write("src/lib.rs", "pub fn ok() {}\n");
    repo.write("tests/smoke.rs", "#[test]\nfn smoke() {}\n");

    let result = run([
        "profile".to_owned(),
        "--repo".to_owned(),
        repo.root().to_string_lossy().into_owned(),
    ]);

    assert_eq!(result, Ok(()));
}

#[test]
fn profile_renderer_shows_detected_languages_and_commands() {
    let repo = TempRepo::new("cli-profile-render");
    repo.write("package.json", "{}\n");
    repo.write("tsconfig.json", "{}\n");
    repo.write("src/index.ts", "export const ok = true;\n");

    let profile = patchwright_index::profile_project(repo.root()).unwrap();
    let output = super::render_project_profile(&profile);

    assert!(output.contains("Patchwright project profile"));
    assert!(output.contains("Detected languages:\n  TypeScript\n  JavaScript"));
    assert!(output.contains("Package managers:\n  npm"));
    assert!(output.contains("Likely test commands:\n  npm test"));
    assert!(output.contains("Source roots:\n  src"));
}

#[test]
fn review_report_flags_source_diff_without_tests() {
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-pub fn value() -> i32 { 1 }\n+pub fn value() -> i32 { 2 }\n";

    let report = super::build_review_report(diff);
    let output = super::render_review_report(&report);

    assert_eq!(report.risk_level, "medium");
    assert!(output.contains("Patchwright review result"));
    assert!(output.contains("Test coverage"));
    assert!(output.contains("src/lib.rs changed without nearby test changes"));
    assert!(output.contains("Recommended next actions"));
}

#[test]
fn review_report_accepts_empty_diff() {
    let report = super::build_review_report("");
    let output = super::render_review_report(&report);

    assert_eq!(report.risk_level, "low");
    assert!(output.contains("No diff to review."));
    assert!(output.contains("Blocking issues:\n  none"));
}

#[test]
fn review_command_reads_diff_without_modifying_code() {
    let repo = TempRepo::new("cli-review-diff");
    repo.write(
        "Cargo.toml",
        "[package]\nname = \"cli_review_diff\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    repo.write("src/lib.rs", "pub fn value() -> i32 { 1 }\n");
    repo.commit_all("seed review repo");
    repo.write("src/lib.rs", "pub fn value() -> i32 { 2 }\n");

    let result = run([
        "review".to_owned(),
        "--repo".to_owned(),
        repo.root().to_string_lossy().into_owned(),
        "--diff".to_owned(),
    ]);

    assert_eq!(result, Ok(()));
    assert_eq!(
        fs::read_to_string(repo.root().join("src/lib.rs")).unwrap(),
        "pub fn value() -> i32 { 2 }\n"
    );
}

fn toml_escape_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "\\\\")
}

fn implementation_graph_markdown() -> &'static str {
    r#"# Implementation Graph

## step-1. Add domain type

Introduce the smallest domain model first.

Depends on: none

Target files:
- `src/domain.rs`

Acceptance criteria:
- Domain type compiles.

Verification commands:
- cargo test domain

## step-2. Wire API

Use the domain type in the API boundary.

Depends on: step-1

Target files:
- `src/api.rs`

Acceptance criteria:
- API tests pass.

Verification commands:
- cargo test api
"#
}

#[cfg(windows)]
fn fake_verify_script(root: &Path, succeeds: bool) -> PathBuf {
    let script = root.join(if succeeds {
        "fake-verify-ok.cmd"
    } else {
        "fake-verify-fail.cmd"
    });
    let exit_code = if succeeds { 0 } else { 1 };
    fs::write(
        &script,
        format!(
            r#"@echo off
echo verified>"{}"
exit /b {}
"#,
            root.join("verified.txt").display(),
            exit_code
        ),
    )
    .expect("fake verify script should be written");
    script
}

#[cfg(not(windows))]
fn fake_verify_script(root: &Path, succeeds: bool) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let script = root.join(if succeeds {
        "fake-verify-ok"
    } else {
        "fake-verify-fail"
    });
    let exit_code = if succeeds { 0 } else { 1 };
    fs::write(
        &script,
        format!(
            r#"#!/bin/sh
printf '%s\n' verified > "{}"
exit {}
"#,
            root.join("verified.txt").display(),
            exit_code
        ),
    )
    .expect("fake verify script should be written");
    let mut permissions = fs::metadata(&script)
        .expect("fake verify script metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&script, permissions).expect("fake verify script should be executable");
    script
}

#[cfg(windows)]
fn fake_auth_codex_script(root: &Path, succeeds: bool) -> PathBuf {
    let script = root.join(if succeeds {
        "fake-auth-codex-ok.cmd"
    } else {
        "fake-auth-codex-fail.cmd"
    });
    let exit_code = if succeeds { 0 } else { 1 };
    let action = if succeeds {
        r#"echo {"status":"ok"}>"%output%""#
    } else {
        r#"echo not logged in 1>&2"#
    };
    fs::write(
        &script,
        format!(
            r#"@echo off
setlocal
echo %*>"{}"
if "%~1"=="login" exit /b 0
set output=
:loop
if "%~1"=="" goto after_args
if "%~1"=="-o" set output=%~2
shift
goto loop
:after_args
more > "{}"
{}
exit /b {}
"#,
            root.join("auth-args.txt").display(),
            root.join("auth-stdin.txt").display(),
            action,
            exit_code
        ),
    )
    .expect("fake auth codex script should be written");
    script
}

#[cfg(not(windows))]
fn fake_auth_codex_script(root: &Path, succeeds: bool) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let script = root.join(if succeeds {
        "fake-auth-codex-ok"
    } else {
        "fake-auth-codex-fail"
    });
    let exit_code = if succeeds { 0 } else { 1 };
    let write_status = if succeeds {
        r#"printf '%s\n' '{"status":"ok"}' > "$output""#
    } else {
        r#"printf '%s\n' 'not logged in' >&2"#
    };
    fs::write(
        &script,
        format!(
            r#"#!/bin/sh
printf '%s\n' "$*" > "{}"
if [ "$1" = "login" ]; then
  exit 0
fi
output=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "-o" ]; then
shift
output="$1"
  fi
  shift
done
cat > "{}"
{}
exit {}
"#,
            root.join("auth-args.txt").display(),
            root.join("auth-stdin.txt").display(),
            write_status,
            exit_code
        ),
    )
    .expect("fake auth codex script should be written");
    let mut permissions = fs::metadata(&script)
        .expect("fake auth codex metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&script, permissions).expect("fake auth codex should be executable");
    script
}
