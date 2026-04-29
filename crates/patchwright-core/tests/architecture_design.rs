use patchwright_core::types::{
    ArchitectureDesign, ArchitectureFinding, DesignOption, EvidenceRef, FileImpact, PlanStep,
    RecommendedDesign, RepoPath, Risk, TaskMode, TestStrategy,
};

#[test]
fn architecture_design_artifacts_attach_file_evidence() {
    let evidence = EvidenceRef {
        path: RepoPath::new("src/auth/session.rs"),
        start_line: Some(12),
        end_line: Some(48),
        reason: "session creation boundary".to_owned(),
    };

    let design = ArchitectureDesign {
        title: "Feature Design: Team Billing".to_owned(),
        goal: "Add organizations, memberships, and invoice ownership.".to_owned(),
        current_architecture: vec![ArchitectureFinding {
            summary: "Authentication is owned by the session module.".to_owned(),
            evidence: vec![evidence.clone()],
        }],
        assumptions: vec!["Stripe is the billing provider.".to_owned()],
        non_goals: vec!["Do not rewrite authentication.".to_owned()],
        options: vec![DesignOption {
            name: "Organizations and memberships".to_owned(),
            summary: "Add an organization aggregate separate from users.".to_owned(),
            pros: vec!["Preserves user identity.".to_owned()],
            cons: vec!["Requires a migration.".to_owned()],
            evidence: vec![evidence.clone()],
        }],
        recommendation: RecommendedDesign {
            option_name: "Organizations and memberships".to_owned(),
            rationale: "Supports future roles without mixing billing with auth.".to_owned(),
            evidence: vec![evidence.clone()],
        },
        file_impact: vec![FileImpact {
            path: RepoPath::new("src/auth/session.rs"),
            change_summary: "Read organization memberships when creating sessions.".to_owned(),
            risk: Some("Session payload compatibility".to_owned()),
            evidence: vec![evidence.clone()],
        }],
        implementation_plan: vec![PlanStep {
            id: "step-1".to_owned(),
            title: "Add organization tables".to_owned(),
            description: "Create organizations and memberships.".to_owned(),
            depends_on: Vec::new(),
            target_files: vec![RepoPath::new("migrations/001_orgs.sql")],
            acceptance_criteria: vec!["Migration applies cleanly.".to_owned()],
            verification_commands: vec!["cargo test".to_owned()],
        }],
        test_strategy: TestStrategy {
            unit: vec!["Membership role parsing.".to_owned()],
            integration: vec!["Invoice ownership API.".to_owned()],
            end_to_end: Vec::new(),
            manual: Vec::new(),
            commands: vec!["cargo test".to_owned()],
        },
        migration_plan: Some("Backfill personal organizations for existing users.".to_owned()),
        rollback_plan: Some("Drop organization tables before launch.".to_owned()),
        risks: vec![Risk {
            title: "Billing ownership mismatch".to_owned(),
            impact: "Invoices could attach to users instead of organizations.".to_owned(),
            mitigation: "Add integration coverage around invoice creation.".to_owned(),
            evidence: vec![evidence.clone()],
        }],
        open_questions: vec!["Are guest members billable?".to_owned()],
        acceptance_criteria: vec!["Team admins can view invoices.".to_owned()],
    };

    assert_eq!(TaskMode::Design, TaskMode::Design);
    assert_eq!(
        design.current_architecture[0].evidence[0].path,
        RepoPath::new("src/auth/session.rs")
    );
    assert_eq!(design.implementation_plan[0].id, "step-1");
    assert_eq!(design.risks[0].evidence[0].start_line, Some(12));
}
