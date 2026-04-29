use patchwright_core::traits::LanguageAdapter;
use patchwright_core::types::{
    CommandSpec, DetectionScore, ExitStatus, RepoView, RunReport, TaskSpec,
};
use patchwright_index::BasicIndexer;
use patchwright_lang_rust::RustAdapter;
use patchwright_test_support::TempRepo;

fn cargo_repo(name: &str) -> TempRepo {
    let repo = TempRepo::new(name);
    repo.write(
        "Cargo.toml",
        r#"[package]
name = "sample"
version = "0.1.0"
edition = "2021"
"#,
    );
    repo.write("src/lib.rs", "pub fn answer() -> u8 { 42 }\n");
    repo
}

#[test]
fn detects_cargo_project_with_full_score() {
    let repo = cargo_repo("rust-detect");
    let view = RepoView {
        root: repo.root().to_path_buf(),
    };

    let score = RustAdapter::default().detect(&view);

    assert_eq!(score, DetectionScore(100));
}

#[test]
fn detects_non_cargo_project_with_zero_score() {
    let repo = TempRepo::new("rust-detect-missing-cargo");
    repo.write("src/lib.rs", "pub fn answer() -> u8 { 42 }\n");
    let view = RepoView {
        root: repo.root().to_path_buf(),
    };

    let score = RustAdapter::default().detect(&view);

    assert_eq!(score, DetectionScore(0));
}

#[test]
fn verifier_plan_includes_default_cargo_checks() {
    let repo = cargo_repo("rust-plan");
    let view = RepoView {
        root: repo.root().to_path_buf(),
    };
    let task = TaskSpec::from_text(repo.root().to_path_buf(), "verify rust project");

    let plan = RustAdapter::default().verifier_plan(&task, &view);

    let commands = plan
        .commands
        .iter()
        .map(|command| {
            (
                command.program.as_str(),
                command.args.iter().map(String::as_str).collect::<Vec<_>>(),
                command.timeout.as_secs(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        commands,
        vec![
            ("cargo", vec!["fmt", "--check"], 300),
            ("cargo", vec!["check"], 300),
            ("cargo", vec!["test"], 300),
        ]
    );
}

#[test]
fn relevant_files_scores_rust_and_test_paths() {
    let repo = cargo_repo("rust-relevant");
    repo.write("tests/integration_test.rs", "#[test]\nfn it_works() {}\n");
    repo.write("README.md", "sample\n");
    let indexer = BasicIndexer::new(repo.root());
    let task = TaskSpec::from_text(repo.root().to_path_buf(), "fix failing test");

    let files = RustAdapter::default().relevant_files(&task, &indexer);

    let scored = files
        .iter()
        .map(|file| (file.path.0.as_str(), file.score))
        .collect::<Vec<_>>();
    assert!(scored.contains(&("src/lib.rs", 6)));
    assert!(scored.contains(&("tests/integration_test.rs", 16)));
    assert!(scored.contains(&("README.md", 1)));
}

#[test]
fn summarize_failure_prefers_stderr_and_limits_to_twenty_lines() {
    let stderr = (1..=25)
        .map(|line| format!("stderr {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let report = RunReport {
        command: CommandSpec::new("cargo", ["test"]),
        status: ExitStatus {
            code: Some(101),
            success: false,
        },
        stdout: "stdout fallback".to_string(),
        stderr,
    };

    let counterexamples = RustAdapter::default().summarize_failure(&report);

    assert_eq!(counterexamples.len(), 1);
    assert_eq!(counterexamples[0].source, "cargo");
    assert_eq!(counterexamples[0].detail.lines().count(), 20);
    assert!(counterexamples[0].detail.contains("stderr 1"));
    assert!(!counterexamples[0].detail.contains("stderr 21"));
}

#[test]
fn summarize_failure_falls_back_to_stdout_when_stderr_is_whitespace() {
    let report = RunReport {
        command: CommandSpec::new("cargo", ["test"]),
        status: ExitStatus {
            code: Some(101),
            success: false,
        },
        stdout: "stdout failure".to_string(),
        stderr: " \n\t\n".to_string(),
    };

    let counterexamples = RustAdapter::default().summarize_failure(&report);

    assert_eq!(counterexamples.len(), 1);
    assert_eq!(counterexamples[0].detail, "stdout failure");
}

#[test]
fn summarize_failure_returns_empty_for_success() {
    let report = RunReport {
        command: CommandSpec::new("cargo", ["check"]),
        status: ExitStatus {
            code: Some(0),
            success: true,
        },
        stdout: String::new(),
        stderr: String::new(),
    };

    let counterexamples = RustAdapter::default().summarize_failure(&report);

    assert!(counterexamples.is_empty());
}
