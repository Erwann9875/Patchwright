#![forbid(unsafe_code)]

use patchwright_config::{ModelProviderKind, PatchwrightConfig, RustConfig};
use patchwright_core::agent::{Agent, SolveReport, SolveStatus};
use patchwright_core::error::Result as PatchwrightResult;
use patchwright_core::policy::Policy;
use patchwright_core::traits::{Indexer, LanguageAdapter, ModelProvider, Verifier};
use patchwright_core::types::{
    ArchitectureDesign, DesignOption, EvidenceRef, FileImpact, ModelRequest, ModelResponse,
    PlanStep, RepoView, Risk, TaskSpec, VerificationStatus,
};
use patchwright_exec_local::{GitWorktreeSandbox, LocalExecution};
use patchwright_index::{profile_project, BasicIndexer, ProjectProfile};
use patchwright_lang_rust::RustAdapter;
use patchwright_model_codex_cli::{CodexCliClient, CodexCliConfig};
use patchwright_model_openai::{OpenAiCompatibleClient, OpenAiConfig};
use patchwright_verify::PlanVerifier;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

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
        "auth" if args.get(1).map(String::as_str) == Some("login") => run_auth_login(&args),
        "auth" if args.get(1).map(String::as_str) == Some("check") => run_auth_check(&args),
        "bench" if args.get(1).map(String::as_str) == Some("startup") => run_startup_bench(),
        "profile" => run_profile(&args),
        "design" => run_design(&args),
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

fn render_project_profile(profile: &ProjectProfile) -> String {
    let mut output = String::new();

    output.push_str("Patchwright project profile\n\n");
    push_plain_list(
        &mut output,
        "Detected languages",
        &profile.detected_languages,
    );
    let manifests = repo_paths_as_strings(&profile.manifests);
    push_plain_list(&mut output, "Manifests", &manifests);
    push_plain_list(&mut output, "Package managers", &profile.package_managers);
    push_plain_list(
        &mut output,
        "Likely install commands",
        &profile.install_commands,
    );
    push_plain_list(
        &mut output,
        "Likely build commands",
        &profile.build_commands,
    );
    push_plain_list(&mut output, "Likely test commands", &profile.test_commands);
    push_plain_list(
        &mut output,
        "Likely typecheck commands",
        &profile.typecheck_commands,
    );
    push_plain_list(&mut output, "Likely lint commands", &profile.lint_commands);
    let source_roots = repo_paths_as_strings(&profile.source_roots);
    push_plain_list(&mut output, "Source roots", &source_roots);
    let test_roots = repo_paths_as_strings(&profile.test_roots);
    push_plain_list(&mut output, "Test roots", &test_roots);
    let ci_files = repo_paths_as_strings(&profile.ci_files);
    push_plain_list(&mut output, "CI files", &ci_files);

    output
}

fn push_plain_list(output: &mut String, heading: &str, items: &[String]) {
    output.push_str(&format!("{heading}:\n"));
    if items.is_empty() {
        output.push_str("  none\n\n");
        return;
    }
    for item in items {
        output.push_str(&format!("  {item}\n"));
    }
    output.push('\n');
}

fn repo_paths_as_strings(paths: &[patchwright_core::types::RepoPath]) -> Vec<String> {
    paths.iter().map(|path| path.0.clone()).collect()
}

fn run_solve(args: &[String]) -> Result<(), String> {
    let options = solve_options(args)?;
    let model = cli_model(&options)?;

    let sandbox = GitWorktreeSandbox::create(&options.repo).map_err(|error| error.to_string())?;
    let sandbox_repo = sandbox.root().to_path_buf();
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
            TaskSpec::from_text(sandbox_repo.clone(), options.task)
                .with_require_patch(options.require_patch),
        )
        .map_err(|error| error.to_string())?;
    if report.status == SolveStatus::Accepted {
        apply_sandbox_diff_to_source(&sandbox_repo, &options.repo)?;
    }
    println!("{}", render_solve_report(&report));

    Ok(())
}

