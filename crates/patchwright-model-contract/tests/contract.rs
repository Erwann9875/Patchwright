use patchwright_core::action::{Action, Observation};
use patchwright_core::types::{
    ContextPack, Counterexample, FileQuery, FileSlice, LineRange, ModelRequest, Patch, RepoPath,
    ScoredPath, SearchQuery, TaskSpec,
};
use patchwright_model_contract::{
    action_output_schema, architecture_design_schema, build_openai_prompt, build_prompt,
    implementation_graph_schema, parse_action_json, parse_architecture_design_json,
    parse_implementation_graph_json, render_exec_prompt, render_implementation_graph_markdown,
};
use std::path::PathBuf;

fn request(task: &str) -> ModelRequest {
    ModelRequest {
        task: TaskSpec::from_text(PathBuf::from("."), task),
        observations: Vec::new(),
        counterexamples: Vec::new(),
        context: None,
    }
}

#[test]
fn prompt_includes_action_contract_and_state() {
    let request = ModelRequest {
        task: TaskSpec::from_text(PathBuf::from("."), "Fix the add test"),
        observations: vec![Observation::FileRead(FileSlice {
            path: RepoPath::new("src/lib.rs"),
            start_line: 1,
            content: "pub fn add(a: i32, b: i32) -> i32 { a - b }\n".to_owned(),
        })],
        counterexamples: vec![Counterexample {
            source: "cargo".to_owned(),
            detail: "assertion failed: left == right".to_owned(),
        }],
        context: None,
    };

    let prompt = build_prompt(&request);

    assert!(prompt.system.contains("Return only one JSON action"));
    assert!(prompt.system.contains("Verification decides success"));
    assert!(prompt.system.contains("read_file"));
    assert!(prompt.system.contains("apply_patch"));
    assert!(prompt.user.contains("Task:"));
    assert!(prompt.user.contains("Fix the add test"));
    assert!(prompt.user.contains("src/lib.rs"));
    assert!(prompt.user.contains("a - b"));
    assert!(prompt.user.contains("assertion failed"));
}

#[test]
fn openai_prompt_uses_shared_contract_messages() {
    let body = build_openai_prompt(&request("Fix the add test"), "test-model");

    assert_eq!(body["model"], "test-model");
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][1]["role"], "user");
    assert!(body["messages"][0]["content"]
        .as_str()
        .expect("system content")
        .contains("Return only one JSON action"));
}

#[test]
fn exec_prompt_warns_transport_not_to_edit_files() {
    let prompt = render_exec_prompt(&request("inspect code"));

    assert!(prompt.contains("Do not edit files"));
    assert!(prompt.contains("Return one final JSON object"));
    assert!(prompt.contains("inspect code"));
}

#[test]
fn prompt_renders_context_pack() {
    let request = ModelRequest {
        task: TaskSpec::from_text(PathBuf::from("."), "Fix the parser"),
        observations: Vec::new(),
        counterexamples: Vec::new(),
        context: Some(ContextPack {
            files: vec![
                ScoredPath {
                    path: RepoPath::new("src/lib.rs"),
                    score: 80,
                },
                ScoredPath {
                    path: RepoPath::new("Cargo.toml"),
                    score: 70,
                },
            ],
            likely_tests: vec![RepoPath::new("tests/parser_test.rs")],
            manifests: vec![RepoPath::new("Cargo.toml")],
            recent_observations: Vec::new(),
            counterexamples: Vec::new(),
        }),
    };

    let prompt = build_prompt(&request);

    assert!(prompt.user.contains("Context:"));
    assert!(prompt.user.contains("Cargo.toml"));
    assert!(prompt.user.contains("src/lib.rs"));
    assert!(prompt.user.contains("tests/parser_test.rs"));
}

#[test]
fn prompt_caps_large_user_state() {
    let request = ModelRequest {
        task: TaskSpec::from_text(PathBuf::from("."), "Fix the add test"),
        observations: vec![Observation::FileRead(FileSlice {
            path: RepoPath::new("src/lib.rs"),
            start_line: 1,
            content: "pub fn add(a: i32, b: i32) -> i32 { a - b }\n".repeat(2_000),
        })],
        counterexamples: vec![Counterexample {
            source: "cargo".to_owned(),
            detail: "assertion failed: left == right".repeat(2_000),
        }],
        context: None,
    };

    let prompt = build_prompt(&request);

    assert!(prompt.user.len() <= 24 * 1024);
    assert!(prompt.user.contains("Task:"));
    assert!(prompt.user.contains("Observations:"));
    assert!(prompt.user.contains("Counterexamples:"));
}

