use crate::types::{CommandSpec, RepoPath};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Policy {
    SafeStructuredOnly,
    ProjectConfiguredCommands { allowed_programs: Vec<String> },
    AllowlistedShell { allowed_programs: Vec<String> },
    FullShellWithConfirmation,
    FullShellAutonomous,
}

impl Policy {
    pub fn allows(&self, command: &CommandSpec) -> bool {
        match self {
            Self::SafeStructuredOnly => false,
            Self::ProjectConfiguredCommands { allowed_programs }
            | Self::AllowlistedShell { allowed_programs } => allowed_programs
                .iter()
                .any(|program| program == &command.program),
            Self::FullShellWithConfirmation => false,
            Self::FullShellAutonomous => true,
        }
    }

    pub fn allows_repo_path(&self, path: &RepoPath) -> bool {
        !is_forbidden_repo_path(path)
    }
}

pub fn is_forbidden_repo_path(path: &RepoPath) -> bool {
    let normalized = path.0.replace('\\', "/");
    let trimmed = normalized.trim_matches('/');

    trimmed == ".git"
        || trimmed.starts_with(".git/")
        || trimmed == ".hg"
        || trimmed.starts_with(".hg/")
        || trimmed == ".svn"
        || trimmed.starts_with(".svn/")
        || trimmed == ".env"
        || trimmed.starts_with(".env.")
        || trimmed.ends_with("/.env")
        || trimmed.contains("/.env.")
}

#[cfg(test)]
mod tests {
    use crate::policy::Policy;
    use crate::types::{CommandSpec, RepoPath};

    #[test]
    fn safe_structured_policy_denies_processes() {
        let policy = Policy::SafeStructuredOnly;
        let command = CommandSpec::new("cargo", ["test"]);
        assert!(!policy.allows(&command));
    }

    #[test]
    fn project_configured_policy_allows_listed_programs() {
        let policy = Policy::ProjectConfiguredCommands {
            allowed_programs: vec!["cargo".to_owned()],
        };
        let command = CommandSpec::new("cargo", ["test"]);
        assert!(policy.allows(&command));
    }

    #[test]
    fn default_path_policy_rejects_sensitive_paths() {
        let policy = Policy::FullShellAutonomous;

        assert!(!policy.allows_repo_path(&RepoPath::new(".git/config")));
        assert!(!policy.allows_repo_path(&RepoPath::new(".env")));
        assert!(!policy.allows_repo_path(&RepoPath::new("nested/.env.local")));
        assert!(policy.allows_repo_path(&RepoPath::new(".github/workflows/ci.yml")));
    }
}