fn apply_sandbox_diff_to_source(sandbox_repo: &Path, source_repo: &Path) -> Result<(), String> {
    let add_intent = Command::new("git")
        .arg("-C")
        .arg(sandbox_repo)
        .args(["add", "-N", "--", "."])
        .output()
        .map_err(|error| error.to_string())?;
    if !add_intent.status.success() {
        return Err(format!(
            "failed to prepare accepted patch diff: {}",
            String::from_utf8_lossy(&add_intent.stderr)
        ));
    }

    let diff = Command::new("git")
        .arg("-C")
        .arg(sandbox_repo)
        .args(["diff", "--binary", "--"])
        .output()
        .map_err(|error| error.to_string())?;
    if !diff.status.success() {
        return Err(format!(
            "failed to export accepted patch diff: {}",
            String::from_utf8_lossy(&diff.stderr)
        ));
    }
    if diff.stdout.is_empty() {
        return Ok(());
    }

    let mut apply = Command::new("git")
        .arg("-C")
        .arg(source_repo)
        .args(["apply", "--whitespace=nowarn"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    let mut stdin = apply
        .stdin
        .take()
        .ok_or_else(|| "failed to open git apply stdin".to_owned())?;
    stdin
        .write_all(&diff.stdout)
        .map_err(|error| error.to_string())?;
    drop(stdin);

    let output = apply
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "failed to apply accepted patch to source repo: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
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

fn write_design_document(
    repo: &Path,
    task: &str,
    design: &ArchitectureDesign,
) -> Result<PathBuf, String> {
    let plans_dir = repo.join("docs").join("patchwright").join("plans");
    fs::create_dir_all(&plans_dir).map_err(|error| error.to_string())?;
    let path = plans_dir.join(format!(
        "{}-{}.md",
        current_date_string(),
        slugify_task(task)
    ));
    fs::write(&path, render_architecture_design_markdown(design))
        .map_err(|error| error.to_string())?;
    Ok(path)
}

fn render_architecture_design_markdown(design: &ArchitectureDesign) -> String {
    let mut output = String::new();

    output.push_str(&format!("# {}\n\n", design.title));
    push_section(&mut output, "Goal", &design.goal);
    push_findings(
        &mut output,
        "Current Architecture",
        &design.current_architecture,
    );
    push_inspected_files(&mut output, design);
    push_list(&mut output, "Assumptions", &design.assumptions);
    push_list(&mut output, "Non-goals", &design.non_goals);
    push_options(&mut output, &design.options);
    push_recommendation(&mut output, design);
    push_file_impact(&mut output, &design.file_impact);
    push_plan_steps(
        &mut output,
        "Implementation Plan",
        &design.implementation_plan,
    );
    push_test_strategy(&mut output, design);
    push_optional_section(
        &mut output,
        "Migration Plan",
        design.migration_plan.as_deref(),
    );
    push_optional_section(
        &mut output,
        "Rollback Plan",
        design.rollback_plan.as_deref(),
    );
    push_risks(&mut output, &design.risks);
    push_list(&mut output, "Open Questions", &design.open_questions);
    push_list(
        &mut output,
        "Acceptance Criteria",
        &design.acceptance_criteria,
    );

    output
}

fn push_section(output: &mut String, heading: &str, content: &str) {
    output.push_str(&format!("## {heading}\n\n{}\n\n", blank_if_empty(content)));
}

fn push_optional_section(output: &mut String, heading: &str, content: Option<&str>) {
    push_section(output, heading, content.unwrap_or("None."));
}

fn push_list(output: &mut String, heading: &str, items: &[String]) {
    output.push_str(&format!("## {heading}\n\n"));
    if items.is_empty() {
        output.push_str("- None.\n\n");
        return;
    }
    for item in items {
        output.push_str(&format!("- {}\n", blank_if_empty(item)));
    }
    output.push('\n');
}

fn push_findings(
    output: &mut String,
    heading: &str,
    findings: &[patchwright_core::types::ArchitectureFinding],
) {
    output.push_str(&format!("## {heading}\n\n"));
    if findings.is_empty() {
        output.push_str("- None.\n\n");
        return;
    }
    for finding in findings {
        output.push_str(&format!("- {}\n", blank_if_empty(&finding.summary)));
        push_evidence(output, &finding.evidence);
    }
    output.push('\n');
}

fn push_inspected_files(output: &mut String, design: &ArchitectureDesign) {
    output.push_str("## Relevant Files Inspected\n\n");
    let mut files = Vec::new();
    collect_evidence_paths(&mut files, &design.recommendation.evidence);
    for finding in &design.current_architecture {
        collect_evidence_paths(&mut files, &finding.evidence);
    }
    for option in &design.options {
        collect_evidence_paths(&mut files, &option.evidence);
    }
    for impact in &design.file_impact {
        if !files.contains(&impact.path.0) {
            files.push(impact.path.0.clone());
        }
        collect_evidence_paths(&mut files, &impact.evidence);
    }
    for risk in &design.risks {
        collect_evidence_paths(&mut files, &risk.evidence);
    }

    if files.is_empty() {
        output.push_str("- None.\n\n");
        return;
    }
    for file in files {
        output.push_str(&format!("- `{file}`\n"));
    }
    output.push('\n');
}

fn collect_evidence_paths(files: &mut Vec<String>, evidence: &[EvidenceRef]) {
    for item in evidence {
        if !files.contains(&item.path.0) {
            files.push(item.path.0.clone());
        }
    }
}

fn push_options(output: &mut String, options: &[DesignOption]) {
    output.push_str("## Design Options\n\n");
    if options.is_empty() {
        output.push_str("- None.\n\n");
        return;
    }
    for option in options {
        output.push_str(&format!("### {}\n\n{}\n\n", option.name, option.summary));
        push_list(output, "Pros", &option.pros);
        push_list(output, "Cons", &option.cons);
        push_evidence(output, &option.evidence);
        output.push('\n');
    }
}

fn push_recommendation(output: &mut String, design: &ArchitectureDesign) {
    output.push_str("## Recommended Design\n\n");
    output.push_str(&format!(
        "**{}**\n\n{}\n",
        design.recommendation.option_name, design.recommendation.rationale
    ));
    push_evidence(output, &design.recommendation.evidence);
    output.push('\n');
}

fn push_file_impact(output: &mut String, impacts: &[FileImpact]) {
    output.push_str("## File Impact\n\n");
    if impacts.is_empty() {
        output.push_str("- None.\n\n");
        return;
    }
    for impact in impacts {
        output.push_str(&format!("- `{}`: {}", impact.path.0, impact.change_summary));
        if let Some(risk) = &impact.risk {
            output.push_str(&format!(" Risk: {risk}."));
        }
        output.push('\n');
        push_evidence(output, &impact.evidence);
    }
    output.push('\n');
}

fn push_plan_steps(output: &mut String, heading: &str, steps: &[PlanStep]) {
    output.push_str(&format!("## {heading}\n\n"));
    if steps.is_empty() {
        output.push_str("- None.\n\n");
        return;
    }
    for step in steps {
        output.push_str(&format!(
            "### {}. {}\n\n{}\n\n",
            step.id, step.title, step.description
        ));
        push_list(output, "Depends On", &step.depends_on);
        let target_files = step
            .target_files
            .iter()
            .map(|path| path.0.clone())
            .collect::<Vec<_>>();
        push_list(output, "Target Files", &target_files);
        push_list(
            output,
            "Step Acceptance Criteria",
            &step.acceptance_criteria,
        );
        push_list(output, "Verification Commands", &step.verification_commands);
    }
}

fn push_test_strategy(output: &mut String, design: &ArchitectureDesign) {
    output.push_str("## Test Strategy\n\n");
    push_list(output, "Unit", &design.test_strategy.unit);
    push_list(output, "Integration", &design.test_strategy.integration);
    push_list(output, "End To End", &design.test_strategy.end_to_end);
    push_list(output, "Manual", &design.test_strategy.manual);
    push_list(output, "Commands", &design.test_strategy.commands);
}

fn push_risks(output: &mut String, risks: &[Risk]) {
    output.push_str("## Risks\n\n");
    if risks.is_empty() {
        output.push_str("- None.\n\n");
        return;
    }
    for risk in risks {
        output.push_str(&format!(
            "- **{}**: {} Mitigation: {}\n",
            risk.title, risk.impact, risk.mitigation
        ));
        push_evidence(output, &risk.evidence);
    }
    output.push('\n');
}

fn push_evidence(output: &mut String, evidence: &[EvidenceRef]) {
    if evidence.is_empty() {
        return;
    }
    output.push_str("  Evidence:\n");
    for item in evidence {
        let line_suffix = match (item.start_line, item.end_line) {
            (Some(start), Some(end)) => format!(":{start}-{end}"),
            (Some(start), None) => format!(":{start}"),
            _ => String::new(),
        };
        output.push_str(&format!(
            "  - `{}`{}: {}\n",
            item.path.0, line_suffix, item.reason
        ));
    }
}

fn blank_if_empty(value: &str) -> &str {
    if value.trim().is_empty() {
        "None."
    } else {
        value.trim()
    }
}

fn current_date_string() -> String {
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| (duration.as_secs() / 86_400) as i64)
        .unwrap_or_default();
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i64, i64, i64) {
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let month_prime = (5 * doy + 2) / 153;
    let day = doy - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };

    (year, month, day)
}

fn slugify_task(task: &str) -> String {
    let mut slug = String::new();
    let mut previous_was_dash = false;
    for character in task.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            previous_was_dash = false;
        } else if !previous_was_dash && !slug.is_empty() {
            slug.push('-');
            previous_was_dash = true;
        }
        if slug.len() >= 60 {
            break;
        }
    }

    let slug = slug.trim_matches('-').to_owned();
    if slug.is_empty() {
        "design".to_owned()
    } else {
        slug
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SolveOptions {
    repo: PathBuf,
    task: String,
    dry_run: bool,
    model_provider: ModelProviderKind,
    openai: OpenAiConfig,
    codex_cli: CodexCliConfig,
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
    model_provider: Option<ModelProviderKind>,
    model: Option<String>,
    base_url: Option<String>,
    api_key_env: Option<String>,
    max_steps: Option<usize>,
    info_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DesignOptions {
    repo: PathBuf,
    task: String,
    dry_run: bool,
    model_provider: ModelProviderKind,
    openai: OpenAiConfig,
    codex_cli: CodexCliConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DesignFlags {
    repo: String,
    task: String,
    dry_run: bool,
    model_provider: Option<ModelProviderKind>,
    model: Option<String>,
    base_url: Option<String>,
    api_key_env: Option<String>,
}

fn solve_options(args: &[String]) -> Result<SolveOptions, String> {
    let flags = SolveFlags::parse(args)?;
    let repo = accessible_repo_path(&flags.repo)?;
    let config = PatchwrightConfig::load(&repo).map_err(|error| error.to_string())?;
    let model_provider = selected_model_provider(&flags, &config)?;
    let openai_model = flags
        .model
        .clone()
        .or_else(|| config.model.openai.model.clone())
        .or_else(|| config.model.model.clone());
    let codex_model = flags
        .model
        .clone()
        .or_else(|| config.model.codex_cli.model.clone())
        .or_else(|| config.model.model.clone());

    Ok(SolveOptions {
        repo,
        task: flags.task,
        dry_run: flags.dry_run,
        model_provider,
        openai: OpenAiConfig {
            base_url: flags
                .base_url
                .or_else(|| config.model.openai.base_url.clone())
                .unwrap_or(config.model.base_url),
            model: openai_model.unwrap_or_default(),
            api_key_env: flags
                .api_key_env
                .or_else(|| config.model.openai.api_key_env.clone())
                .unwrap_or(config.model.api_key_env),
            timeout_seconds: config.model.openai.timeout_seconds,
        },
        codex_cli: CodexCliConfig {
            command: config.model.codex_cli.command,
            model: codex_model,
            timeout_seconds: config.model.codex_cli.timeout_seconds,
        },
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

fn design_options(args: &[String]) -> Result<DesignOptions, String> {
    let flags = DesignFlags::parse(args)?;
    let repo = accessible_repo_path(&flags.repo)?;
    let config = PatchwrightConfig::load(&repo).map_err(|error| error.to_string())?;
    let model_provider = selected_design_model_provider(&flags, &config)?;
    let openai_model = flags
        .model
        .clone()
        .or_else(|| config.model.openai.model.clone())
        .or_else(|| config.model.model.clone());
    let codex_model = flags
        .model
        .clone()
        .or_else(|| config.model.codex_cli.model.clone())
        .or_else(|| config.model.model.clone());

    Ok(DesignOptions {
        repo,
        task: flags.task,
        dry_run: flags.dry_run,
        model_provider,
        openai: OpenAiConfig {
            base_url: flags
                .base_url
                .or_else(|| config.model.openai.base_url.clone())
                .unwrap_or(config.model.base_url),
            model: openai_model.unwrap_or_default(),
            api_key_env: flags
                .api_key_env
                .or_else(|| config.model.openai.api_key_env.clone())
                .unwrap_or(config.model.api_key_env),
            timeout_seconds: config.model.openai.timeout_seconds,
        },
        codex_cli: CodexCliConfig {
            command: config.model.codex_cli.command,
            model: codex_model,
            timeout_seconds: config.model.codex_cli.timeout_seconds,
        },
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
        let model_provider = optional_model_provider(args)?;
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
            model_provider,
            model,
            base_url: optional_value_if_present(args, "--base-url")?,
            api_key_env: optional_value_if_present(args, "--api-key-env")?,
            max_steps,
            info_only: has_flag(args, "--info-only"),
        })
    }
}

impl DesignFlags {
    fn parse(args: &[String]) -> Result<Self, String> {
        validate_design_args(args)?;

        let Some(repo) = value_after(args, "--repo") else {
            return Err("design requires --repo <path> and --task <text>".to_owned());
        };
        let Some(task) = value_after(args, "--task") else {
            return Err("design requires --repo <path> and --task <text>".to_owned());
        };

        Ok(Self {
            repo,
            task,
            dry_run: has_flag(args, "--dry-run"),
            model_provider: optional_model_provider_for(args, "design")?,
            model: optional_value_if_present_for(args, "--model", "design")?,
            base_url: optional_value_if_present_for(args, "--base-url", "design")?,
            api_key_env: optional_value_if_present_for(args, "--api-key-env", "design")?,
        })
    }
}

fn selected_model_provider(
    flags: &SolveFlags,
    config: &PatchwrightConfig,
) -> Result<ModelProviderKind, String> {
    if matches!(flags.model_provider, Some(ModelProviderKind::CodexCli))
        && (flags.base_url.is_some() || flags.api_key_env.is_some())
    {
        return Err(
            "OpenAI-compatible flags require --model-provider openai-compatible".to_owned(),
        );
    }

    if let Some(provider) = flags.model_provider.clone() {
        return Ok(provider);
    }

    if flags.base_url.is_some() || flags.api_key_env.is_some() {
        return Ok(ModelProviderKind::OpenAiCompatible);
    }

    Ok(config.model.provider.clone())
}

fn selected_design_model_provider(
    flags: &DesignFlags,
    config: &PatchwrightConfig,
) -> Result<ModelProviderKind, String> {
    if matches!(flags.model_provider, Some(ModelProviderKind::CodexCli))
        && (flags.base_url.is_some() || flags.api_key_env.is_some())
    {
        return Err(
            "OpenAI-compatible flags require --model-provider openai-compatible".to_owned(),
        );
    }

    if let Some(provider) = flags.model_provider.clone() {
        return Ok(provider);
    }

    if flags.base_url.is_some() || flags.api_key_env.is_some() {
        return Ok(ModelProviderKind::OpenAiCompatible);
    }

    Ok(config.model.provider.clone())
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

fn validate_design_args(args: &[String]) -> Result<(), String> {
    for arg in args.iter().skip(1) {
        if arg == "--api-key" || arg.starts_with("--api-key=") {
            return Err("raw API keys are not accepted; use --api-key-env <name>".to_owned());
        }
    }

    let mut index = 1;
    while index < args.len() {
        let arg = &args[index];
        if !arg.starts_with("--") {
            return Err(format!("unexpected design argument: {arg}"));
        }

        if is_design_value_flag(arg) {
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

        if is_design_bool_flag(arg) {
            index += 1;
            continue;
        }

        return Err(format!("unknown design flag: {arg}"));
    }

    Ok(())
}

fn is_solve_value_flag(arg: &str) -> bool {
    matches!(
        arg,
        "--repo"
            | "--task"
            | "--model-provider"
            | "--model"
            | "--base-url"
            | "--api-key-env"
            | "--max-steps"
    )
}

fn is_solve_bool_flag(arg: &str) -> bool {
    matches!(arg, "--dry-run" | "--info-only")
}

fn is_design_value_flag(arg: &str) -> bool {
    matches!(
        arg,
        "--repo" | "--task" | "--model-provider" | "--model" | "--base-url" | "--api-key-env"
    )
}

fn is_design_bool_flag(arg: &str) -> bool {
    matches!(arg, "--dry-run")
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

fn render_solve_report(report: &SolveReport) -> String {
    let mut output = String::new();

    output.push_str("Patchwright solve result\n\n");
    output.push_str(&format!("Status: {:?}\n\n", report.status));
    output.push_str("Changed files:\n");
    let changed_files = report
        .attempts
        .last()
        .map(|attempt| &attempt.verification.diff_summary.changed_files)
        .filter(|files| !files.is_empty());
    if let Some(files) = changed_files {
        for path in files {
            output.push_str(&format!("  {}\n", path.0));
        }
    } else {
        output.push_str("  none\n");
    }

    output.push_str("\nVerification:\n");
    let checks = report
        .attempts
        .last()
        .map(|attempt| &attempt.verification.checks)
        .filter(|checks| !checks.is_empty());
    if let Some(checks) = checks {
        for check in checks {
            let status = if check.passed { "ok" } else { "failed" };
            output.push_str(&format!("  [{status}] {}\n", check.name));
        }
    } else {
        output.push_str("  none\n");
    }

    output.push_str(&format!("\nAttempts: {}\n", report.attempts.len()));

    if !report.counterexamples.is_empty() {
        output.push_str("\nCounterexamples:\n");
        for counterexample in report.counterexamples.iter().take(3) {
            output.push_str(&format!(
                "  [{}] {}\n",
                counterexample.source,
                first_useful_line(&counterexample.detail)
            ));
        }
    }

    output.push_str("\nSummary:\n");
    output.push_str(&format!("  {}\n", first_useful_line(&report.summary)));

    output
}

fn first_useful_line(value: &str) -> String {
    value
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("none")
        .chars()
        .take(240)
        .collect()
}

fn startup_average_micros(total_nanos: u128, iterations: u128) -> u128 {
    total_nanos / iterations / 1_000
}

enum CliModel {
    OpenAi(OpenAiCompatibleClient),
    CodexCli(CodexCliClient),
}

impl ModelProvider for CliModel {
    fn propose_action(&mut self, request: ModelRequest) -> PatchwrightResult<ModelResponse> {
        match self {
            Self::OpenAi(model) => model.propose_action(request),
            Self::CodexCli(model) => model.propose_action(request),
        }
    }

    fn propose_design(&mut self, request: ModelRequest) -> PatchwrightResult<ArchitectureDesign> {
        match self {
            Self::OpenAi(model) => model.propose_design(request),
            Self::CodexCli(model) => model.propose_design(request),
        }
    }
}

fn cli_model(options: &SolveOptions) -> Result<CliModel, String> {
    if options.dry_run {
        let mut config = options.openai.clone();
        if config.model.is_empty() {
            config.model = "dry-run".to_owned();
        }
        return Ok(CliModel::OpenAi(OpenAiCompatibleClient::dry_run(config)));
    }

    match options.model_provider {
        ModelProviderKind::CodexCli => Ok(CliModel::CodexCli(CodexCliClient::new(
            options.codex_cli.clone(),
        ))),
        ModelProviderKind::OpenAiCompatible => {
            if options.openai.model.is_empty() {
                return Err(
                    "OpenAI-compatible solve requires --model <name> or model.openai.model"
                        .to_owned(),
                );
            }
            Ok(CliModel::OpenAi(OpenAiCompatibleClient::new(
                options.openai.clone(),
            )))
        }
    }
}

fn design_model(options: &DesignOptions) -> Result<CliModel, String> {
    if options.dry_run {
        let mut config = options.openai.clone();
        if config.model.is_empty() {
            config.model = "dry-run".to_owned();
        }
        return Ok(CliModel::OpenAi(OpenAiCompatibleClient::dry_run(config)));
    }

    match options.model_provider {
        ModelProviderKind::CodexCli => Ok(CliModel::CodexCli(CodexCliClient::new(
            options.codex_cli.clone(),
        ))),
        ModelProviderKind::OpenAiCompatible => {
            if options.openai.model.is_empty() {
                return Err(
                    "OpenAI-compatible design requires --model <name> or model.openai.model"
                        .to_owned(),
                );
            }
            Ok(CliModel::OpenAi(OpenAiCompatibleClient::new(
                options.openai.clone(),
            )))
        }
    }
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
    optional_value_if_present_for(args, flag, "solve")
}

fn optional_value_if_present_for(
    args: &[String],
    flag: &str,
    command_name: &str,
) -> Result<Option<String>, String> {
    match value_after(args, flag) {
        Some(value) => Ok(Some(value)),
        None if has_flag(args, flag) => Err(format!("{command_name} requires {flag} <value>")),
        None => Ok(None),
    }
}

fn optional_model_provider(args: &[String]) -> Result<Option<ModelProviderKind>, String> {
    optional_model_provider_for(args, "solve")
}

fn optional_model_provider_for(
    args: &[String],
    command_name: &str,
) -> Result<Option<ModelProviderKind>, String> {
    let Some(value) = value_after(args, "--model-provider") else {
        if has_flag(args, "--model-provider") {
            return Err(format!("{command_name} requires --model-provider <name>"));
        }
        return Ok(None);
    };

    match value.as_str() {
        "codex-cli" => Ok(Some(ModelProviderKind::CodexCli)),
        "openai-compatible" => Ok(Some(ModelProviderKind::OpenAiCompatible)),
        _ => Err(format!(
            "unknown model provider: {value}; expected codex-cli or openai-compatible"
        )),
    }
}

fn model_provider_name(provider: &ModelProviderKind) -> &'static str {
    match provider {
        ModelProviderKind::CodexCli => "codex-cli",
        ModelProviderKind::OpenAiCompatible => "openai-compatible",
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
        "patchwright\n\nUSAGE:\n    patchwright --version\n    patchwright status\n    patchwright auth login [--repo <path>]\n    patchwright auth check [--repo <path>]\n    patchwright config check [--repo <path>]\n    patchwright bench startup\n    patchwright profile [--repo <path>]\n    patchwright design --repo <path> --task <text> [--dry-run] [--model-provider codex-cli|openai-compatible] [--model <name>] [--base-url <url>] [--api-key-env <name>]\n    patchwright solve --repo <path> --task <text> [--dry-run] [--model-provider codex-cli|openai-compatible] [--model <name>] [--base-url <url>] [--api-key-env <name>] [--max-steps <n>] [--info-only]\n    patchwright verify --repo <path>"
    );
}

#[cfg(test)]
mod tests {
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

    fn toml_escape_path(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "\\\\")
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
}
