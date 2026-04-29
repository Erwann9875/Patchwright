#![forbid(unsafe_code)]

use patchwright_core::error::{PatchwrightError, Result};
use patchwright_core::traits::ModelProvider;
use patchwright_core::types::{ModelRequest, ModelResponse};
use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexCliConfig {
    pub command: String,
    pub model: Option<String>,
    pub timeout_seconds: u64,
}

impl Default for CodexCliConfig {
    fn default() -> Self {
        Self {
            command: "codex".to_owned(),
            model: None,
            timeout_seconds: 120,
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

    pub fn check_auth(&self) -> Result<()> {
        let work_dir = TempActionDir::create()?;
        let schema_path = work_dir.path().join("auth.schema.json");
        let action_path = work_dir.path().join("auth.json");
        let schema = json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["status"],
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["ok"]
                }
            }
        });
        fs::write(
            &schema_path,
            serde_json::to_vec_pretty(&schema).map_err(|error| {
                PatchwrightError::Model(format!("failed to serialize auth schema: {error}"))
            })?,
        )?;

        let output = run_codex_exec(
            &self.config,
            work_dir.path(),
            &schema_path,
            &action_path,
            "Return {\"status\":\"ok\"} if structured output is available.",
        )?;
        if output.timed_out {
            return Err(PatchwrightError::Model(
                output.timeout_error("codex auth check"),
            ));
        }
        if !output.success() {
            return Err(PatchwrightError::Model(format!(
                "codex auth check failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
                output.status_code(),
                output.stdout,
                output.stderr
            )));
        }

        let content = fs::read_to_string(&action_path).map_err(|error| {
            PatchwrightError::Model(format!(
                "codex auth check did not write structured output {}: {error}",
                action_path.display()
            ))
        })?;
        let value: Value = serde_json::from_str(&content).map_err(|error| {
            PatchwrightError::Model(format!("codex auth check output was not JSON: {error}"))
        })?;
        if value.get("status").and_then(Value::as_str) != Some("ok") {
            return Err(PatchwrightError::Model(
                "codex auth check output did not contain status=ok".to_owned(),
            ));
        }

        Ok(())
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
        if output.timed_out {
            return Err(PatchwrightError::Model(output.timeout_error("codex exec")));
        }
        if !output.success() {
            return Err(PatchwrightError::Model(format!(
                "codex exec failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
                output.status_code(),
                output.stdout,
                output.stderr
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
) -> Result<CodexExecOutput> {
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
    prepare_command(&mut command);

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

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| PatchwrightError::Model("failed to open codex stdout pipe".to_owned()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| PatchwrightError::Model("failed to open codex stderr pipe".to_owned()))?;
    let stdout_handle = read_pipe(stdout);
    let stderr_handle = read_pipe(stderr);

    let deadline = Instant::now() + Duration::from_secs(config.timeout_seconds);
    loop {
        if let Some(status) = child.try_wait().map_err(PatchwrightError::from)? {
            return Ok(CodexExecOutput {
                status: Some(status),
                stdout: join_reader(stdout_handle)?,
                stderr: join_reader(stderr_handle)?,
                timed_out: false,
            });
        }

        if Instant::now() >= deadline {
            kill_child_tree(&mut child);
            let status = child.wait().ok();
            return Ok(CodexExecOutput {
                status,
                stdout: join_reader(stdout_handle)?,
                stderr: join_reader(stderr_handle)?,
                timed_out: true,
            });
        }

        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(unix)]
fn prepare_command(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
fn prepare_command(_command: &mut Command) {}

#[cfg(windows)]
fn kill_child_tree(child: &mut std::process::Child) {
    let pid = child.id().to_string();
    let _ = Command::new("taskkill")
        .args(["/PID", &pid, "/T", "/F"])
        .output();
    let _ = child.kill();
}

#[cfg(unix)]
fn kill_child_tree(child: &mut std::process::Child) {
    let group = format!("-{}", child.id());
    let _ = Command::new("kill").args(["-TERM", &group]).output();
    thread::sleep(Duration::from_millis(50));
    if matches!(child.try_wait(), Ok(None)) {
        let _ = Command::new("kill").args(["-KILL", &group]).output();
    }
    let _ = child.kill();
}

#[cfg(all(not(windows), not(unix)))]
fn kill_child_tree(child: &mut std::process::Child) {
    let _ = child.kill();
}

#[derive(Debug)]
struct CodexExecOutput {
    status: Option<ExitStatus>,
    stdout: String,
    stderr: String,
    timed_out: bool,
}

impl CodexExecOutput {
    fn success(&self) -> bool {
        self.status.is_some_and(|status| status.success())
    }

    fn status_code(&self) -> Option<i32> {
        self.status.and_then(|status| status.code())
    }

    fn timeout_error(&self, label: &str) -> String {
        format!(
            "{label} timed out\nstdout:\n{}\nstderr:\n{}",
            self.stdout, self.stderr
        )
    }
}

fn read_pipe(
    mut pipe: impl Read + Send + 'static,
) -> JoinHandle<std::result::Result<Vec<u8>, String>> {
    thread::spawn(move || {
        let mut output = Vec::new();
        pipe.read_to_end(&mut output)
            .map_err(|error| error.to_string())?;
        Ok(output)
    })
}

fn join_reader(handle: JoinHandle<std::result::Result<Vec<u8>, String>>) -> Result<String> {
    let output = handle
        .join()
        .map_err(|_| PatchwrightError::Model("codex output reader thread panicked".to_owned()))?
        .map_err(PatchwrightError::Io)?;

    Ok(String::from_utf8_lossy(&output).into_owned())
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
