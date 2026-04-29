#![forbid(unsafe_code)]

use patchwright_core::error::{PatchwrightError, Result};
use patchwright_core::traits::ModelProvider;
use patchwright_core::types::{ModelRequest, ModelResponse};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexCliConfig {
    pub command: String,
    pub model: Option<String>,
}

impl Default for CodexCliConfig {
    fn default() -> Self {
        Self {
            command: "codex".to_owned(),
            model: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CodexCliClient {
    config: CodexCliConfig,
}

impl CodexCliClient {
    pub fn new(config: CodexCliConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &CodexCliConfig {
        &self.config
    }
}

impl ModelProvider for CodexCliClient {
    fn propose_action(&mut self, request: ModelRequest) -> Result<ModelResponse> {
        let work_dir = TempActionDir::create()?;
        let schema_path = work_dir.path().join("action.schema.json");
        let action_path = work_dir.path().join("action.json");

        patchwright_model_contract::write_action_output_schema(&schema_path)?;
        let prompt = patchwright_model_contract::render_exec_prompt(&request);

        let output = run_codex_exec(
            &self.config,
            work_dir.path(),
            &schema_path,
            &action_path,
            &prompt,
        )?;
        if !output.status.success() {
            return Err(PatchwrightError::Model(format!(
                "codex exec failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        let content = fs::read_to_string(&action_path).map_err(|error| {
            PatchwrightError::Model(format!(
                "codex exec did not write action output {}: {error}",
                action_path.display()
            ))
        })?;
        let action = patchwright_model_contract::parse_action_json(&content)?;

        Ok(ModelResponse { action })
    }
}

fn run_codex_exec(
    config: &CodexCliConfig,
    cwd: &Path,
    schema_path: &Path,
    action_path: &Path,
    prompt: &str,
) -> Result<std::process::Output> {
    let mut command = Command::new(&config.command);
    command
        .arg("exec")
        .arg("--ephemeral")
        .arg("--sandbox")
        .arg("read-only")
        .arg("--ask-for-approval")
        .arg("never")
        .arg("--skip-git-repo-check")
        .arg("--output-schema")
        .arg(schema_path)
        .arg("--json")
        .arg("-o")
        .arg(action_path);

    if let Some(model) = &config.model {
        command.arg("--model").arg(model);
    }

    command.arg("-");

    let mut child = command
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            PatchwrightError::Model(format!(
                "failed to start codex command '{}': {error}",
                config.command
            ))
        })?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| PatchwrightError::Model("failed to open codex stdin pipe".to_owned()))?;
    stdin
        .write_all(prompt.as_bytes())
        .map_err(PatchwrightError::from)?;
    drop(stdin);

    child.wait_with_output().map_err(PatchwrightError::from)
}

struct TempActionDir {
    path: PathBuf,
}

impl TempActionDir {
    fn create() -> Result<Self> {
        for _ in 0..100 {
            let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("patchwright-codex-cli-{}-{id}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(PatchwrightError::from(error)),
            }
        }

        Err(PatchwrightError::Model(
            "failed to create unique codex-cli temp directory".to_owned(),
        ))
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempActionDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
