use patchwright_core::action::{Action, Observation};
use patchwright_core::traits::ModelProvider;
use patchwright_core::types::{
    ContextPack, Counterexample, FileQuery, FileSlice, LineRange, ModelRequest, Patch, RepoPath,
    ScoredPath, SearchQuery, TaskSpec,
};
use patchwright_model_openai::{
    build_prompt, parse_action_json, OpenAiCompatibleClient, OpenAiConfig,
};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::thread;

const TEST_API_KEY_ENV: &str = "PATCHWRIGHT_OPENAI_TEST_API_KEY";
const TEST_CHILD_ADDR_ENV: &str = "PATCHWRIGHT_OPENAI_TEST_CHILD_ADDR";

fn config(base_url: String) -> OpenAiConfig {
    OpenAiConfig {
        base_url,
        model: "test-model".to_string(),
        api_key_env: TEST_API_KEY_ENV.to_string(),
        timeout_seconds: 5,
    }
}

fn request(task: &str) -> ModelRequest {
    ModelRequest {
        task: TaskSpec::from_text(PathBuf::from("."), task),
        observations: Vec::new(),
        counterexamples: Vec::new(),
        context: None,
    }
}

#[test]
fn build_prompt_includes_action_contract_and_state() {
    let request = ModelRequest {
        task: TaskSpec::from_text(PathBuf::from("."), "Fix the add test"),
        observations: vec![Observation::FileRead(FileSlice {
            path: RepoPath::new("src/lib.rs"),
            start_line: 1,
            content: "pub fn add(a: i32, b: i32) -> i32 { a - b }\n".to_string(),
        })],
        counterexamples: vec![Counterexample {
            source: "cargo".to_string(),
            detail: "assertion failed: left == right".to_string(),
        }],
        context: None,
    };

    let body = build_prompt(&request, "test-model");

    assert_eq!(body["model"], "test-model");
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][1]["role"], "user");

    let system = body["messages"][0]["content"]
        .as_str()
        .expect("system content");
    assert!(system.contains("Return only one JSON action"));
    assert!(system.contains("Verification decides success"));
    assert!(system.contains("read_file"));
    assert!(system.contains("apply_patch"));
    assert!(system.contains("Core rules"));

    let user = body["messages"][1]["content"]
        .as_str()
        .expect("user content");
    assert!(user.contains("Task:"));
    assert!(user.contains("Fix the add test"));
    assert!(user.contains("src/lib.rs"));
    assert!(user.contains("a - b"));
    assert!(user.contains("assertion failed"));
}

#[test]
fn build_prompt_apply_patch_example_uses_valid_hunk_header() {
    let body = build_prompt(&request("Fix the add test"), "test-model");
    let system = body["messages"][0]["content"]
        .as_str()
        .expect("system content");

    assert!(system.contains("@@ -1,3 +1,3 @@"));
    assert!(!system.contains("\\n@@\\n"));
}

#[test]
fn build_prompt_renders_context_pack() {
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

    let body = build_prompt(&request, "test-model");
    let user = body["messages"][1]["content"]
        .as_str()
        .expect("user content");

    assert!(user.contains("Context:"));
    assert!(user.contains("Cargo.toml"));
    assert!(user.contains("src/lib.rs"));
    assert!(user.contains("tests/parser_test.rs"));
}

#[test]
fn build_prompt_caps_large_user_state() {
    let request = ModelRequest {
        task: TaskSpec::from_text(PathBuf::from("."), "Fix the add test"),
        observations: vec![Observation::FileRead(FileSlice {
            path: RepoPath::new("src/lib.rs"),
            start_line: 1,
            content: "pub fn add(a: i32, b: i32) -> i32 { a - b }\n".repeat(2_000),
        })],
        counterexamples: vec![Counterexample {
            source: "cargo".to_string(),
            detail: "assertion failed: left == right".repeat(2_000),
        }],
        context: None,
    };

    let body = build_prompt(&request, "test-model");
    let user = body["messages"][1]["content"]
        .as_str()
        .expect("user content");

    assert!(user.len() <= 24 * 1024);
    assert!(user.contains("Task:"));
    assert!(user.contains("Fix the add test"));
    assert!(user.contains("Observations:"));
    assert!(user.contains("Counterexamples:"));
}