#[test]
fn schema_contains_supported_actions_without_revert() {
    let schema = action_output_schema().to_string();

    assert_eq!(action_output_schema()["type"], "object");
    assert!(action_output_schema().get("oneOf").is_none());
    for action in [
        "read_file",
        "search_text",
        "list_files",
        "apply_patch",
        "run_verifier",
        "run_tests",
        "run_typecheck",
        "run_benchmark",
        "finish",
    ] {
        assert!(schema.contains(action));
    }
    assert!(!schema.contains("revert_attempt"));
}

#[test]
fn architecture_design_schema_requires_professional_design_sections() {
    let schema = architecture_design_schema();

    assert_eq!(schema["type"], "object");
    assert_eq!(schema["additionalProperties"], false);

    let required = schema["required"]
        .as_array()
        .expect("required fields should be an array")
        .iter()
        .map(|value| value.as_str().expect("required field should be string"))
        .collect::<Vec<_>>();

    for field in [
        "title",
        "goal",
        "current_architecture",
        "assumptions",
        "non_goals",
        "options",
        "recommendation",
        "file_impact",
        "implementation_plan",
        "test_strategy",
        "risks",
        "open_questions",
        "acceptance_criteria",
    ] {
        assert!(required.contains(&field), "missing required field {field}");
    }

    assert!(schema.to_string().contains("evidence"));
    assert!(schema.to_string().contains("path"));
    assert!(schema.to_string().contains("start_line"));
    assert!(schema.to_string().contains("verification_commands"));
}

#[test]
fn parses_architecture_design_json() {
    let design = parse_architecture_design_json(
        r#"{
          "title": "Feature Design: Team Billing",
          "goal": "Add organization billing.",
          "current_architecture": [
            {
              "summary": "Sessions are owned by auth.",
              "evidence": [
                {
                  "path": "src/auth/session.rs",
                  "start_line": 10,
                  "end_line": 30,
                  "reason": "session creation"
                }
              ]
            }
          ],
          "assumptions": ["Stripe remains the billing provider."],
          "non_goals": ["Do not rewrite auth."],
          "options": [
            {
              "name": "Organizations",
              "summary": "Add organizations and memberships.",
              "pros": ["Future role support."],
              "cons": ["Migration required."],
              "evidence": []
            }
          ],
          "recommendation": {
            "option_name": "Organizations",
            "rationale": "Keeps billing separate from user identity.",
            "evidence": []
          },
          "file_impact": [
            {
              "path": "src/auth/session.rs",
              "change_summary": "Load membership context.",
              "risk": null,
              "evidence": []
            }
          ],
          "implementation_plan": [
            {
              "id": "step-1",
              "title": "Add organization model",
              "description": "Create organization storage.",
              "depends_on": [],
              "target_files": ["src/org.rs"],
              "acceptance_criteria": ["Model compiles."],
              "verification_commands": ["cargo test"]
            }
          ],
          "test_strategy": {
            "unit": ["role parsing"],
            "integration": ["invoice ownership"],
            "end_to_end": [],
            "manual": [],
            "commands": ["cargo test"]
          },
          "migration_plan": null,
          "rollback_plan": "Drop unreleased tables.",
          "risks": [
            {
              "title": "Wrong invoice owner",
              "impact": "Billing data may attach to users.",
              "mitigation": "Add invoice ownership tests.",
              "evidence": []
            }
          ],
          "open_questions": ["Are guests billable?"],
          "acceptance_criteria": ["Admins can view invoices."]
        }"#,
    )
    .expect("design JSON should parse");

    assert_eq!(design.title, "Feature Design: Team Billing");
    assert_eq!(
        design.current_architecture[0].evidence[0].path.0,
        "src/auth/session.rs"
    );
    assert_eq!(
        design.implementation_plan[0].target_files[0].0,
        "src/org.rs"
    );
    assert_eq!(
        design.rollback_plan.as_deref(),
        Some("Drop unreleased tables.")
    );
}

