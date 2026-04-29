#![forbid(unsafe_code)]

use patchwright_core::error::{PatchwrightError, Result};
use patchwright_core::policy::Policy;
use patchwright_core::traits::ExecutionBackend;
use patchwright_core::types::{
    CommandSpec, ExitStatus, FileSlice, LineRange, Patch, PatchId, RepoPath, RunReport,
    SearchMatch, SearchQuery, SearchResults, SnapshotId,
};
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct LocalExecution {
    root: PathBuf,
}

impl LocalExecution {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = fs::canonicalize(root.as_ref())
            .unwrap_or_else(|error| panic!("failed to canonicalize repo root: {error}"));

        Self { root }
    }

    fn resolve(&self, path: &RepoPath) -> Result<PathBuf> {
        let relative = Path::new(&path.0);

        if relative.is_absolute() {
            return Err(PatchwrightError::InvalidInput(format!(
                "repo path must be relative: {}",
                path.0
            )));
        }

        for component in relative.components() {
            if matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            ) {
                return Err(PatchwrightError::InvalidInput(format!(
                    "repo path must not escape repo root: {}",
                    path.0
                )));
            }
        }

        let resolved = self.root.join(relative);

        match fs::canonicalize(&resolved) {
            Ok(canonical) => {
                if canonical.starts_with(&self.root) {
                    Ok(canonical)
                } else {
                    Err(PatchwrightError::InvalidInput(format!(
                        "repo path must not escape repo root: {}",
                        path.0
                    )))
                }
            }
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(resolved),
            Err(error) => Err(PatchwrightError::from(error)),
        }
    }
}

impl ExecutionBackend for LocalExecution {
    fn snapshot(&mut self) -> Result<SnapshotId> {
        let output = git_output(&self.root, ["rev-parse", "HEAD"])?;
        Ok(SnapshotId(output.trim().to_owned()))
    }

    fn read_file(&self, path: RepoPath, range: Option<LineRange>) -> Result<FileSlice> {
        let resolved = self.resolve(&path)?;
        let content = fs::read_to_string(&resolved).map_err(PatchwrightError::from)?;

        let Some(range) = range else {
            return Ok(FileSlice {
                path,
                start_line: 1,
                content,
            });
        };

        if range.start == 0 || range.end < range.start {
            return Err(PatchwrightError::InvalidInput(format!(
                "invalid line range: {}..={}",
                range.start, range.end
            )));
        }

        let selected = content
            .split_inclusive('\n')
            .enumerate()
            .filter_map(|(index, line)| {
                let line_number = index + 1;
                (range.start <= line_number && line_number <= range.end).then_some(line)
            })
            .collect();

        Ok(FileSlice {
            path,
            start_line: range.start,
            content: selected,
        })
    }

    fn search(&self, query: SearchQuery) -> Result<SearchResults> {
        let search_root = match &query.root {
            Some(root) => self.resolve(root)?,
            None => self.root.clone(),
        };
        let mut matches = Vec::new();

        search_directory(&self.root, &search_root, &query.pattern, &mut matches)?;

        Ok(SearchResults { matches })
    }

