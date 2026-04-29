use patchwright_core::traits::Indexer;
use patchwright_core::types::{Counterexample, FileQuery, RepoPath, SearchQuery, TaskSpec};
use patchwright_core::PatchwrightError;
use patchwright_index::BasicIndexer;
use patchwright_test_support::TempRepo;
use std::fs;
use std::path::Path;
#[cfg(windows)]
use std::process::Command;

#[test]
fn lists_files_and_searches_text() {
    let repo = TempRepo::new("basic-index");
    repo.write("src/lib.rs", "pub fn target() {}\n");
    repo.write("README.md", "# Test Repo\n");

    let indexer = BasicIndexer::new(repo.root());

    let files = indexer.list_files(FileQuery::default()).unwrap();
    let paths: Vec<_> = files.iter().map(|file| file.path.0.as_str()).collect();
    assert!(paths.contains(&"src/lib.rs"));
    assert!(paths.contains(&"README.md"));

    let results = indexer
        .search_text(SearchQuery {
            pattern: "target".to_string(),
            root: None,
        })
        .unwrap();

    assert_eq!(results.matches.len(), 1);
    assert_eq!(results.matches[0].path.0, "src/lib.rs");
    assert_eq!(results.matches[0].line, 1);
    assert_eq!(results.matches[0].text, "pub fn target() {}");
}

#[test]
fn skips_git_directory_when_listing_and_searching() {
    let repo = TempRepo::new("basic-index-git-skip");
    repo.write("src/lib.rs", "pub fn visible() {}\n");
    repo.write(".git/secret.txt", "target\n");

    let indexer = BasicIndexer::new(repo.root());

    let files = indexer.list_files(FileQuery::default()).unwrap();
    assert!(!files.iter().any(|file| file.path.0.starts_with(".git/")));

    let results = indexer
        .search_text(SearchQuery {
            pattern: "target".to_string(),
            root: None,
        })
        .unwrap();

    assert!(results.matches.is_empty());
}

#[test]
fn skips_generated_and_vendor_directories_when_listing_and_ranking_context() {
    let repo = TempRepo::new("basic-index-generated-skip");
    repo.write("src/lib.rs", "pub fn visible() {}\n");
    repo.write(".git/secret.txt", "pub fn hidden() {}\n");
    repo.write("target/debug/generated.rs", "pub fn hidden() {}\n");
    repo.write("node_modules/pkg/index.js", "export const hidden = true;\n");
    repo.write(".next/cache/page.js", "export const hidden = true;\n");
    repo.write("dist/app.js", "export const hidden = true;\n");
    repo.write("build/output.rs", "pub fn hidden() {}\n");
    repo.write("vendor/lib.rs", "pub fn hidden() {}\n");

    let indexer = BasicIndexer::new(repo.root());
    let files = indexer.list_files(FileQuery::default()).unwrap();
    let listed_paths = files
        .iter()
        .map(|file| file.path.0.as_str())
        .collect::<Vec<_>>();

    assert_eq!(listed_paths, vec!["src/lib.rs"]);

    let pack = indexer
        .context_pack(
            &TaskSpec::from_text(repo.root().to_path_buf(), "inspect generated output"),
            &[],
            &[],
        )
        .unwrap();

    assert_eq!(
        pack.files
            .iter()
            .map(|file| file.path.0.as_str())
            .collect::<Vec<_>>(),
        vec!["src/lib.rs"]
    );
}

#[test]
fn ignores_non_utf8_files_when_searching() {
    let repo = TempRepo::new("basic-index-non-utf8");
    repo.write("src/lib.rs", "pub fn target() {}\n");
    fs::write(
        repo.root().join("binary.dat"),
        [0xff, 0xfe, b't', b'a', b'r', b'g', b'e', b't'],
    )
    .unwrap();

    let indexer = BasicIndexer::new(repo.root());

    let results = indexer
        .search_text(SearchQuery {
            pattern: "target".to_string(),
            root: None,
        })
        .unwrap();

    assert_eq!(results.matches.len(), 1);
    assert_eq!(results.matches[0].path.0, "src/lib.rs");
}

#[test]
fn search_text_respects_scoped_root() {
    let repo = TempRepo::new("basic-index-scoped-root");
    repo.write("src/lib.rs", "pub fn target() {}\n");
    repo.write("tests/basic.rs", "fn target() {}\n");

    let indexer = BasicIndexer::new(repo.root());

    let results = indexer
        .search_text(SearchQuery {
            pattern: "target".to_string(),
            root: Some(RepoPath::new("tests")),
        })
        .unwrap();

    assert_eq!(results.matches.len(), 1);
    assert_eq!(results.matches[0].path.0, "tests/basic.rs");
}

#[test]
fn rejects_parent_traversal_roots() {
    let repo = TempRepo::new("basic-index-parent-root");
    let indexer = BasicIndexer::new(repo.root());

    let result = indexer.list_files(FileQuery {
        root: Some(RepoPath::new("..")),
    });

    assert!(matches!(result, Err(PatchwrightError::InvalidInput(_))));
}

