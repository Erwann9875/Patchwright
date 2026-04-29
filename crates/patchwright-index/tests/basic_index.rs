use patchwright_core::traits::Indexer;
use patchwright_core::types::{FileQuery, RepoPath, SearchQuery};
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
