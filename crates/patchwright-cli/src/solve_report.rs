use patchwright_core::agent::SolveReport;

pub(crate) fn render_solve_report(report: &SolveReport) -> String {
    let mut output = String::new();

    output.push_str("Patchwright solve result\n\n");
    output.push_str(&format!("Status: {:?}\n\n", report.status));
    output.push_str("Changed files:\n");
    let changed_files = report
        .attempts
        .last()
        .map(|attempt| &attempt.verification.diff_summary.changed_files)
        .filter(|files| !files.is_empty());
    if let Some(files) = changed_files {
        for path in files {
            output.push_str(&format!("  {}\n", path.0));
        }
    } else {
        output.push_str("  none\n");
    }

    output.push_str("\nVerification:\n");
    let checks = report
        .attempts
        .last()
        .map(|attempt| &attempt.verification.checks)
        .filter(|checks| !checks.is_empty());
    if let Some(checks) = checks {
        for check in checks {
            let status = if check.passed { "ok" } else { "failed" };
            output.push_str(&format!("  [{status}] {}\n", check.name));
        }
    } else {
        output.push_str("  none\n");
    }

    output.push_str(&format!("\nAttempts: {}\n", report.attempts.len()));

    if !report.counterexamples.is_empty() {
        output.push_str("\nCounterexamples:\n");
        for counterexample in report.counterexamples.iter().take(3) {
            output.push_str(&format!(
                "  [{}] {}\n",
                counterexample.source,
                first_useful_line(&counterexample.detail)
            ));
        }
    }

    output.push_str("\nSummary:\n");
    output.push_str(&format!("  {}\n", first_useful_line(&report.summary)));

    output
}

fn first_useful_line(value: &str) -> String {
    value
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("none")
        .chars()
        .take(240)
        .collect()
}