#[test]
fn parses_and_renders_implementation_graph_json() {
    let graph = parse_implementation_graph_json(
        r#"{
          "steps": [
            {
              "id": "step-1",
              "title": "Add domain type",
              "description": "Introduce the smallest domain model first.",
              "depends_on": [],
              "target_files": ["src/domain.rs"],
              "acceptance_criteria": ["Domain type compiles."],
              "verification_commands": ["cargo test domain"]
            },
            {
              "id": "step-2",
              "title": "Wire API",
              "description": "Use the domain type in the API boundary.",
              "depends_on": ["step-1"],
              "target_files": ["src/api.rs"],
              "acceptance_criteria": ["API tests pass."],
              "verification_commands": ["cargo test api"]
            }
          ]
        }"#,
    )
    .expect("implementation graph JSON should parse");

    assert_eq!(graph.steps[1].depends_on, vec!["step-1"]);
    assert_eq!(graph.steps[1].target_files[0], RepoPath::new("src/api.rs"));

    let markdown = render_implementation_graph_markdown(&graph);
    assert!(markdown.contains("# Implementation Graph"));
    assert!(markdown.contains("## step-1. Add domain type"));
    assert!(markdown.contains("Depends on: none"));
    assert!(markdown.contains("Target files:\n- `src/api.rs`"));
}

#[test]
fn implementation_graph_schema_requires_steps() {
    let schema = implementation_graph_schema();

    assert_eq!(schema["type"], "object");
    assert!(schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "steps"));
}

#[test]
fn parses_supported_action_json() {
    let cases = [
        (
            r#"{"action":"read_file","path":"src/lib.rs","start":1,"end":120}"#,
            Action::ReadFile {
                path: RepoPath::new("src/lib.rs"),
                range: Some(LineRange { start: 1, end: 120 }),
            },
        ),
        (
            r#"{"action":"read_file","path":"src/lib.rs","start":null,"end":null,"pattern":null,"root":null,"unified_diff":null,"summary":null}"#,
            Action::ReadFile {
                path: RepoPath::new("src/lib.rs"),
                range: None,
            },
        ),
        (
            r#"{"action":"search_text","pattern":"parse_user","root":"src"}"#,
            Action::SearchText(SearchQuery {
                pattern: "parse_user".to_owned(),
                root: Some(RepoPath::new("src")),
            }),
        ),
        (
            r#"{"action":"list_files","root":"src"}"#,
            Action::ListFiles(FileQuery {
                root: Some(RepoPath::new("src")),
            }),
        ),
        (
            r#"{"action":"apply_patch","unified_diff":"diff --git a/src/lib.rs b/src/lib.rs\n..."}"#,
            Action::ApplyPatch(Patch {
                unified_diff: "diff --git a/src/lib.rs b/src/lib.rs\n...".to_owned(),
            }),
        ),
        (
            r#"{"action":"finish","path":null,"start":null,"end":null,"pattern":null,"root":null,"unified_diff":null,"summary":"all done"}"#,
            Action::Finish {
                summary: "all done".to_owned(),
            },
        ),
        (r#"{"action":"run_verifier"}"#, Action::RunVerifier),
        (
            r#"{"action":"finish","summary":"all done"}"#,
            Action::Finish {
                summary: "all done".to_owned(),
            },
        ),
    ];

    for (json, expected) in cases {
        let action = parse_action_json(json).expect("action should parse");
        assert_eq!(action, expected);
    }
}

#[test]
fn rejects_unsafe_or_unknown_action_json() {
    let cases = [
        "not json",
        r#"{"action":"unknown"}"#,
        r#"{"action":"revert_attempt","snapshot_id":"snap"}"#,
        r#"{"action":"read_file","path":"/src/lib.rs"}"#,
        r#"{"action":"read_file","path":"../src/lib.rs"}"#,
        r#"{"action":"read_file","path":"C:\\src\\lib.rs"}"#,
        r#"{"action":"search_text","pattern":"secret","root":".env:backup"}"#,
        r#"{"action":"read_file","path":"src/lib.rs","start":0,"end":120}"#,
    ];

    for json in cases {
        parse_action_json(json).expect_err("invalid action should fail");
    }
}
