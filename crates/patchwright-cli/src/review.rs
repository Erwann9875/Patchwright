use crate::render_utils::push_plain_list;
use patchwright_core::types::RepoPath;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReviewReport {
    pub(crate) summary: String,
    pub(crate) risk_level: String,
    pub(crate) dimensions: Vec<ReviewDimension>,
    pub(crate) blocking_issues: Vec<ReviewFinding>,
    pub(crate) suggestions: Vec<ReviewFinding>,
    pub(crate) missing_tests: Vec<String>,
    pub(crate) recommended_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReviewDimension {
    name: String,
    status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReviewFinding {
    category: String,
    severity: String,
    location: Option<RepoPath>,
    message: String,
    recommendation: String,
}

pub(crate) fn build_review_report(diff: &str) -> ReviewReport {
    let changed_files = changed_files_from_diff(diff);
    let changed_tests = changed_files.iter().any(|path| is_test_path(&path.0));
    let changed_sources = changed_files
        .iter()
        .filter(|path| is_source_path(&path.0))
        .cloned()
        .collect::<Vec<_>>();
    let added_lines = added_lines_from_diff(diff);

    if changed_files.is_empty() {
        return ReviewReport {
            summary: "No diff to review.".to_owned(),
            risk_level: "low".to_owned(),
            dimensions: default_review_dimensions("ok", "No changes present."),
            blocking_issues: Vec::new(),
            suggestions: Vec::new(),
            missing_tests: Vec::new(),
            recommended_actions: vec![
                "Create or select a diff before requesting review.".to_owned()
            ],
        };
    }

    let mut blocking_issues = Vec::new();
    let mut suggestions = Vec::new();
    let mut missing_tests = Vec::new();

    if !changed_sources.is_empty() && !changed_tests {
        for path in &changed_sources {
            missing_tests.push(format!("{} changed without nearby test changes", path.0));
        }
        suggestions.push(ReviewFinding {
            category: "Test coverage".to_owned(),
            severity: "medium".to_owned(),
            location: changed_sources.first().cloned(),
            message: "Source files changed without test files in the diff.".to_owned(),
            recommendation: "Add or update focused tests for the changed behavior.".to_owned(),
        });
    }

    if let Some(secret_line) = added_lines.iter().find(|line| looks_like_secret(line)) {
        blocking_issues.push(ReviewFinding {
            category: "Security".to_owned(),
            severity: "high".to_owned(),
            location: None,
            message: format!(
                "Added line looks like a credential: {}",
                truncate(secret_line, 120)
            ),
            recommendation: "Remove hard-coded secrets and load credentials from configuration."
                .to_owned(),
        });
    }

    let risk_level = if blocking_issues.iter().any(|issue| issue.severity == "high") {
        "high"
    } else if !suggestions.is_empty() || changed_files.len() > 5 {
        "medium"
    } else {
        "low"
    };

    let mut dimensions = vec![
        review_dimension("Correctness", "needs review"),
        review_dimension("Architecture fit", "needs review"),
        review_dimension(
            "Security",
            if blocking_issues.is_empty() {
                "ok"
            } else {
                "risk"
            },
        ),
        review_dimension("Performance", "not assessed"),
        review_dimension("Maintainability", "needs review"),
        review_dimension(
            "Test coverage",
            if missing_tests.is_empty() {
                "ok"
            } else {
                "missing tests"
            },
        ),
        review_dimension("Breaking changes", "not assessed"),
        review_dimension("Overengineering", "not assessed"),
        review_dimension("Documentation", "not assessed"),
    ];
    dimensions.sort_by(|left, right| left.name.cmp(&right.name));

    let recommended_actions = if blocking_issues.is_empty() && suggestions.is_empty() {
        vec!["Run the project verifier before merging.".to_owned()]
    } else {
        let mut actions = Vec::new();
        if !blocking_issues.is_empty() {
            actions.push("Resolve blocking issues before implementation is accepted.".to_owned());
        }
        if !missing_tests.is_empty() {
            actions.push("Add regression tests for changed source behavior.".to_owned());
        }
        actions.push("Run patchwright verify --repo . after changes.".to_owned());
        actions
    };

    ReviewReport {
        summary: format!(
            "Reviewed {} changed file(s), with {} added line(s).",
            changed_files.len(),
            added_lines.len()
        ),
        risk_level: risk_level.to_owned(),
        dimensions,
        blocking_issues,
        suggestions,
        missing_tests,
        recommended_actions,
    }
}

pub(crate) fn render_review_report(report: &ReviewReport) -> String {
    let mut output = String::new();
    output.push_str("Patchwright review result\n\n");
    output.push_str(&format!("Risk: {}\n\n", title_case(&report.risk_level)));
    output.push_str(&format!("Summary:\n  {}\n\n", report.summary));

    output.push_str("Dimensions:\n");
    for dimension in &report.dimensions {
        output.push_str(&format!("  {}: {}\n", dimension.name, dimension.status));
    }
    output.push('\n');

    push_review_findings(&mut output, "Blocking issues", &report.blocking_issues);
    push_review_findings(&mut output, "Non-blocking suggestions", &report.suggestions);
    push_plain_list(&mut output, "Missing tests", &report.missing_tests);
    push_plain_list(
        &mut output,
        "Recommended next actions",
        &report.recommended_actions,
    );

    output
}

fn push_review_findings(output: &mut String, heading: &str, findings: &[ReviewFinding]) {
    output.push_str(&format!("{heading}:\n"));
    if findings.is_empty() {
        output.push_str("  none\n\n");
        return;
    }
    for finding in findings {
        let location = finding
            .location
            .as_ref()
            .map(|path| format!(" [{}]", path.0))
            .unwrap_or_default();
        output.push_str(&format!(
            "  [{}][{}]{} {}\n    {}\n",
            finding.category, finding.severity, location, finding.message, finding.recommendation
        ));
    }
    output.push('\n');
}

fn changed_files_from_diff(diff: &str) -> Vec<RepoPath> {
    let mut files = Vec::new();
    for line in diff.lines() {
        let Some(rest) = line.strip_prefix("diff --git ") else {
            continue;
        };
        let Some((_, right)) = rest.split_once(" b/") else {
            continue;
        };
        let path = right.trim();
        if !path.is_empty() {
            files.push(RepoPath::new(path));
        }
    }
    files
}

fn added_lines_from_diff(diff: &str) -> Vec<String> {
    diff.lines()
        .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
        .map(|line| line.trim_start_matches('+').to_owned())
        .collect()
}

fn default_review_dimensions(status: &str, note: &str) -> Vec<ReviewDimension> {
    [
        "Architecture fit",
        "Breaking changes",
        "Correctness",
        "Documentation",
        "Maintainability",
        "Overengineering",
        "Performance",
        "Security",
        "Test coverage",
    ]
    .into_iter()
    .map(|name| review_dimension(name, &format!("{status}: {note}")))
    .collect()
}

fn review_dimension(name: &str, status: &str) -> ReviewDimension {
    ReviewDimension {
        name: name.to_owned(),
        status: status.to_owned(),
    }
}

fn is_source_path(path: &str) -> bool {
    path.starts_with("src/")
        || path.starts_with("app/")
        || path.starts_with("lib/")
        || path.starts_with("crates/")
}

fn is_test_path(path: &str) -> bool {
    path.starts_with("tests/")
        || path.starts_with("__tests__/")
        || path.contains("/tests/")
        || path.contains("_test.")
        || path.contains(".test.")
}

fn looks_like_secret(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    (lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("secret")
        || lower.contains("password")
        || lower.contains("token"))
        && line.contains('=')
}

fn truncate(value: &str, max: usize) -> String {
    if value.len() <= max {
        value.to_owned()
    } else {
        format!("{}...", &value[..max])
    }
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
