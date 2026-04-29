#![forbid(unsafe_code)]

use patchwright_core::action::Observation;
use patchwright_core::error::PatchwrightError;
use patchwright_core::traits::Indexer;
use patchwright_core::types::{
    ContextPack, Counterexample, FileQuery, RepoPath, ScoredPath, SearchMatch, SearchQuery,
    SearchResults, Symbol, TaskSpec,
};
use patchwright_core::Result;
use std::ffi::OsStr;
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone)]
pub struct BasicIndexer {
    root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectProfile {
    pub detected_languages: Vec<String>,
    pub manifests: Vec<RepoPath>,
    pub package_managers: Vec<String>,
    pub install_commands: Vec<String>,
    pub build_commands: Vec<String>,
    pub test_commands: Vec<String>,
    pub typecheck_commands: Vec<String>,
    pub lint_commands: Vec<String>,
    pub source_roots: Vec<RepoPath>,
    pub test_roots: Vec<RepoPath>,
    pub ci_files: Vec<RepoPath>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ArchitectureMap {
    pub manifests: Vec<RepoPath>,
    pub source_roots: Vec<RepoPath>,
    pub test_roots: Vec<RepoPath>,
    pub route_files: Vec<RepoPath>,
    pub model_files: Vec<RepoPath>,
    pub service_files: Vec<RepoPath>,
    pub config_files: Vec<RepoPath>,
    pub migration_files: Vec<RepoPath>,
    pub ci_files: Vec<RepoPath>,
}

impl BasicIndexer {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = fs::canonicalize(root.as_ref())
            .unwrap_or_else(|error| panic!("failed to canonicalize repo root: {error}"));

        Self { root }
    }
}

pub fn profile_project(root: impl AsRef<Path>) -> Result<ProjectProfile> {
    let indexer = BasicIndexer::new(root);
    let files = indexer.list_files(FileQuery::default())?;
    let paths = files
        .iter()
        .map(|file| file.path.0.as_str())
        .collect::<Vec<_>>();

    let mut profile = ProjectProfile {
        manifests: files
            .iter()
            .filter(|file| is_project_manifest(&file.path.0))
            .map(|file| file.path.clone())
            .collect(),
        source_roots: detect_roots(&paths, source_root_candidates()),
        test_roots: detect_roots(&paths, test_root_candidates()),
        ci_files: files
            .iter()
            .filter(|file| is_ci_file(&file.path.0))
            .map(|file| file.path.clone())
            .collect(),
        ..ProjectProfile::default()
    };

    detect_rust_profile(&paths, &mut profile);
    detect_javascript_profile(&paths, &mut profile);
    detect_python_profile(&paths, &mut profile);
    detect_go_profile(&paths, &mut profile);
    detect_java_profile(&paths, &mut profile);
    detect_dotnet_profile(&paths, &mut profile);
    detect_php_profile(&paths, &mut profile);
    detect_ruby_profile(&paths, &mut profile);
    detect_cpp_profile(&paths, &mut profile);
    detect_terraform_profile(&paths, &mut profile);

    Ok(profile)
}

pub fn architecture_map(root: impl AsRef<Path>) -> Result<ArchitectureMap> {
    let profile = profile_project(root.as_ref())?;
    let indexer = BasicIndexer::new(root);
    let files = indexer.list_files(FileQuery::default())?;

    Ok(ArchitectureMap {
        manifests: profile.manifests,
        source_roots: profile.source_roots,
        test_roots: profile.test_roots,
        route_files: files_matching(&files, is_route_file),
        model_files: files_matching(&files, is_model_file),
        service_files: files_matching(&files, is_service_file),
        config_files: files_matching(&files, is_config_file),
        migration_files: files_matching(&files, is_migration_file),
        ci_files: profile.ci_files,
    })
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

fn detect_rust_profile(paths: &[&str], profile: &mut ProjectProfile) {
    if !has_path(paths, "Cargo.toml") {
        return;
    }

    push_unique(&mut profile.detected_languages, "Rust");
    push_unique(&mut profile.package_managers, "cargo");
    push_unique(&mut profile.build_commands, "cargo build");
    push_unique(&mut profile.test_commands, "cargo test");
    push_unique(&mut profile.typecheck_commands, "cargo check");
    push_unique(&mut profile.lint_commands, "cargo clippy");
}

fn detect_javascript_profile(paths: &[&str], profile: &mut ProjectProfile) {
    if !has_path(paths, "package.json") && !has_path(paths, "tsconfig.json") {
        return;
    }

    if has_path(paths, "tsconfig.json") || paths.iter().any(|path| path.ends_with(".ts")) {
        push_unique(&mut profile.detected_languages, "TypeScript");
    }
    if has_path(paths, "package.json") {
        push_unique(&mut profile.detected_languages, "JavaScript");
    }

    let package_manager = javascript_package_manager(paths);
    push_unique(&mut profile.package_managers, package_manager);
    push_unique(
        &mut profile.install_commands,
        format!("{package_manager} install"),
    );
    push_unique(
        &mut profile.build_commands,
        format!("{package_manager} build"),
    );
    push_unique(
        &mut profile.test_commands,
        format!("{package_manager} test"),
    );
    push_unique(
        &mut profile.typecheck_commands,
        format!("{package_manager} tsc --noEmit"),
    );
    push_unique(
        &mut profile.lint_commands,
        format!("{package_manager} lint"),
    );
}

fn detect_python_profile(paths: &[&str], profile: &mut ProjectProfile) {
    if !has_path(paths, "pyproject.toml") && !has_path(paths, "requirements.txt") {
        return;
    }

    push_unique(&mut profile.detected_languages, "Python");
    let package_manager = python_package_manager(paths);
    push_unique(&mut profile.package_managers, package_manager);
    match package_manager {
        "uv" => push_unique(&mut profile.install_commands, "uv sync"),
        "poetry" => push_unique(&mut profile.install_commands, "poetry install"),
        _ => push_unique(
            &mut profile.install_commands,
            "python -m pip install -r requirements.txt",
        ),
    }
    push_unique(&mut profile.test_commands, "python -m pytest");
    push_unique(&mut profile.typecheck_commands, "python -m mypy .");
    push_unique(&mut profile.lint_commands, "python -m ruff check .");
}

fn detect_go_profile(paths: &[&str], profile: &mut ProjectProfile) {
    if !has_path(paths, "go.mod") {
        return;
    }

    push_unique(&mut profile.detected_languages, "Go");
    push_unique(&mut profile.package_managers, "go");
    push_unique(&mut profile.build_commands, "go build ./...");
    push_unique(&mut profile.test_commands, "go test ./...");
    push_unique(&mut profile.lint_commands, "go vet ./...");
}

fn detect_java_profile(paths: &[&str], profile: &mut ProjectProfile) {
    if has_path(paths, "pom.xml") {
        push_unique(&mut profile.detected_languages, "Java");
        push_unique(&mut profile.package_managers, "maven");
        push_unique(&mut profile.build_commands, "mvn package");
        push_unique(&mut profile.test_commands, "mvn test");
    }
    if has_path(paths, "build.gradle") || has_path(paths, "build.gradle.kts") {
        push_unique(&mut profile.detected_languages, "Java");
        push_unique(&mut profile.package_managers, "gradle");
        push_unique(&mut profile.build_commands, "gradle build");
        push_unique(&mut profile.test_commands, "gradle test");
    }
}

fn detect_dotnet_profile(paths: &[&str], profile: &mut ProjectProfile) {
    if !paths
        .iter()
        .any(|path| path.ends_with(".csproj") || path.ends_with(".sln"))
    {
        return;
    }

    push_unique(&mut profile.detected_languages, ".NET");
    push_unique(&mut profile.package_managers, "dotnet");
    push_unique(&mut profile.build_commands, "dotnet build");
    push_unique(&mut profile.test_commands, "dotnet test");
}

fn detect_php_profile(paths: &[&str], profile: &mut ProjectProfile) {
    if !has_path(paths, "composer.json") {
        return;
    }

    push_unique(&mut profile.detected_languages, "PHP");
    push_unique(&mut profile.package_managers, "composer");
    push_unique(&mut profile.install_commands, "composer install");
    push_unique(&mut profile.test_commands, "composer test");
}

fn detect_ruby_profile(paths: &[&str], profile: &mut ProjectProfile) {
    if !has_path(paths, "Gemfile") {
        return;
    }

    push_unique(&mut profile.detected_languages, "Ruby");
    push_unique(&mut profile.package_managers, "bundler");
    push_unique(&mut profile.install_commands, "bundle install");
    push_unique(&mut profile.test_commands, "bundle exec rspec");
}

fn detect_cpp_profile(paths: &[&str], profile: &mut ProjectProfile) {
    if !has_path(paths, "CMakeLists.txt") {
        return;
    }

    push_unique(&mut profile.detected_languages, "C/C++");
    push_unique(&mut profile.package_managers, "cmake");
    push_unique(&mut profile.build_commands, "cmake --build build");
    push_unique(&mut profile.test_commands, "ctest --test-dir build");
}

fn detect_terraform_profile(paths: &[&str], profile: &mut ProjectProfile) {
    if !paths.iter().any(|path| path.ends_with(".tf")) {
        return;
    }

    push_unique(&mut profile.detected_languages, "Terraform");
    push_unique(&mut profile.package_managers, "terraform");
    push_unique(&mut profile.build_commands, "terraform validate");
    push_unique(&mut profile.lint_commands, "terraform fmt -check");
}

fn javascript_package_manager(paths: &[&str]) -> &'static str {
    if has_path(paths, "pnpm-lock.yaml") {
        "pnpm"
    } else if has_path(paths, "yarn.lock") {
        "yarn"
    } else {
        "npm"
    }
}

fn python_package_manager(paths: &[&str]) -> &'static str {
    if has_path(paths, "uv.lock") {
        "uv"
    } else if has_path(paths, "poetry.lock") {
        "poetry"
    } else {
        "pip"
    }
}

