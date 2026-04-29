#![forbid(unsafe_code)]

use patchwright_core::action::Observation;
use patchwright_core::error::PatchwrightError;
use patchwright_core::traits::Indexer;
use patchwright_core::types::{
    ContextPack, Counterexample, FileQuery, RepoPath, ScoredPath, SearchMatch, SearchQuery,
    SearchResults, Symbol, TaskSpec,
};
use patchwright_core::Result;
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone)]
pub struct BasicIndexer {
    root: PathBuf,
}

impl BasicIndexer {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = fs::canonicalize(root.as_ref())
            .unwrap_or_else(|error| panic!("failed to canonicalize repo root: {error}"));

        Self { root }
    }
}

impl Indexer for BasicIndexer {
    fn list_files(&self, query: FileQuery) -> Result<Vec<ScoredPath>> {
        let search_root = self.query_root(query.root.as_ref())?;
        let mut paths = Vec::new();

        self.walk_files(&search_root, &mut |path| {
            paths.push(ScoredPath {
                path: RepoPath(relative_path(&self.root, path)?),
                score: 1,
            });
            Ok(())
        })?;

        paths.sort_by(|left, right| left.path.0.cmp(&right.path.0));
        Ok(paths)
    }

    fn search_text(&self, query: SearchQuery) -> Result<SearchResults> {
        let search_root = self.query_root(query.root.as_ref())?;
        let mut matches = Vec::new();

        self.walk_files(&search_root, &mut |path| {
            let content = match fs::read_to_string(path) {
                Ok(content) => content,
                Err(error) if error.kind() == ErrorKind::InvalidData => return Ok(()),
                Err(error) => return Err(error.into()),
            };

            let repo_path = RepoPath(relative_path(&self.root, path)?);
            for (index, line) in content.lines().enumerate() {
                if line.contains(&query.pattern) {
                    matches.push(SearchMatch {
                        path: repo_path.clone(),
                        line: index + 1,
                        text: line.to_string(),
                    });
                }
            }

            Ok(())
        })?;

        matches.sort_by(|left, right| {
            left.path
                .0
                .cmp(&right.path.0)
                .then_with(|| left.line.cmp(&right.line))
                .then_with(|| left.text.cmp(&right.text))
        });

        Ok(SearchResults { matches })
    }

    fn symbols(&self, _path: &RepoPath) -> Result<Vec<Symbol>> {
        Ok(Vec::new())
    }

    fn context_pack(
        &self,
        task: &TaskSpec,
        observations: &[Observation],
        counterexamples: &[Counterexample],
    ) -> Result<ContextPack> {
        let task_words = task_words(&task.text);
        let mut files = self.list_files(FileQuery::default())?;

        for file in &mut files {
            file.score = context_score(&file.path.0, &task_words, counterexamples);
        }

        files.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.path.0.cmp(&right.path.0))
        });

        let likely_tests = files
            .iter()
            .filter(|file| is_likely_test(&file.path.0))
            .map(|file| file.path.clone())
            .collect();
        let manifests = files
            .iter()
            .filter(|file| is_manifest(&file.path.0))
            .map(|file| file.path.clone())
            .collect();

        files.truncate(20);

        Ok(ContextPack {
            files,
            likely_tests,
            manifests,
            recent_observations: observations.iter().rev().take(8).cloned().collect(),
            counterexamples: counterexamples.to_vec(),
        })
    }
}

fn context_score(path: &str, task_words: &[String], counterexamples: &[Counterexample]) -> u16 {
    let lower_path = path.to_ascii_lowercase();
    let mut score = 1;

    if is_manifest(path) {
        score += 90;
    }
    if path.ends_with(".rs") {
        score += 30;
    }
    if is_likely_test(path) {
        score += 50;
    }
    for word in task_words {
        if lower_path.contains(word) {
            score += 40;
        }
    }
    if counterexamples
        .iter()
        .any(|counterexample| counterexample.detail.contains(path))
    {
        score += 100;
    }

    score
}

fn is_manifest(path: &str) -> bool {
    path == "Cargo.toml" || path.ends_with("/Cargo.toml")
}

fn is_likely_test(path: &str) -> bool {
    path.starts_with("tests/")
        || path.contains("/tests/")
        || path.contains("_test")
        || path.contains("test_")
}

fn task_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();

    for character in text.chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            current.push(character.to_ascii_lowercase());
        } else {
            push_task_word(&mut words, &mut current);
        }
    }
    push_task_word(&mut words, &mut current);

    words.sort();
    words.dedup();
    words
}

fn push_task_word(words: &mut Vec<String>, current: &mut String) {
    if current.len() >= 3 {
        words.push(std::mem::take(current));
    } else {
        current.clear();
    }
}

impl BasicIndexer {
    fn query_root(&self, root: Option<&RepoPath>) -> Result<PathBuf> {
        match root {
            Some(root) => self.resolve_existing_root(root),
            None => Ok(self.root.clone()),
        }
    }

    fn walk_files(
        &self,
        directory: &Path,
        visit: &mut dyn FnMut(&Path) -> Result<()>,
    ) -> Result<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let file_name = entry.file_name();

            if file_name == ".git" {
                continue;
            }

            let file_type = entry.file_type()?;
            let path = entry.path();

            if file_type.is_dir() {
                let canonical = fs::canonicalize(&path)?;
                if canonical.starts_with(&self.root) {
                    self.walk_files(&canonical, visit)?;
                }
            } else if file_type.is_file() {
                visit(&path)?;
            }
        }

        Ok(())
    }

    fn resolve_existing_root(&self, path: &RepoPath) -> Result<PathBuf> {
        let resolved = self.root.join(validate_repo_path(path)?);
        let canonical = fs::canonicalize(&resolved)?;

        if canonical.starts_with(&self.root) {
            Ok(canonical)
        } else {
            Err(PatchwrightError::InvalidInput(format!(
                "repo path must not escape repo root: {}",
                path.0
            )))
        }
    }
}

fn validate_repo_path(path: &RepoPath) -> Result<PathBuf> {
    let path = Path::new(&path.0);

    if path.as_os_str().is_empty() {
        return Ok(PathBuf::new());
    }

    let mut validated = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => validated.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(PatchwrightError::InvalidInput(format!(
                    "repo path must be relative and stay within the repository: {}",
                    path.display()
                )));
            }
        }
    }

    Ok(validated)
}

fn relative_path(root: &Path, path: &Path) -> Result<String> {
    let relative = path.strip_prefix(root).map_err(|error| {
        PatchwrightError::InvalidInput(format!(
            "indexed path is outside repository root: {} ({error})",
            path.display()
        ))
    })?;

    Ok(relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/"))
}