#[test]
fn rejects_symlink_directory_escape_as_query_root_when_supported() {
    let repo = TempRepo::new("basic-index-symlink-root");
    let outside = repo.root().with_extension("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("secret.txt"), "target\n").unwrap();

    let link = repo.root().join("escape");
    if link_escape_dir(&outside, &link).is_err() {
        fs::remove_dir_all(&outside).unwrap();
        return;
    }

    let indexer = BasicIndexer::new(repo.root());
    let result = indexer.list_files(FileQuery {
        root: Some(RepoPath::new("escape")),
    });

    cleanup_link_dir(&link).unwrap();
    fs::remove_dir_all(&outside).unwrap();

    assert!(matches!(result, Err(PatchwrightError::InvalidInput(_))));
}

#[test]
fn skips_symlink_directory_escape_during_traversal_when_supported() {
    let repo = TempRepo::new("basic-index-symlink-traversal");
    repo.write("src/lib.rs", "pub fn visible() {}\n");
    let outside = repo.root().with_extension("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("secret.txt"), "target\n").unwrap();

    let link = repo.root().join("escape");
    if link_escape_dir(&outside, &link).is_err() {
        fs::remove_dir_all(&outside).unwrap();
        return;
    }

    let indexer = BasicIndexer::new(repo.root());
    let files = indexer.list_files(FileQuery::default()).unwrap();
    let results = indexer
        .search_text(SearchQuery {
            pattern: "target".to_string(),
            root: None,
        })
        .unwrap();

    cleanup_link_dir(&link).unwrap();
    fs::remove_dir_all(&outside).unwrap();

    assert!(!files.iter().any(|file| file.path.0 == "escape/secret.txt"));
    assert!(results.matches.is_empty());
}

#[test]
fn slash_normalizes_nested_paths() {
    let repo = TempRepo::new("basic-index-normalized-paths");
    repo.write("src/nested/mod.rs", "pub fn target() {}\n");

    let indexer = BasicIndexer::new(repo.root());

    let files = indexer.list_files(FileQuery::default()).unwrap();
    assert!(files.iter().any(|file| file.path.0 == "src/nested/mod.rs"));
}

#[test]
fn context_pack_prioritizes_rust_sources_tests_and_manifests() {
    let repo = TempRepo::new("basic-index-context-pack");
    repo.write("Cargo.toml", "[package]\nname = \"context-pack\"\n");
    repo.write(
        "src/lib.rs",
        "pub fn parse_user(input: &str) -> String { input.into() }\n",
    );
    repo.write(
        "tests/parser_test.rs",
        "#[test]\nfn parse_user_accepts_name() {}\n",
    );
    repo.write("README.md", "# context pack\n");

    let indexer = BasicIndexer::new(repo.root());
    let task = TaskSpec::from_text(repo.root().to_path_buf(), "fix parser parse_user failure");
    let counterexamples = vec![Counterexample {
        source: "cargo test".to_string(),
        detail: "tests/parser_test.rs parse_user_accepts_name failed".to_string(),
    }];

    let pack = indexer.context_pack(&task, &[], &counterexamples).unwrap();

    assert_eq!(pack.manifests, vec![RepoPath::new("Cargo.toml")]);
    assert_eq!(
        pack.likely_tests,
        vec![RepoPath::new("tests/parser_test.rs")]
    );
    assert_eq!(pack.files[0].path, RepoPath::new("tests/parser_test.rs"));
    assert!(
        pack.files
            .iter()
            .any(|file| file.path == RepoPath::new("src/lib.rs")),
        "ranked context should include Rust sources"
    );
    assert!(
        pack.files
            .iter()
            .any(|file| file.path == RepoPath::new("Cargo.toml")),
        "ranked context should include manifests"
    );
}

#[test]
fn context_pack_boosts_counterexample_paths_with_backslashes_and_mixed_case() {
    let repo = TempRepo::new("basic-index-counterexample-normalized");
    repo.write("Cargo.toml", "[package]\nname = \"context-pack\"\n");
    repo.write("src/lib.rs", "pub fn parse_user() {}\n");
    repo.write("README.md", "# context pack\n");

    let indexer = BasicIndexer::new(repo.root());
    let task = TaskSpec::from_text(repo.root().to_path_buf(), "fix failing check");
    let counterexamples = vec![Counterexample {
        source: "cargo test".to_string(),
        detail: "failure in SRC\\LIB.RS: parse_user panicked".to_string(),
    }];

    let pack = indexer.context_pack(&task, &[], &counterexamples).unwrap();

    assert_eq!(pack.files[0].path, RepoPath::new("src/lib.rs"));
    assert!(
        pack.files[0].score > pack.files[1].score,
        "counterexample path mention should outrank manifest defaults"
    );
}

#[cfg(windows)]
fn link_escape_dir(original: &Path, link: &Path) -> std::io::Result<()> {
    if std::os::windows::fs::symlink_dir(original, link).is_ok() {
        return Ok(());
    }

    let output = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(original)
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "failed to create junction: {}",
            String::from_utf8_lossy(&output.stderr)
        )))
    }
}

#[cfg(windows)]
fn cleanup_link_dir(link: &Path) -> std::io::Result<()> {
    fs::remove_dir(link)
}

#[cfg(not(windows))]
fn link_escape_dir(original: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(original, link)
}

#[cfg(not(windows))]
fn cleanup_link_dir(link: &Path) -> std::io::Result<()> {
    fs::remove_file(link)
}
