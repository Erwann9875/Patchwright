use patchwright_core::action::Action;
use patchwright_core::traits::ModelProvider;
use patchwright_core::types::{ModelRequest, TaskSpec};
use patchwright_model_codex_cli::{CodexCliClient, CodexCliConfig};
use patchwright_test_support::TempRepo;
use std::fs;
use std::path::{Path, PathBuf};

fn request(task: &str) -> ModelRequest {
    ModelRequest {
        task: TaskSpec::from_text(PathBuf::from("."), task),
        observations: Vec::new(),
        counterexamples: Vec::new(),
        context: None,
    }
}

#[test]
fn codex_cli_provider_uses_schema_output_and_read_only_exec() {
    let repo = TempRepo::new("codex-cli-provider");
    let args_path = repo.root().join("args.txt");
    let stdin_path = repo.root().join("stdin.txt");
    let script = fake_codex_script(repo.root());

    let mut client = CodexCliClient::new(CodexCliConfig {
        command: script.to_string_lossy().into_owned(),
        model: Some("test-codex-model".to_owned()),
        timeout_seconds: 5,
    });

    let response = client
        .propose_action(request("summarize the parser"))
        .expect("fake codex should return an action");

    assert_eq!(
        response.action,
        Action::Finish {
            summary: "from fake codex".to_owned()
        }
    );

    let args = fs::read_to_string(args_path).expect("fake codex args should be recorded");
    assert!(args.contains("exec"));
    assert!(args.contains("--ephemeral"));
    assert!(args.contains("--sandbox read-only"));
    assert!(args.contains("--ask-for-approval never"));
    assert!(args.contains("--skip-git-repo-check"));
    assert!(args.contains("--output-schema"));
    assert!(args.contains("--json"));
    assert!(args.contains("-o"));
    assert!(args.contains("--model test-codex-model"));

    let stdin = fs::read_to_string(stdin_path).expect("fake codex stdin should be recorded");
    assert!(stdin.contains("Do not edit files"));
    assert!(stdin.contains("Return one final JSON object"));
    assert!(stdin.contains("summarize the parser"));
}

#[test]
fn codex_cli_provider_kills_hung_process_after_timeout() {
    let repo = TempRepo::new("codex-cli-timeout");
    let script = sleeping_codex_script(repo.root());

    let mut client = CodexCliClient::new(CodexCliConfig {
        command: script.to_string_lossy().into_owned(),
        model: None,
        timeout_seconds: 1,
    });

    let started = std::time::Instant::now();
    let error = client
        .propose_action(request("summarize slowly"))
        .expect_err("hung fake codex should time out");

    assert!(error.to_string().contains("timed out"));
    assert!(error.to_string().contains("codex exec"));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(3),
        "timeout should kill the fake Codex process quickly; elapsed {:?}",
        started.elapsed()
    );
}

#[cfg(windows)]
fn fake_codex_script(root: &Path) -> PathBuf {
    let script = root.join("fake-codex.cmd");
    fs::write(
        &script,
        format!(
            r#"@echo off
setlocal
echo %*>"{}"
set output=
:loop
if "%~1"=="" goto after_args
if "%~1"=="-o" set output=%~2
shift
goto loop
:after_args
more > "{}"
> "%output%" echo {{"action":"finish","summary":"from fake codex"}}
exit /b 0
"#,
            root.join("args.txt").display(),
            root.join("stdin.txt").display()
        ),
    )
    .expect("fake codex script should be written");
    script
}

#[cfg(windows)]
fn sleeping_codex_script(root: &Path) -> PathBuf {
    let script = root.join("sleeping-codex.cmd");
    fs::write(
        &script,
        r#"@echo off
powershell -NoProfile -Command "Start-Sleep -Seconds 5"
exit /b 0
"#,
    )
    .expect("sleeping codex script should be written");
    script
}

#[cfg(not(windows))]
fn fake_codex_script(root: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let script = root.join("fake-codex");
    fs::write(
        &script,
        format!(
            r#"#!/bin/sh
printf '%s\n' "$*" > "{}"
output=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "-o" ]; then
    shift
    output="$1"
  fi
  shift
done
cat > "{}"
printf '%s\n' '{{"action":"finish","summary":"from fake codex"}}' > "$output"
"#,
            root.join("args.txt").display(),
            root.join("stdin.txt").display()
        ),
    )
    .expect("fake codex script should be written");
    let mut permissions = fs::metadata(&script)
        .expect("fake codex metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&script, permissions).expect("fake codex should be executable");
    script
}

#[cfg(not(windows))]
fn sleeping_codex_script(root: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let script = root.join("sleeping-codex");
    fs::write(
        &script,
        r#"#!/bin/sh
sleep 5
"#,
    )
    .expect("sleeping codex script should be written");
    let mut permissions = fs::metadata(&script)
        .expect("sleeping codex metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&script, permissions).expect("sleeping codex should be executable");
    script
}