    fn apply_patch(&mut self, patch: Patch) -> Result<PatchId> {
        let mut child = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["apply", "--whitespace=nowarn"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(PatchwrightError::from)?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(patch.unified_diff.as_bytes())
                .map_err(PatchwrightError::from)?;
        }

        let output = child.wait_with_output().map_err(PatchwrightError::from)?;
        if !output.status.success() {
            return Err(PatchwrightError::CommandFailed(format!(
                "git apply failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        Ok(PatchId("local-git-apply".to_owned()))
    }

    fn run(&mut self, command: CommandSpec, policy: &Policy) -> Result<RunReport> {
        if !policy.allows(&command) {
            return Err(PatchwrightError::PolicyDenied(format!(
                "command denied by policy: {}",
                command.program
            )));
        }

        let cwd = match &command.cwd {
            Some(path) => self.resolve(path)?,
            None => self.root.clone(),
        };
        let mut child = Command::new(&command.program)
            .args(&command.args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(PatchwrightError::from)?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| PatchwrightError::Io("failed to capture stdout".to_owned()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| PatchwrightError::Io("failed to capture stderr".to_owned()))?;
        let stdout_thread = read_pipe(stdout);
        let stderr_thread = read_pipe(stderr);

        let stdin_thread = if let Some(stdin_content) = &command.stdin {
            child.stdin.take().map(|mut stdin| {
                let stdin_content = stdin_content.clone();
                thread::spawn(move || {
                    let _ = stdin.write_all(stdin_content.as_bytes());
                })
            })
        } else {
            drop(child.stdin.take());
            None
        };

        let started = Instant::now();
        let mut timed_out = false;
        let status = loop {
            if let Some(status) = child.try_wait().map_err(PatchwrightError::from)? {
                break status;
            }

            if started.elapsed() >= command.timeout {
                child.kill().map_err(PatchwrightError::from)?;
                timed_out = true;
                break child.wait().map_err(PatchwrightError::from)?;
            }

            thread::sleep(std::time::Duration::from_millis(10));
        };

        join_optional_writer(stdin_thread)?;
        let stdout = join_reader(stdout_thread)?;
        let mut stderr = join_reader(stderr_thread)?;

        if timed_out {
            if !stderr.is_empty() && !stderr.ends_with('\n') {
                stderr.push('\n');
            }
            stderr.push_str("command timed out");
        }

        Ok(RunReport {
            command,
            status: ExitStatus {
                code: if timed_out { None } else { status.code() },
                success: !timed_out && status.success(),
            },
            stdout,
            stderr,
        })
    }

    fn revert(&mut self, snapshot: SnapshotId) -> Result<()> {
        git_status(&self.root, ["reset", "--hard", &snapshot.0])?;
        git_status(&self.root, ["clean", "-fd"])?;
        Ok(())
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
        .map_err(|_| PatchwrightError::Io("reader thread panicked".to_owned()))?
        .map_err(PatchwrightError::Io)?;

    Ok(String::from_utf8_lossy(&output).into_owned())
}

fn join_optional_writer(handle: Option<JoinHandle<()>>) -> Result<()> {
    if let Some(handle) = handle {
        handle
            .join()
            .map_err(|_| PatchwrightError::Io("stdin writer thread panicked".to_owned()))?;
    }

    Ok(())
}

fn search_directory(
    repo_root: &Path,
    directory: &Path,
    pattern: &str,
    matches: &mut Vec<SearchMatch>,
) -> Result<()> {
    let mut entries = fs::read_dir(directory)
        .map_err(PatchwrightError::from)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(PatchwrightError::from)?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type().map_err(PatchwrightError::from)?;

        if file_type.is_dir() {
            if entry.file_name() != ".git" {
                search_directory(repo_root, &path, pattern, matches)?;
            }
            continue;
        }

        if !file_type.is_file() {
            continue;
        }

        match fs::read_to_string(&path) {
            Ok(content) => collect_matches(repo_root, &path, &content, pattern, matches)?,
            Err(error) if error.kind() == ErrorKind::InvalidData => {}
            Err(error) => return Err(PatchwrightError::from(error)),
        }
    }

    Ok(())
}

fn collect_matches(
    repo_root: &Path,
    path: &Path,
    content: &str,
    pattern: &str,
    matches: &mut Vec<SearchMatch>,
) -> Result<()> {
    let repo_path = path
        .strip_prefix(repo_root)
        .map_err(|error| PatchwrightError::InvalidInput(error.to_string()))?;
    let repo_path = normalize_repo_path(repo_path);

    for (index, line) in content.lines().enumerate() {
        if line.contains(pattern) {
            matches.push(SearchMatch {
                path: RepoPath::new(repo_path.clone()),
                line: index + 1,
                text: line.to_owned(),
            });
        }
    }

    Ok(())
}

fn normalize_repo_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn git_output<const N: usize>(root: &Path, args: [&str; N]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(PatchwrightError::from)?;

    if !output.status.success() {
        return Err(PatchwrightError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn git_status<const N: usize>(root: &Path, args: [&str; N]) -> Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(PatchwrightError::from)?;

    if !output.status.success() {
        return Err(PatchwrightError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }

    Ok(())
}
