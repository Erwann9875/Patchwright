use patchwright_config::{ModelProviderKind, PatchwrightConfig};
use patchwright_core::PatchwrightError;
use std::fs;

#[test]
fn defaults_match_v0_1_plan() {
    let config = PatchwrightConfig::default();

    assert_eq!(config.model.provider, ModelProviderKind::CodexCli);
    assert_eq!(config.model.base_url, "https://api.openai.com/v1");
    assert_eq!(config.model.model, None);
    assert_eq!(config.model.api_key_env, "OPENAI_API_KEY");
    assert_eq!(config.model.codex_cli.command, "codex");
    assert_eq!(config.model.codex_cli.model, None);
    assert_eq!(config.model.codex_cli.timeout_seconds, 120);
    assert_eq!(config.model.openai.base_url, None);
    assert_eq!(config.model.openai.model, None);
    assert_eq!(config.model.openai.api_key_env, None);
    assert_eq!(config.model.openai.timeout_seconds, 30);
    assert_eq!(config.agent.max_steps, 30);
    assert!(config.agent.require_patch);
    assert_eq!(config.agent.max_changed_files, 5);
    assert_eq!(config.agent.max_inserted_lines, 300);
    assert_eq!(config.policy.allowed_programs, vec!["cargo"]);
    assert!(config.rust.fmt);
    assert!(config.rust.check);
    assert!(config.rust.test);
    assert!(!config.rust.clippy);
}

#[test]
fn parses_partial_toml_over_defaults() {
    let config = PatchwrightConfig::from_toml_str(
        r#"
[model]
provider = "openai-compatible"
base_url = "http://127.0.0.1:8080/v1"
model = "gpt-test"
api_key_env = "PATCHWRIGHT_TEST_KEY"

[model.codex_cli]
command = "codex-test"
model = "codex-test-model"
timeout_seconds = 11

[model.openai]
base_url = "http://nested.invalid/v1"
model = "nested-openai-model"
api_key_env = "NESTED_OPENAI_KEY"
timeout_seconds = 9

[agent]
max_steps = 7
require_patch = false

[policy]
allowed_programs = ["cargo", "rustc"]

[rust]
clippy = true
"#,
    )
    .unwrap();

    assert_eq!(config.model.provider, ModelProviderKind::OpenAiCompatible);
    assert_eq!(config.model.base_url, "http://127.0.0.1:8080/v1");
    assert_eq!(config.model.model, Some("gpt-test".to_owned()));
    assert_eq!(config.model.api_key_env, "PATCHWRIGHT_TEST_KEY");
    assert_eq!(config.model.codex_cli.command, "codex-test");
    assert_eq!(
        config.model.codex_cli.model,
        Some("codex-test-model".to_owned())
    );
    assert_eq!(config.model.codex_cli.timeout_seconds, 11);
    assert_eq!(
        config.model.openai.base_url,
        Some("http://nested.invalid/v1".to_owned())
    );
    assert_eq!(
        config.model.openai.model,
        Some("nested-openai-model".to_owned())
    );
    assert_eq!(
        config.model.openai.api_key_env,
        Some("NESTED_OPENAI_KEY".to_owned())
    );
    assert_eq!(config.model.openai.timeout_seconds, 9);
    assert_eq!(config.agent.max_steps, 7);
    assert!(!config.agent.require_patch);
    assert_eq!(config.agent.max_changed_files, 5);
    assert_eq!(config.agent.max_inserted_lines, 300);
    assert_eq!(config.policy.allowed_programs, vec!["cargo", "rustc"]);
    assert!(config.rust.fmt);
    assert!(config.rust.check);
    assert!(config.rust.test);
    assert!(config.rust.clippy);
}

#[test]
fn rejects_zero_max_steps() {
    let result = PatchwrightConfig::from_toml_str("[agent]\nmax_steps = 0\n");

    assert!(matches!(
        result,
        Err(PatchwrightError::InvalidInput(message)) if message.contains("agent.max_steps")
    ));
}

#[test]
fn rejects_zero_diff_limits() {
    let changed_files = PatchwrightConfig::from_toml_str("[agent]\nmax_changed_files = 0\n");
    let inserted_lines = PatchwrightConfig::from_toml_str("[agent]\nmax_inserted_lines = 0\n");

    assert!(matches!(
        changed_files,
        Err(PatchwrightError::InvalidInput(message)) if message.contains("agent.max_changed_files")
    ));
    assert!(matches!(
        inserted_lines,
        Err(PatchwrightError::InvalidInput(message)) if message.contains("agent.max_inserted_lines")
    ));
}

#[test]
fn rejects_empty_codex_command_and_zero_openai_timeout() {
    let command = PatchwrightConfig::from_toml_str("[model.codex_cli]\ncommand = \"\"\n");
    let codex_timeout =
        PatchwrightConfig::from_toml_str("[model.codex_cli]\ntimeout_seconds = 0\n");
    let timeout = PatchwrightConfig::from_toml_str("[model.openai]\ntimeout_seconds = 0\n");

    assert!(matches!(
        command,
        Err(PatchwrightError::InvalidInput(message)) if message.contains("model.codex_cli.command")
    ));
    assert!(matches!(
        codex_timeout,
        Err(PatchwrightError::InvalidInput(message)) if message.contains("model.codex_cli.timeout_seconds")
    ));
    assert!(matches!(
        timeout,
        Err(PatchwrightError::InvalidInput(message)) if message.contains("model.openai.timeout_seconds")
    ));
}

#[test]
fn rejects_empty_allowed_programs() {
    let result = PatchwrightConfig::from_toml_str("[policy]\nallowed_programs = []\n");

    assert!(matches!(
        result,
        Err(PatchwrightError::InvalidInput(message)) if message.contains("policy.allowed_programs")
    ));
}

#[test]
fn rejects_unknown_top_level_keys() {
    let result = PatchwrightConfig::from_toml_str("max_steps = 5\n");

    assert!(matches!(
        result,
        Err(PatchwrightError::InvalidInput(message)) if message.contains("unknown field")
    ));
}

#[test]
fn rejects_unknown_nested_keys() {
    let result = PatchwrightConfig::from_toml_str("[agent]\nmax_step = 5\n");

    assert!(matches!(
        result,
        Err(PatchwrightError::InvalidInput(message)) if message.contains("unknown field")
    ));
}

#[test]
fn load_reads_patchwright_toml_from_repo_root() {
    let repo = std::env::temp_dir().join(format!("patchwright-config-load-{}", std::process::id()));
    if repo.exists() {
        fs::remove_dir_all(&repo).unwrap();
    }
    fs::create_dir(&repo).unwrap();
    fs::write(
        repo.join("patchwright.toml"),
        "[model]\nmodel = \"configured\"\n[agent]\nmax_steps = 4\n",
    )
    .unwrap();

    let config = PatchwrightConfig::load(&repo).unwrap();

    fs::remove_dir_all(&repo).unwrap();
    assert_eq!(config.model.model, Some("configured".to_owned()));
    assert_eq!(config.agent.max_steps, 4);
}
