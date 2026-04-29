use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

pub(crate) fn apply_sandbox_diff_to_source(
    sandbox_repo: &Path,
    source_repo: &Path,
) -> Result<(), String> {
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

pub(crate) fn git_diff(repo: &Path) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["diff", "--"])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "failed to read git diff: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
