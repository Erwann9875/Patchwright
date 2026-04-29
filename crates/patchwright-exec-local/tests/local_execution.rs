use patchwright_core::error::{PatchwrightError, Result};
use patchwright_core::policy::Policy;
use patchwright_core::traits::ExecutionBackend;
use patchwright_core::types::{CommandSpec, LineRange, Patch, RepoPath, SearchMatch, SearchQuery};
use patchwright_exec_local::{GitWorktreeSandbox, LocalExecution};
use patchwright_test_support::TempRepo;
use std::fs;
use std::path::Path;
use std::time::Duration;

#[test]
fn reads_file_ranges_and_searches_text() -> Result<()> {
    let repo = TempRepo::new("exec-local-read-search");
    repo.write("src/lib.rs", "alpha\nneedle one\nbeta\nneedle two\n");
    repo.write("README.md", "needle docs\n");
    repo.write(".git/ignored.txt", "needle hidden\n");
    fs::write(repo.root().join("binary.bin"), b"before\xFFafter")?;
    repo.commit_all("seed");

    let execution = LocalExecution::new(repo.root());

    let slice = execution.read_file(
        RepoPath::new("src/lib.rs"),
        Some(LineRange { start: 2, end: 3 }),
    )?;
    assert_eq!(slice.path, RepoPath::new("src/lib.rs"));
    assert_eq!(slice.start_line, 2);
    assert_eq!(slice.content, "needle one\nbeta\n");

    let results = execution.search(SearchQuery {
        pattern: "needle".to_owned(),
        root: None,
    })?;

    assert_eq!(
        results.matches,
        vec![
            SearchMatch {
                path: RepoPath::new("README.md"),
                line: 1,
                text: "needle docs".to_owned(),
            },
            SearchMatch {
                path: RepoPath::new("src/lib.rs"),
                line: 2,
                text: "needle one".to_owned(),
            },
            SearchMatch {
                path: RepoPath::new("src/lib.rs"),
                line: 4,
                text: "needle two".to_owned(),
            },
        ]
    );

    Ok(())
}

#[test]
fn applies_patch_and_reverts_to_snapshot() -> Result<()> {
    let repo = TempRepo::new("exec-local-apply-revert");
    repo.write("notes.txt", "old\nkeep\n");
    repo.commit_all("seed");

    let mut execution = LocalExecution::new(repo.root());
    let snapshot = execution.snapshot()?;

    let patch_id = execution.apply_patch(Patch {
        unified_diff: "\
diff --git a/notes.txt b/notes.txt
index aac65dc..6320cd2 100644
--- a/notes.txt
+++ b/notes.txt
@@ -1,2 +1,2 @@
-old
+new
 keep
"
        .to_owned(),
    })?;

    assert_eq!(patch_id.0, "local-git-apply");
    let applied_content = fs::read_to_string(repo.root().join("notes.txt"))?;
    assert_eq!(applied_content.replace("\r\n", "\n"), "new\nkeep\n");

    execution.revert(snapshot)?;

    let reverted_content = fs::read_to_string(repo.root().join("notes.txt"))?;
    assert_eq!(reverted_content.replace("\r\n", "\n"), "old\nkeep\n");

    Ok(())
}

#[test]
fn diff_summary_reports_modified_and_new_files() -> Result<()> {
    let repo = TempRepo::new("exec-local-diff-summary");
    repo.write("src/lib.rs", "old\nkeep\n");
    repo.commit_all("seed");
    let mut execution = LocalExecution::new(repo.root());

    execution.apply_patch(Patch {
        unified_diff: "\
diff --git a/README.md b/README.md
new file mode 100644
index 0000000..1269488
--- /dev/null
+++ b/README.md
@@ -0,0 +1 @@
+hello
diff --git a/src/lib.rs b/src/lib.rs
index aac65dc..6320cd2 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,2 +1,2 @@
-old
+new
 keep
"
        .to_owned(),
    })?;

    let summary = execution.diff_summary()?;

    assert_eq!(
        summary.changed_files,
        vec![RepoPath::new("README.md"), RepoPath::new("src/lib.rs")]
    );
    assert_eq!(summary.inserted_lines, 2);
    assert_eq!(summary.deleted_lines, 1);

    Ok(())
}

#[test]
fn sandbox_creation_rejects_dirty_source_without_removing_user_files() -> Result<()> {
    let repo = TempRepo::new("exec-local-sandbox-rejects-dirty-source");
    repo.write("notes.txt", "committed\n");
    repo.commit_all("seed");
    repo.write("notes.txt", "dirty source\n");
    repo.write("scratch.txt", "untracked source\n");

    let result = GitWorktreeSandbox::create(repo.root());

    assert!(
        matches!(result, Err(PatchwrightError::InvalidInput(message)) if message.contains("uncommitted changes"))
    );
    assert_eq!(
        fs::read_to_string(repo.root().join("notes.txt"))?,
        "dirty source\n"
    );
    assert_eq!(
        fs::read_to_string(repo.root().join("scratch.txt"))?,
        "untracked source\n"
    );

    Ok(())
}

