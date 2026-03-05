use super::*;
use mockito::{Matcher, Server};

// ---------------------------------------------------------------------------
// retry_loop tests — end-to-end with run_learn_with_config()
//
// Tests use run_learn_with_config() to avoid environment variable mutation,
// ensuring thread-safety — they can run in parallel without --test-threads=1.
// ---------------------------------------------------------------------------

#[test]
fn retry_succeeds_on_second_attempt() {
    let mut server = Server::new();
    let temp_dir = tempfile::TempDir::new().expect("tempdir");
    // label() generates "echo-test" from "echo test"
    let patterns_dir = temp_dir.path().join("patterns");
    let pattern_path = patterns_dir.join("echo-test.toml");
    let learn_status_path = temp_dir.path().join("learn-status.log");

    // First call returns invalid TOML
    let _mock1 = server
        .mock("POST", "/v1/messages")
        .expect(1)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(include_str!("../tests/fixtures/anthropic_invalid.json"))
        .create();

    // Second call returns valid TOML
    let _mock2 = server
        .mock("POST", "/v1/messages")
        .expect(1)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(include_str!("../tests/fixtures/anthropic_success.json"))
        .create();

    let config = LearnConfig {
        provider: "anthropic".into(),
        model: "claude-haiku-4-5".into(),
        api_key_env: "ANTHROPIC_API_KEY".into(),
    };
    let base_url = format!("{}/v1/messages", server.url());
    let params = LearnParams {
        config: &config,
        api_key: "test-key",
        base_url: &base_url,
        patterns_dir: &patterns_dir,
        learn_status_path: &learn_status_path,
    };

    let result = run_learn_with_config(&params, "echo test", "test output", 0);

    // Should succeed on second attempt
    assert!(
        result.is_ok(),
        "run_learn_with_config should succeed: {:?}",
        result
    );

    // Verify pattern file was written (before temp_dir goes out of scope)
    assert!(
        pattern_path.exists(),
        "pattern file should exist at {:?}",
        pattern_path
    );
    let content = std::fs::read_to_string(&pattern_path).unwrap();
    assert!(
        content.contains("command_match"),
        "pattern should contain command_match"
    );
}

#[test]
fn retry_succeeds_on_third_attempt() {
    let mut server = Server::new();
    let temp_dir = tempfile::TempDir::new().expect("tempdir");
    let patterns_dir = temp_dir.path().join("patterns");
    let learn_status_path = temp_dir.path().join("learn-status.log");

    // First two calls return invalid TOML
    let _mock1 = server
        .mock("POST", "/v1/messages")
        .expect(1)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(include_str!("../tests/fixtures/anthropic_invalid.json"))
        .create();

    let _mock2 = server
        .mock("POST", "/v1/messages")
        .expect(1)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(include_str!("../tests/fixtures/anthropic_invalid.json"))
        .create();

    // Third call returns valid TOML
    let _mock3 = server
        .mock("POST", "/v1/messages")
        .expect(1)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(include_str!("../tests/fixtures/anthropic_success.json"))
        .create();

    let config = LearnConfig {
        provider: "anthropic".into(),
        model: "claude-haiku-4-5".into(),
        api_key_env: "ANTHROPIC_API_KEY".into(),
    };
    let base_url = format!("{}/v1/messages", server.url());
    let params = LearnParams {
        config: &config,
        api_key: "test-key",
        base_url: &base_url,
        patterns_dir: &patterns_dir,
        learn_status_path: &learn_status_path,
    };

    let result = run_learn_with_config(&params, "echo test", "test output", 0);

    // Should succeed on third attempt
    assert!(
        result.is_ok(),
        "run_learn_with_config should succeed: {:?}",
        result
    );

    // Verify pattern file was written
    let pattern_path = patterns_dir.join("echo-test.toml");
    assert!(pattern_path.exists(), "pattern file should exist");
}

#[test]
fn all_retries_exhausted_returns_error() {
    let mut server = Server::new();
    let temp_dir = tempfile::TempDir::new().expect("tempdir");
    let patterns_dir = temp_dir.path().join("patterns");
    let learn_status_path = temp_dir.path().join("learn-status.log");

    // All 3 calls return invalid TOML
    let _mock1 = server
        .mock("POST", "/v1/messages")
        .expect(1)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(include_str!("../tests/fixtures/anthropic_invalid.json"))
        .create();

    let _mock2 = server
        .mock("POST", "/v1/messages")
        .expect(1)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(include_str!("../tests/fixtures/anthropic_invalid.json"))
        .create();

    let _mock3 = server
        .mock("POST", "/v1/messages")
        .expect(1)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(include_str!("../tests/fixtures/anthropic_invalid.json"))
        .create();

    let config = LearnConfig {
        provider: "anthropic".into(),
        model: "claude-haiku-4-5".into(),
        api_key_env: "ANTHROPIC_API_KEY".into(),
    };
    let base_url = format!("{}/v1/messages", server.url());
    let params = LearnParams {
        config: &config,
        api_key: "test-key",
        base_url: &base_url,
        patterns_dir: &patterns_dir,
        learn_status_path: &learn_status_path,
    };

    let result = run_learn_with_config(&params, "echo test", "test output", 0);

    // Should fail with error containing "failed after 3 attempts"
    assert!(
        result.is_err(),
        "run_learn_with_config should fail after exhausting retries"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("failed after 3 attempts"),
        "error should contain 'failed after 3 attempts': {err_msg}"
    );
    assert!(
        err_msg.contains("invalid TOML"),
        "error should contain the validation failure: {err_msg}"
    );
}

#[test]
fn temperature_zero_in_request() {
    let mut server = Server::new();
    let temp_dir = tempfile::TempDir::new().expect("tempdir");
    let patterns_dir = temp_dir.path().join("patterns");
    let learn_status_path = temp_dir.path().join("learn-status.log");

    let _mock = server
        .mock("POST", "/v1/messages")
        .match_body(Matcher::PartialJson(
            serde_json::json!({"temperature": 0.0}),
        ))
        .expect(1)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(include_str!("../tests/fixtures/anthropic_success.json"))
        .create();

    let config = LearnConfig {
        provider: "anthropic".into(),
        model: "claude-haiku-4-5".into(),
        api_key_env: "ANTHROPIC_API_KEY".into(),
    };
    let base_url = format!("{}/v1/messages", server.url());
    let params = LearnParams {
        config: &config,
        api_key: "test-key",
        base_url: &base_url,
        patterns_dir: &patterns_dir,
        learn_status_path: &learn_status_path,
    };

    let result = run_learn_with_config(&params, "echo test", "test output", 0);

    // Should succeed and temperature should match 0.0 (float)
    assert!(
        result.is_ok(),
        "run_learn_with_config should succeed: {:?}",
        result
    );
}
