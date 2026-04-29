use patchwright_core::agent::SolveReport;
use patchwright_core::types::{ImplementationGraph, PlanStep, RepoPath};
use std::fs;
use std::path::{Path, PathBuf};
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StepScope {
    pub(crate) target_files: Vec<RepoPath>,
}

pub(crate) fn load_implementation_graph(
    repo: &Path,
    from: &str,
) -> Result<ImplementationGraph, String> {
    let path = resolve_plan_path(repo, from);
    let content = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read plan {}: {error}", path.display()))?;

    if content.trim_start().starts_with('{') {
        return patchwright_model_contract::parse_implementation_graph_json(&content)
            .map_err(|error| error.to_string());
    }

    parse_implementation_graph_markdown(&content)
}

fn resolve_plan_path(repo: &Path, from: &str) -> PathBuf {
    let path = PathBuf::from(from);
    if path.is_absolute() {
        return path;
    }

    let repo_path = repo.join(&path);
    if repo_path.exists() {
        repo_path
    } else {
        path
    }
}

pub(crate) fn parse_implementation_graph_markdown(
    content: &str,
) -> Result<ImplementationGraph, String> {
    let mut steps = Vec::new();
    let mut current: Option<PartialPlanStep> = None;

    for line in content.lines() {
        if let Some((id, title)) = parse_plan_step_heading(line) {
            if let Some(step) = current.take() {
                steps.push(step.finish());
            }
            current = Some(PartialPlanStep::new(id, title));
            continue;
        }

        if let Some(step) = current.as_mut() {
            step.push_line(line);
        }
    }

    if let Some(step) = current {
        steps.push(step.finish());
    }

    if steps.is_empty() {
        return Err("implementation plan did not contain any steps".to_owned());
    }

    Ok(ImplementationGraph { steps })
}

fn parse_plan_step_heading(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("## ")?;
    let (id, title) = rest.split_once(". ")?;
    if id.trim().is_empty() || title.trim().is_empty() {
        return None;
    }
    Some((id.trim().to_owned(), title.trim().to_owned()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PlanList {
    None,
    TargetFiles,
    AcceptanceCriteria,
    VerificationCommands,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PartialPlanStep {
    id: String,
    title: String,
    description: Vec<String>,
    depends_on: Vec<String>,
    pub(crate) target_files: Vec<RepoPath>,
    acceptance_criteria: Vec<String>,
    verification_commands: Vec<String>,
    list: PlanList,
}

impl PartialPlanStep {
    fn new(id: String, title: String) -> Self {
        Self {
            id,
            title,
            description: Vec::new(),
            depends_on: Vec::new(),
            target_files: Vec::new(),
            acceptance_criteria: Vec::new(),
            verification_commands: Vec::new(),
            list: PlanList::None,
        }
    }

    fn push_line(&mut self, line: &str) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return;
        }

        if let Some(depends_on) = trimmed.strip_prefix("Depends on:") {
            self.depends_on = parse_depends_on(depends_on);
            self.list = PlanList::None;
            return;
        }

        self.list = match trimmed {
            "Target files:" => PlanList::TargetFiles,
            "Acceptance criteria:" => PlanList::AcceptanceCriteria,
            "Verification commands:" => PlanList::VerificationCommands,
            _ => self.list.clone(),
        };
        if matches!(
            trimmed,
            "Target files:" | "Acceptance criteria:" | "Verification commands:"
        ) {
            return;
        }

        if let Some(item) = parse_markdown_list_item(trimmed) {
            match self.list {
                PlanList::TargetFiles => self.target_files.push(RepoPath::new(item)),
                PlanList::AcceptanceCriteria => self.acceptance_criteria.push(item),
                PlanList::VerificationCommands => self.verification_commands.push(item),
                PlanList::None => self.description.push(item),
            }
        } else if matches!(self.list, PlanList::None) {
            self.description.push(trimmed.to_owned());
        }
    }

    fn finish(self) -> PlanStep {
        PlanStep {
            id: self.id,
            title: self.title,
            description: self.description.join("\n"),
            depends_on: self.depends_on,
            target_files: self.target_files,
            acceptance_criteria: self.acceptance_criteria,
            verification_commands: self.verification_commands,
        }
    }
}

fn parse_depends_on(value: &str) -> Vec<String> {
    let value = value.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("none") {
        return Vec::new();
    }

    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn parse_markdown_list_item(line: &str) -> Option<String> {
    let item = line.strip_prefix("- ")?;
    Some(item.trim().trim_matches('`').to_owned())
}

pub(crate) fn implementation_task_text(step: &PlanStep) -> String {
    format!(
        "Implement only this Patchwright plan step.\n\nStep ID: {}\nTitle: {}\nDescription:\n{}\n\nDepends on:\n{}\n\nTarget files:\n{}\n\nAcceptance criteria:\n{}\n\nVerification commands:\n{}\n\nRules:\n- Implement only this step.\n- Do not implement unrelated plan steps.\n- Keep changes within the target files unless a supporting file is strictly required for compilation.",
        step.id,
        step.title,
        if step.description.trim().is_empty() {
            "None."
        } else {
            step.description.trim()
        },
        if step.depends_on.is_empty() {
            "none".to_owned()
        } else {
            step.depends_on.join(", ")
        },
        format_repo_path_lines(&step.target_files),
        format_string_lines(&step.acceptance_criteria),
        format_string_lines(&step.verification_commands)
    )
}

fn format_repo_path_lines(paths: &[RepoPath]) -> String {
    if paths.is_empty() {
        return "- none".to_owned();
    }

    paths
        .iter()
        .map(|path| format!("- {}", path.0))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_string_lines(values: &[String]) -> String {
    if values.is_empty() {
        return "- none".to_owned();
    }

    values
        .iter()
        .map(|value| format!("- {value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn out_of_scope_changed_files(report: &SolveReport, scope: &StepScope) -> Vec<RepoPath> {
    report
        .attempts
        .last()
        .map(|attempt| {
            attempt
                .verification
                .diff_summary
                .changed_files
                .iter()
                .filter(|path| !path_matches_scope(path, &scope.target_files))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

fn path_matches_scope(path: &RepoPath, target_files: &[RepoPath]) -> bool {
    target_files.iter().any(|target| {
        path.0 == target.0
            || target
                .0
                .strip_suffix('/')
                .is_some_and(|prefix| path.0.starts_with(&format!("{prefix}/")))
    })
}