#[test]
fn sandboxed_worktree_revert_does_not_mutate_clean_source_repo() -> Result<()> {
    let repo = TempRepo::new("exec-local-sandbox-preserves-clean-source");
    repo.write("notes.txt", "committed\n");
    repo.commit_all("seed");

    {
        let sandbox = GitWorktreeSandbox::create(repo.root())?;
        assert_ne!(sandbox.root(), repo.root());

        let mut execution = LocalExecution::new(sandbox.root());
        let snapshot = execution.snapshot()?;
        execution.apply_patch(Patch {
            unified_diff: "\
diff --git a/notes.txt b/notes.txt
index 3f17798..8f7fc58 100644
--- a/notes.txt
+++ b/notes.txt
@@ -1 +1 @@
-committed
+sandbox change
"
            .to_owned(),
        })?;
        execution.revert(snapshot)?;
    }

    assert_eq!(
        fs::read_to_string(repo.root().join("notes.txt"))?,
        "committed\n"
    );
    assert!(!repo.root().join("scratch.txt").exists());

    Ok(())
}

#[test]
fn policy_denies_unlisted_commands() {
    let repo = TempRepo::new("exec-local-policy");
    repo.write("README.md", "policy\n");
    repo.commit_all("seed");
    let mut execution = LocalExecution::new(repo.root());

    let mut command = CommandSpec::new("git", ["status", "--short"]);
    command.timeout = Duration::from_secs(5);

    let result = execution.run(
        command,
        &Policy::ProjectConfiguredCommands {
            allowed_programs: vec!["cargo".to_owned()],
        },
    );

    assert_eq!(
        result,
        Err(PatchwrightError::PolicyDenied(
            "command denied by policy: git".to_owned()
        ))
    );
}

#[test]
fn read_file_rejects_symlink_escape_when_supported() -> Result<()> {
    let repo = TempRepo::new("exec-local-symlink-read");
    repo.write("README.md", "inside\n");
    repo.commit_all("seed");

    let outside = repo.root().with_extension("outside.txt");
    fs::write(&outside, "outside\n")?;

    let link = repo.root().join("escape.txt");
    if symlink_file(&outside, &link).is_err() {
        let _ = fs::remove_file(&outside);
        return Ok(());
    }

    let execution = LocalExecution::new(repo.root());
    let result = execution.read_file(RepoPath::new("escape.txt"), None);

    let _ = fs::remove_file(&outside);

    assert!(matches!(result, Err(PatchwrightError::InvalidInput(_))));

    Ok(())
}

#[test]
fn run_rejects_cwd_escape() -> Result<()> {
    let repo = TempRepo::new("exec-local-cwd-escape");
    repo.write("README.md", "cwd\n");
    repo.commit_all("seed");
    let mut execution = LocalExecution::new(repo.root());

    let mut command = CommandSpec::new("git", ["status", "--short"]);
    command.cwd = Some(RepoPath::new("../"));

    let result = execution.run(command, &allow_git());

    assert!(matches!(result, Err(PatchwrightError::InvalidInput(_))));

    Ok(())
}

#[test]
fn run_drains_substantial_stdout_without_false_timeout() -> Result<()> {
    let repo = TempRepo::new("exec-local-large-stdout");
    let large_content = "x".repeat(200_000);
    repo.write("big.txt", &large_content);
    repo.commit_all("seed");
    let mut execution = LocalExecution::new(repo.root());

    let mut command = CommandSpec::new("git", ["cat-file", "blob", "HEAD:big.txt"]);
    command.timeout = Duration::from_secs(5);

    let report = execution.run(command, &allow_git())?;

    assert!(report.status.success);
    assert_eq!(report.stdout, large_content);
    assert_eq!(report.stderr, "");

    Ok(())
}

#[test]
fn run_times_out_and_reports_timeout() -> Result<()> {
    let repo = TempRepo::new("exec-local-timeout");
    repo.write("README.md", "timeout\n");
    repo.commit_all("seed");
    let mut execution = LocalExecution::new(repo.root());

    let mut command = sleep_command();
    command.timeout = Duration::from_millis(100);

    let report = execution.run(command, &allow_sleep_program())?;

    assert!(!report.status.success);
    assert!(report.stderr.contains("command timed out"));

    Ok(())
}

#[test]
fn apply_patch_failure_returns_useful_error() {
    let repo = TempRepo::new("exec-local-apply-failure");
    repo.write("notes.txt", "old\n");
    repo.commit_all("seed");
    let mut execution = LocalExecution::new(repo.root());

    let result = execution.apply_patch(Patch {
        unified_diff: "not a patch\n".to_owned(),
    });

    match result {
        Err(PatchwrightError::CommandFailed(message)) => {
            assert!(message.contains("git apply failed"));
            assert!(message.contains("No valid patches") || message.contains("patch"));
        }
        other => panic!("expected command failure, got {other:?}"),
    }
}

fn allow_git() -> Policy {
    Policy::ProjectConfiguredCommands {
        allowed_programs: vec!["git".to_owned()],
    }
}

#[cfg(windows)]
fn sleep_command() -> CommandSpec {
    CommandSpec::new(
        "powershell",
        ["-NoProfile", "-Command", "Start-Sleep -Seconds 5"],
    )
}

#[cfg(not(windows))]
fn sleep_command() -> CommandSpec {
    CommandSpec::new("sh", ["-c", "sleep 5"])
}

#[cfg(windows)]
fn allow_sleep_program() -> Policy {
    Policy::ProjectConfiguredCommands {
        allowed_programs: vec!["powershell".to_owned()],
    }
}

#[cfg(not(windows))]
fn allow_sleep_program() -> Policy {
    Policy::ProjectConfiguredCommands {
        allowed_programs: vec!["sh".to_owned()],
    }
}

#[cfg(windows)]
fn symlink_file(original: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(original, link)
}

#[cfg(not(windows))]
fn symlink_file(original: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(original, link)
}
