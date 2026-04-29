use patchwright_core::action::Action;
use patchwright_core::traits::ModelProvider;
use patchwright_core::types::{ModelRequest, TaskSpec};
use patchwright_model_openai::{parse_action_json, OpenAiCompatibleClient, OpenAiConfig};
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
    }
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
fn rejects_invalid_action_json() {
    let error = parse_action_json("not json").expect_err("invalid JSON should fail");

    assert!(error.to_string().contains("not valid JSON"));
}

#[test]
fn rejects_unsupported_action_json() {
    let error =
        parse_action_json(r#"{"action":"run_tests"}"#).expect_err("unsupported action should fail");

    assert!(error.to_string().contains("unsupported model action"));
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
