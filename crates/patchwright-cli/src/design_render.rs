use patchwright_core::types::{
    ArchitectureDesign, DesignOption, EvidenceRef, FileImpact, PlanStep, Risk,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
pub(crate) fn write_design_document(
    repo: &Path,
    task: &str,
    design: &ArchitectureDesign,
) -> Result<PathBuf, String> {
    let plans_dir = repo.join("docs").join("patchwright").join("plans");
    fs::create_dir_all(&plans_dir).map_err(|error| error.to_string())?;
    let path = plans_dir.join(format!(
        "{}-{}.md",
        current_date_string(),
        slugify_task(task)
    ));
    fs::write(&path, render_architecture_design_markdown(design))
        .map_err(|error| error.to_string())?;
    Ok(path)
}

pub(crate) fn render_architecture_design_markdown(design: &ArchitectureDesign) -> String {
    let mut output = String::new();

    output.push_str(&format!("# {}\n\n", design.title));
    push_section(&mut output, "Goal", &design.goal);
    push_findings(
        &mut output,
        "Current Architecture",
        &design.current_architecture,
    );
    push_inspected_files(&mut output, design);
    push_list(&mut output, "Assumptions", &design.assumptions);
    push_list(&mut output, "Non-goals", &design.non_goals);
    push_options(&mut output, &design.options);
    push_recommendation(&mut output, design);
    push_file_impact(&mut output, &design.file_impact);
    push_plan_steps(
        &mut output,
        "Implementation Plan",
        &design.implementation_plan,
    );
    push_test_strategy(&mut output, design);
    push_optional_section(
        &mut output,
        "Migration Plan",
        design.migration_plan.as_deref(),
    );
    push_optional_section(
        &mut output,
        "Rollback Plan",
        design.rollback_plan.as_deref(),
    );
    push_risks(&mut output, &design.risks);
    push_list(&mut output, "Open Questions", &design.open_questions);
    push_list(
        &mut output,
        "Acceptance Criteria",
        &design.acceptance_criteria,
    );

    output
}

fn push_section(output: &mut String, heading: &str, content: &str) {
    output.push_str(&format!("## {heading}\n\n{}\n\n", blank_if_empty(content)));
}

fn push_optional_section(output: &mut String, heading: &str, content: Option<&str>) {
    push_section(output, heading, content.unwrap_or("None."));
}

fn push_list(output: &mut String, heading: &str, items: &[String]) {
    output.push_str(&format!("## {heading}\n\n"));
    if items.is_empty() {
        output.push_str("- None.\n\n");
        return;
    }
    for item in items {
        output.push_str(&format!("- {}\n", blank_if_empty(item)));
    }
    output.push('\n');
}

fn push_findings(
    output: &mut String,
    heading: &str,
    findings: &[patchwright_core::types::ArchitectureFinding],
) {
    output.push_str(&format!("## {heading}\n\n"));
    if findings.is_empty() {
        output.push_str("- None.\n\n");
        return;
    }
    for finding in findings {
        output.push_str(&format!("- {}\n", blank_if_empty(&finding.summary)));
        push_evidence(output, &finding.evidence);
    }
    output.push('\n');
}

fn push_inspected_files(output: &mut String, design: &ArchitectureDesign) {
    output.push_str("## Relevant Files Inspected\n\n");
    let mut files = Vec::new();
    collect_evidence_paths(&mut files, &design.recommendation.evidence);
    for finding in &design.current_architecture {
        collect_evidence_paths(&mut files, &finding.evidence);
    }
    for option in &design.options {
        collect_evidence_paths(&mut files, &option.evidence);
    }
    for impact in &design.file_impact {
        if !files.contains(&impact.path.0) {
            files.push(impact.path.0.clone());
        }
        collect_evidence_paths(&mut files, &impact.evidence);
    }
    for risk in &design.risks {
        collect_evidence_paths(&mut files, &risk.evidence);
    }

    if files.is_empty() {
        output.push_str("- None.\n\n");
        return;
    }
    for file in files {
        output.push_str(&format!("- `{file}`\n"));
    }
    output.push('\n');
}

fn collect_evidence_paths(files: &mut Vec<String>, evidence: &[EvidenceRef]) {
    for item in evidence {
        if !files.contains(&item.path.0) {
            files.push(item.path.0.clone());
        }
    }
}

fn push_options(output: &mut String, options: &[DesignOption]) {
    output.push_str("## Design Options\n\n");
    if options.is_empty() {
        output.push_str("- None.\n\n");
        return;
    }
    for option in options {
        output.push_str(&format!("### {}\n\n{}\n\n", option.name, option.summary));
        push_list(output, "Pros", &option.pros);
        push_list(output, "Cons", &option.cons);
        push_evidence(output, &option.evidence);
        output.push('\n');
    }
}

fn push_recommendation(output: &mut String, design: &ArchitectureDesign) {
    output.push_str("## Recommended Design\n\n");
    output.push_str(&format!(
        "**{}**\n\n{}\n",
        design.recommendation.option_name, design.recommendation.rationale
    ));
    push_evidence(output, &design.recommendation.evidence);
    output.push('\n');
}

fn push_file_impact(output: &mut String, impacts: &[FileImpact]) {
    output.push_str("## File Impact\n\n");
    if impacts.is_empty() {
        output.push_str("- None.\n\n");
        return;
    }
    for impact in impacts {
        output.push_str(&format!("- `{}`: {}", impact.path.0, impact.change_summary));
        if let Some(risk) = &impact.risk {
            output.push_str(&format!(" Risk: {risk}."));
        }
        output.push('\n');
        push_evidence(output, &impact.evidence);
    }
    output.push('\n');
}

fn push_plan_steps(output: &mut String, heading: &str, steps: &[PlanStep]) {
    output.push_str(&format!("## {heading}\n\n"));
    if steps.is_empty() {
        output.push_str("- None.\n\n");
        return;
    }
    for step in steps {
        output.push_str(&format!(
            "### {}. {}\n\n{}\n\n",
            step.id, step.title, step.description
        ));
        push_list(output, "Depends On", &step.depends_on);
        let target_files = step
            .target_files
            .iter()
            .map(|path| path.0.clone())
            .collect::<Vec<_>>();
        push_list(output, "Target Files", &target_files);
        push_list(
            output,
            "Step Acceptance Criteria",
            &step.acceptance_criteria,
        );
        push_list(output, "Verification Commands", &step.verification_commands);
    }
}

fn push_test_strategy(output: &mut String, design: &ArchitectureDesign) {
    output.push_str("## Test Strategy\n\n");
    push_list(output, "Unit", &design.test_strategy.unit);
    push_list(output, "Integration", &design.test_strategy.integration);
    push_list(output, "End To End", &design.test_strategy.end_to_end);
    push_list(output, "Manual", &design.test_strategy.manual);
    push_list(output, "Commands", &design.test_strategy.commands);
}

fn push_risks(output: &mut String, risks: &[Risk]) {
    output.push_str("## Risks\n\n");
    if risks.is_empty() {
        output.push_str("- None.\n\n");
        return;
    }
    for risk in risks {
        output.push_str(&format!(
            "- **{}**: {} Mitigation: {}\n",
            risk.title, risk.impact, risk.mitigation
        ));
        push_evidence(output, &risk.evidence);
    }
    output.push('\n');
}

fn push_evidence(output: &mut String, evidence: &[EvidenceRef]) {
    if evidence.is_empty() {
        return;
    }
    output.push_str("  Evidence:\n");
    for item in evidence {
        let line_suffix = match (item.start_line, item.end_line) {
            (Some(start), Some(end)) => format!(":{start}-{end}"),
            (Some(start), None) => format!(":{start}"),
            _ => String::new(),
        };
        output.push_str(&format!(
            "  - `{}`{}: {}\n",
            item.path.0, line_suffix, item.reason
        ));
    }
}

fn blank_if_empty(value: &str) -> &str {
    if value.trim().is_empty() {
        "None."
    } else {
        value.trim()
    }
}

fn current_date_string() -> String {
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| (duration.as_secs() / 86_400) as i64)
        .unwrap_or_default();
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i64, i64, i64) {
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let month_prime = (5 * doy + 2) / 153;
    let day = doy - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };

    (year, month, day)
}

fn slugify_task(task: &str) -> String {
    let mut slug = String::new();
    let mut previous_was_dash = false;
    for character in task.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            previous_was_dash = false;
        } else if !previous_was_dash && !slug.is_empty() {
            slug.push('-');
            previous_was_dash = true;
        }
        if slug.len() >= 60 {
            break;
        }
    }

    let slug = slug.trim_matches('-').to_owned();
    if slug.is_empty() {
        "design".to_owned()
    } else {
        slug
    }
}