#[test]
fn build_prompt_preserves_context_before_large_state_sections() {
    let request = ModelRequest {
        task: TaskSpec::from_text(PathBuf::from("."), "Fix the parser"),
        observations: vec![Observation::FileRead(FileSlice {
            path: RepoPath::new("logs/full.txt"),
            start_line: 1,
            content: "observed failure\n".repeat(5_000),
        })],
        counterexamples: vec![Counterexample {
            source: "cargo test".to_string(),
            detail: "counterexample detail\n".repeat(5_000),
        }],
        context: Some(ContextPack {
            files: vec![
                ScoredPath {
                    path: RepoPath::new("Cargo.toml"),
                    score: 90,
                },
                ScoredPath {
                    path: RepoPath::new("src/lib.rs"),
                    score: 80,
                },
            ],
            likely_tests: Vec::new(),
            manifests: vec![RepoPath::new("Cargo.toml")],
            recent_observations: Vec::new(),
            counterexamples: Vec::new(),
        }),
    };

    let body = build_prompt(&request, "test-model");
    let user = body["messages"][1]["content"]
        .as_str()
        .expect("user content");

    assert!(user.len() <= 24 * 1024);
    assert!(user.contains("Context:"));
    assert!(user.contains("Cargo.toml"));
    assert!(user.contains("src/lib.rs"));
    assert!(
        user.find("Context:").expect("context section")
            < user.find("Observations:").expect("observations section")
    );
}

#[test]
fn dry_run_client_returns_finish_action_without_network() {
    let mut client = OpenAiCompatibleClient::dry_run(config("http://127.0.0.1:9".to_string()));

    let response = client
        .propose_action(request("repair the parser"))
        .expect("dry-run should not require network or an API key");

    let Action::Finish { summary } = response.action else {
        panic!("expected finish action");
    };
    assert!(summary.contains("test-model"));
    assert!(summary.contains("repair the parser"));
}

