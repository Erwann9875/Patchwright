#![forbid(unsafe_code)]

use patchwright_core::error::PatchwrightError;
use patchwright_core::traits::Indexer;
use patchwright_core::types::{
    FileQuery, RepoPath, ScoredPath, SearchMatch, SearchQuery, SearchResults, Symbol,
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

        Self {
            root,
        }
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
}

impl BasicIndexer {
    fn query_root(&self, root: Option<&RepoPath>) -> Result<PathBuf> {
        match root {
            Some(root) => self.resolve_existing_root(root),
            None => Ok(self.root.clone()),
        }
    }

    fn walk_files(&self, directory: &Path, visit: &mut dyn FnMut(&Path) -> Result<()>) -> Result<()> {
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
