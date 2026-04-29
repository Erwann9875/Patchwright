#![forbid(unsafe_code)]

use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_REPO_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    pub fn new(name: &str) -> Self {
        let root = create_unique_repo_dir(name);

        run_git(&root, ["init"]);
        run_git(
            &root,
            ["config", "user.email", "patchwright@example.invalid"],
        );
        run_git(&root, ["config", "user.name", "Patchwright Tests"]);

        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn write(&self, relative: &str, content: &str) {
        validate_relative_path(relative);

        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|error| panic!("failed to create {}: {error}", parent.display()));
        }
        fs::write(&path, content)
            .unwrap_or_else(|error| panic!("failed to write {}: {error}", path.display()));
    }

    pub fn commit_all(&self, message: &str) {
        run_git(&self.root, ["add", "."]);
        run_git(&self.root, ["commit", "-m", message]);
    }
}

fn create_unique_repo_dir(name: &str) -> PathBuf {
    for _ in 0..100 {
        let id = NEXT_REPO_ID.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("patchwright-{name}-{}-{id}", std::process::id()));

        match fs::create_dir(&root) {
            Ok(()) => return root,
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("failed to create temp repo {}: {error}", root.display()),
        }
    }

    panic!("failed to create unique temp repo for {name}: too many collisions");
}

fn validate_relative_path(relative: &str) {
    let path = Path::new(relative);

    if path.is_absolute() {
        panic!("temp repo path must be relative: {relative}");
    }

    for component in path.components() {
        if matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        ) {
            panic!("temp repo path must not escape repo root: {relative}");
        }
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        if self.root.exists() {
            fs::remove_dir_all(&self.root).unwrap_or_else(|error| {
                panic!(
                    "failed to remove temp repo {}: {error}",
                    self.root.display()
                )
            });
        }
    }
}

fn run_git<const N: usize>(root: &Path, args: [&str; N]) {
    let args_display = args.join(" ");
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("failed to run git in {}: {error}", root.display()));

    if !output.status.success() {
        panic!(
            "git failed in {} with args: {}\nstdout:\n{}\nstderr:\n{}",
            root.display(),
            args_display,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::TempRepo;
    use std::panic;
    use std::path::PathBuf;

    #[test]
    fn write_rejects_parent_traversal() {
        let repo = TempRepo::new("write-parent-traversal");

        let result = panic::catch_unwind(|| {
            repo.write("../escape.txt", "escape\n");
        });

        assert!(result.is_err());
    }

    #[test]
    fn write_rejects_absolute_paths() {
        let repo = TempRepo::new("write-absolute");
        let absolute = repo.root().join("absolute.txt");

        let result = panic::catch_unwind(|| {
            repo.write(absolute.to_str().unwrap(), "absolute\n");
        });

        assert!(result.is_err());
    }

    #[test]
    fn new_creates_distinct_roots_for_same_name() {
        let first = TempRepo::new("same-name");
        let second = TempRepo::new("same-name");

        assert_ne!(first.root(), second.root());
        assert!(PathBuf::from(first.root()).exists());
        assert!(PathBuf::from(second.root()).exists());
    }
}