fn detect_roots(paths: &[&str], candidates: &[&str]) -> Vec<RepoPath> {
    candidates
        .iter()
        .filter(|candidate| {
            paths
                .iter()
                .any(|path| path_starts_with_root(path, candidate))
        })
        .map(|candidate| RepoPath::new(*candidate))
        .collect()
}

fn source_root_candidates() -> &'static [&'static str] {
    &["app", "cmd", "crates", "lib", "pages", "pkg", "src"]
}

fn test_root_candidates() -> &'static [&'static str] {
    &["__tests__", "spec", "test", "tests"]
}

fn path_starts_with_root(path: &str, root: &str) -> bool {
    path == root || path.starts_with(&format!("{root}/"))
}

fn is_project_manifest(path: &str) -> bool {
    matches!(
        path,
        "Cargo.toml"
            | "package.json"
            | "package-lock.json"
            | "pnpm-lock.yaml"
            | "yarn.lock"
            | "tsconfig.json"
            | "pyproject.toml"
            | "requirements.txt"
            | "uv.lock"
            | "poetry.lock"
            | "go.mod"
            | "pom.xml"
            | "build.gradle"
            | "build.gradle.kts"
            | "composer.json"
            | "Gemfile"
            | "CMakeLists.txt"
    ) || path.ends_with(".csproj")
        || path.ends_with(".sln")
}

