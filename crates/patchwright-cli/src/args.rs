use crate::implementation_plan::{implementation_task_text, load_implementation_graph, StepScope};
use patchwright_config::{ModelProviderKind, PatchwrightConfig, RustConfig, VerifyCommandConfig};
use patchwright_model_codex_cli::CodexCliConfig;
use patchwright_model_openai::OpenAiConfig;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SolveOptions {
    pub(crate) repo: PathBuf,
    pub(crate) task: String,
    pub(crate) dry_run: bool,
    pub(crate) model_provider: ModelProviderKind,
    pub(crate) openai: OpenAiConfig,
    pub(crate) codex_cli: CodexCliConfig,
    pub(crate) max_steps: usize,
    pub(crate) require_patch: bool,
    pub(crate) max_changed_files: usize,
    pub(crate) max_inserted_lines: usize,
    pub(crate) allowed_programs: Vec<String>,
    pub(crate) rust: RustConfig,
    pub(crate) verify_commands: Vec<VerifyCommandConfig>,
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
struct ImplementFlags {
    repo: String,
    from: String,
    step: String,
    dry_run: bool,
    model_provider: Option<ModelProviderKind>,
    model: Option<String>,
    base_url: Option<String>,
    api_key_env: Option<String>,
    max_steps: Option<usize>,
    info_only: bool,
    allow_out_of_scope: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DesignOptions {
    pub(crate) repo: PathBuf,
    pub(crate) task: String,
    pub(crate) dry_run: bool,
    pub(crate) model_provider: ModelProviderKind,
    pub(crate) openai: OpenAiConfig,
    pub(crate) codex_cli: CodexCliConfig,
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

pub(crate) fn solve_options(args: &[String]) -> Result<SolveOptions, String> {
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
        verify_commands: config.verify.commands,
    })
}

pub(crate) fn implement_options(
    args: &[String],
) -> Result<(SolveOptions, Option<StepScope>), String> {
    let flags = ImplementFlags::parse(args)?;
    let repo = accessible_repo_path(&flags.repo)?;
    let graph = load_implementation_graph(&repo, &flags.from)?;
    let step = graph
        .steps
        .iter()
        .find(|step| step.id == flags.step)
        .ok_or_else(|| format!("plan step '{}' was not found", flags.step))?;
    let task = implementation_task_text(step);

    let mut solve_args = vec![
        "solve".to_owned(),
        "--repo".to_owned(),
        flags.repo.clone(),
        "--task".to_owned(),
        task,
    ];
    append_optional_solve_flags(&mut solve_args, &flags);

    let options = solve_options(&solve_args)?;
    let scope = if flags.allow_out_of_scope || step.target_files.is_empty() {
        None
    } else {
        Some(StepScope {
            target_files: step.target_files.clone(),
        })
    };

    Ok((options, scope))
}

fn append_optional_solve_flags(args: &mut Vec<String>, flags: &ImplementFlags) {
    if flags.dry_run {
        args.push("--dry-run".to_owned());
    }
    if flags.info_only {
        args.push("--info-only".to_owned());
    }
    if let Some(provider) = &flags.model_provider {
        args.push("--model-provider".to_owned());
        args.push(model_provider_name(provider).to_owned());
    }
    if let Some(model) = &flags.model {
        args.push("--model".to_owned());
        args.push(model.clone());
    }
    if let Some(base_url) = &flags.base_url {
        args.push("--base-url".to_owned());
        args.push(base_url.clone());
    }
    if let Some(api_key_env) = &flags.api_key_env {
        args.push("--api-key-env".to_owned());
        args.push(api_key_env.clone());
    }
    if let Some(max_steps) = flags.max_steps {
        args.push("--max-steps".to_owned());
        args.push(max_steps.to_string());
    }
}

pub(crate) fn design_options(args: &[String]) -> Result<DesignOptions, String> {
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

impl ImplementFlags {
    fn parse(args: &[String]) -> Result<Self, String> {
        validate_implement_args(args)?;

        let Some(repo) = value_after(args, "--repo") else {
            return Err(
                "implement requires --repo <path>, --from <plan>, and --step <id>".to_owned(),
            );
        };
        let Some(from) = value_after(args, "--from") else {
            return Err(
                "implement requires --repo <path>, --from <plan>, and --step <id>".to_owned(),
            );
        };
        let Some(step) = value_after(args, "--step") else {
            return Err(
                "implement requires --repo <path>, --from <plan>, and --step <id>".to_owned(),
            );
        };

        let max_steps = match value_after(args, "--max-steps") {
            Some(value) => value
                .parse::<usize>()
                .ok()
                .filter(|value| *value > 0)
                .map(Some)
                .ok_or_else(|| {
                    "implement requires --max-steps to be a positive integer".to_owned()
                })?,
            None if has_flag(args, "--max-steps") => {
                return Err("implement requires --max-steps to be a positive integer".to_owned());
            }
            None => None,
        };

        Ok(Self {
            repo,
            from,
            step,
            dry_run: has_flag(args, "--dry-run"),
            model_provider: optional_model_provider_for(args, "implement")?,
            model: optional_value_if_present_for(args, "--model", "implement")?,
            base_url: optional_value_if_present_for(args, "--base-url", "implement")?,
            api_key_env: optional_value_if_present_for(args, "--api-key-env", "implement")?,
            max_steps,
            info_only: has_flag(args, "--info-only"),
            allow_out_of_scope: has_flag(args, "--allow-out-of-scope"),
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

fn validate_implement_args(args: &[String]) -> Result<(), String> {
    for arg in args.iter().skip(1) {
        if arg == "--api-key" || arg.starts_with("--api-key=") {
            return Err("raw API keys are not accepted; use --api-key-env <name>".to_owned());
        }
    }

    let mut index = 1;
    while index < args.len() {
        let arg = &args[index];
        if !arg.starts_with("--") {
            return Err(format!("unexpected implement argument: {arg}"));
        }

        if is_implement_value_flag(arg) {
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

        if is_implement_bool_flag(arg) {
            index += 1;
            continue;
        }

        return Err(format!("unknown implement flag: {arg}"));
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

fn is_implement_value_flag(arg: &str) -> bool {
    matches!(
        arg,
        "--repo"
            | "--from"
            | "--step"
            | "--model-provider"
            | "--model"
            | "--base-url"
            | "--api-key-env"
            | "--max-steps"
    )
}

fn is_implement_bool_flag(arg: &str) -> bool {
    matches!(arg, "--dry-run" | "--info-only" | "--allow-out-of-scope")
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

pub(crate) fn value_after(args: &[String], flag: &str) -> Option<String> {
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

pub(crate) fn model_provider_name(provider: &ModelProviderKind) -> &'static str {
    match provider {
        ModelProviderKind::CodexCli => "codex-cli",
        ModelProviderKind::OpenAiCompatible => "openai-compatible",
    }
}

pub(crate) fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

pub(crate) fn accessible_repo_path(path: &str) -> Result<PathBuf, String> {
    let repo =
        fs::canonicalize(path).map_err(|_| format!("repo path is not accessible: {path}"))?;
    if !repo.is_dir() {
        return Err(format!("repo path is not a directory: {path}"));
    }
    Ok(repo)
}
