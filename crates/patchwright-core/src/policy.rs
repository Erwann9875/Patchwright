use crate::types::CommandSpec;

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
}

#[cfg(test)]
mod tests {
    use crate::policy::Policy;
    use crate::types::CommandSpec;

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
}