fn is_ci_file(path: &str) -> bool {
    path.starts_with(".github/workflows/")
        || path.starts_with(".gitlab-ci")
        || path == "Jenkinsfile"
        || path.starts_with(".circleci/")
}

fn files_matching(files: &[ScoredPath], predicate: fn(&str) -> bool) -> Vec<RepoPath> {
    files
        .iter()
        .filter(|file| predicate(&file.path.0))
        .map(|file| file.path.clone())
        .collect()
}

fn is_route_file(path: &str) -> bool {
    path.contains("/routes/")
        || path.contains("/controllers/")
        || path.contains("/api/")
        || path.ends_with("/route.ts")
        || path.ends_with("/route.tsx")
        || path.ends_with("/route.js")
        || path.ends_with("/route.jsx")
        || path.starts_with("routes/")
        || path.starts_with("controllers/")
        || path.starts_with("app/api/")
        || path.starts_with("pages/api/")
}

fn is_model_file(path: &str) -> bool {
    path.contains("/models/")
        || path.starts_with("models/")
        || path.ends_with("model.rs")
        || path.ends_with("model.py")
        || path.ends_with("schema.prisma")
}

fn is_service_file(path: &str) -> bool {
    path.contains("/services/")
        || path.starts_with("services/")
        || path.contains("_service.")
        || path.contains(".service.")
}

fn is_config_file(path: &str) -> bool {
    path == "tsconfig.json"
        || path == "pyproject.toml"
        || path == "Cargo.toml"
        || path == "package.json"
        || path == "go.mod"
        || path.ends_with(".config.js")
        || path.ends_with(".config.ts")
        || path.ends_with(".config.cjs")
        || path.ends_with(".config.mjs")
        || path.ends_with("/config.py")
        || path.contains("/config/")
}

fn is_migration_file(path: &str) -> bool {
    path.contains("/migrations/")
        || path.starts_with("migrations/")
        || path.contains("/db/migrate/")
        || path.starts_with("db/migrate/")
}

fn has_path(paths: &[&str], expected: &str) -> bool {
    paths.contains(&expected)
}

fn push_unique(values: &mut Vec<String>, value: impl Into<String>) {
    let value = value.into();
    if !values.contains(&value) {
        values.push(value);
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
    if counterexamples_mention_path(path, counterexamples) {
        score += 100;
    }

    score
}

fn counterexamples_mention_path(path: &str, counterexamples: &[Counterexample]) -> bool {
    let normalized_path = path.to_ascii_lowercase();
    counterexamples.iter().any(|counterexample| {
        counterexample
            .detail
            .to_ascii_lowercase()
            .replace('\\', "/")
            .contains(&normalized_path)
    })
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
            let file_type = entry.file_type()?;
            let path = entry.path();

            if file_type.is_dir() {
                if should_skip_directory(&file_name) {
                    continue;
                }

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

fn should_skip_directory(file_name: &OsStr) -> bool {
    matches!(
        file_name.to_str(),
        Some(".git" | "target" | "node_modules" | ".next" | "dist" | "build" | "vendor")
    )
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