#[test]
fn parses_finish_action_json() {
    let action = parse_action_json(r#"{"action":"finish","summary":"all done"}"#)
        .expect("finish action should parse");

    assert_eq!(
        action,
        Action::Finish {
            summary: "all done".to_string()
        }
    );
}

#[test]
fn parses_read_file_action_with_range() {
    let action =
        parse_action_json(r#"{"action":"read_file","path":"src/lib.rs","start":1,"end":120}"#)
            .expect("read_file action should parse");

    assert_eq!(
        action,
        Action::ReadFile {
            path: RepoPath::new("src/lib.rs"),
            range: Some(LineRange { start: 1, end: 120 }),
        }
    );
}

#[test]
fn parses_read_file_action_without_range() {
    let action = parse_action_json(r#"{"action":"read_file","path":"README.md"}"#)
        .expect("read_file action without range should parse");

    assert_eq!(
        action,
        Action::ReadFile {
            path: RepoPath::new("README.md"),
            range: None,
        }
    );
}

#[test]
fn parses_search_text_action() {
    let action =
        parse_action_json(r#"{"action":"search_text","pattern":"parse_user","root":"src"}"#)
            .expect("search_text action should parse");

    assert_eq!(
        action,
        Action::SearchText(SearchQuery {
            pattern: "parse_user".to_string(),
            root: Some(RepoPath::new("src")),
        })
    );
}

#[test]
fn parses_list_files_action() {
    let action = parse_action_json(r#"{"action":"list_files","root":"src"}"#)
        .expect("list_files action should parse");

    assert_eq!(
        action,
        Action::ListFiles(FileQuery {
            root: Some(RepoPath::new("src")),
        })
    );
}

#[test]
fn parses_apply_patch_action() {
    let action = parse_action_json(
        r#"{"action":"apply_patch","unified_diff":"diff --git a/src/lib.rs b/src/lib.rs\n..."}"#,
    )
    .expect("apply_patch action should parse");

    assert_eq!(
        action,
        Action::ApplyPatch(Patch {
            unified_diff: "diff --git a/src/lib.rs b/src/lib.rs\n...".to_string(),
        })
    );
}

#[test]
fn parses_run_verifier_action_and_related_run_actions() {
    let cases = [
        (r#"{"action":"run_verifier"}"#, Action::RunVerifier),
        (r#"{"action":"run_tests"}"#, Action::RunTests),
        (r#"{"action":"run_typecheck"}"#, Action::RunTypecheck),
        (r#"{"action":"run_benchmark"}"#, Action::RunBenchmark),
    ];

    for (json, expected) in cases {
        let action = parse_action_json(json).expect("run action should parse");
        assert_eq!(action, expected);
    }
}

#[test]
fn rejects_invalid_action_json() {
    let error = parse_action_json("not json").expect_err("invalid JSON should fail");

    assert!(error.to_string().contains("not valid JSON"));
}

#[test]
fn rejects_unsupported_action_json() {
    let error =
        parse_action_json(r#"{"action":"unknown"}"#).expect_err("unsupported action should fail");

    assert!(error.to_string().contains("unsupported model action"));
}

#[test]
fn rejects_unknown_action() {
    let error = parse_action_json(r#"{"action":"revert_attempt","snapshot_id":"snap"}"#)
        .expect_err("unknown model action should fail");

    assert!(error.to_string().contains("unsupported model action"));
}

#[test]
fn rejects_absolute_or_parent_paths_if_applicable() {
    let cases = [
        r#"{"action":"read_file","path":""}"#,
        r#"{"action":"read_file","path":"/src/lib.rs"}"#,
        r#"{"action":"read_file","path":"../src/lib.rs"}"#,
        r#"{"action":"read_file","path":"src/../lib.rs"}"#,
        r#"{"action":"read_file","path":"."}"#,
        r#"{"action":"read_file","path":"C:\\src\\lib.rs"}"#,
        r#"{"action":"search_text","pattern":"parse_user","root":".."}"#,
        r#"{"action":"list_files","root":"/src"}"#,
    ];

    for json in cases {
        let error = parse_action_json(json).expect_err("unsafe repo path should fail");
        assert!(
            error.to_string().contains("relative"),
            "expected relative-path error for {json}, got {error}"
        );
    }
}

#[test]
fn rejects_colon_containing_model_paths() {
    let cases = [
        r#"{"action":"read_file","path":"README.md:Zone.Identifier"}"#,
        r#"{"action":"search_text","pattern":"secret","root":".env:backup"}"#,
        r#"{"action":"list_files","root":"src:backup"}"#,
    ];

    for json in cases {
        let error = parse_action_json(json).expect_err("colon-containing repo path should fail");
        assert!(
            error.to_string().contains("relative"),
            "expected relative-path error for {json}, got {error}"
        );
    }
}

#[test]
fn rejects_invalid_read_file_ranges() {
    let cases = [
        r#"{"action":"read_file","path":"src/lib.rs","start":1}"#,
        r#"{"action":"read_file","path":"src/lib.rs","end":120}"#,
        r#"{"action":"read_file","path":"src/lib.rs","start":0,"end":120}"#,
        r#"{"action":"read_file","path":"src/lib.rs","start":121,"end":120}"#,
    ];

    for json in cases {
        parse_action_json(json).expect_err("invalid read_file range should fail");
    }
}

#[test]
fn live_mode_uses_openai_compatible_chat_completion_shape() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let address = listener.local_addr().expect("local addr");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept request");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));

        let mut request_line = String::new();
        reader.read_line(&mut request_line).expect("request line");
        assert_eq!(request_line.trim_end(), "POST /chat/completions HTTP/1.1");

        let mut authorization = None;
        let mut content_length = None;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("header line");
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }

            if let Some(value) = line.strip_prefix("Authorization: ") {
                authorization = Some(value.to_string());
            }
            if let Some(value) = line.strip_prefix("Content-Length: ") {
                content_length = Some(value.parse::<usize>().expect("content length"));
            }
        }

        assert_eq!(authorization.as_deref(), Some("Bearer test-key"));
        let mut body = vec![0; content_length.expect("content-length header")];
        reader.read_exact(&mut body).expect("request body");
        let body: serde_json::Value = serde_json::from_slice(&body).expect("json body");
        assert_eq!(body["model"], "test-model");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["role"], "user");
        assert!(body["messages"][1]["content"]
            .as_str()
            .expect("user message")
            .contains("repair the verifier"));

        let response_body = serde_json::json!({
            "choices": [{
                "message": {
                    "content": "{\"action\":\"finish\",\"summary\":\"patched\"}"
                }
            }]
        })
        .to_string();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            response_body.len(),
            response_body
        )
        .expect("write response");
    });

    let output = Command::new(std::env::current_exe().expect("current test binary"))
        .arg("--exact")
        .arg("live_mode_child_uses_test_api_key")
        .arg("--nocapture")
        .env(TEST_API_KEY_ENV, "test-key")
        .env(TEST_CHILD_ADDR_ENV, format!("http://{address}"))
        .output()
        .expect("run child test process");

    if !output.status.success() {
        let _ = TcpStream::connect(address);
        let _ = server.join();
    } else {
        server.join().expect("server thread should finish");
    }

    assert!(
        output.status.success(),
        "child test failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn live_mode_child_uses_test_api_key() {
    let Ok(base_url) = std::env::var(TEST_CHILD_ADDR_ENV) else {
        return;
    };

    let mut client = OpenAiCompatibleClient::new(config(base_url));
    let response = client
        .propose_action(request("repair the verifier"))
        .expect("http response should parse");

    assert_eq!(
        response.action,
        Action::Finish {
            summary: "patched".to_string()
        }
    );
}
