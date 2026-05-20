use super::*;

// ---------------------------------------------------------------------------
// SYSTEM_PROMPT content verification
// ---------------------------------------------------------------------------

#[test]
fn test_system_prompt_mentions_agent_consumer() {
    // The prompt consumer is an LLM agent, not a human — must be stated explicitly
    assert!(
        SYSTEM_PROMPT.contains("agent") || SYSTEM_PROMPT.contains("LLM"),
        "SYSTEM_PROMPT must mention that the consumer is an LLM agent; got:\n{SYSTEM_PROMPT}"
    );
}

#[test]
fn test_system_prompt_penalises_empty_output() {
    // Returning nothing is the WORST outcome — prompt must warn against it
    let lower = SYSTEM_PROMPT.to_lowercase();
    assert!(
        lower.contains("empty") || lower.contains("nothing") || lower.contains("worst"),
        "SYSTEM_PROMPT must warn that an empty/no summary is the worst outcome; got:\n{SYSTEM_PROMPT}"
    );
}

#[test]
fn test_system_prompt_contains_toml_schema() {
    // The expected TOML output format must be exemplified in the prompt
    assert!(
        SYSTEM_PROMPT.contains("command_match"),
        "SYSTEM_PROMPT must show the TOML schema (command_match field); got:\n{SYSTEM_PROMPT}"
    );
}

#[test]
fn test_system_prompt_explains_tier_system() {
    // Prompt must explain oo's 4-tier classification so the LLM has context
    let lower = SYSTEM_PROMPT.to_lowercase();
    assert!(
        lower.contains("passthrough") || lower.contains("large") || lower.contains("tier"),
        "SYSTEM_PROMPT must describe oo's tier system; got:\n{SYSTEM_PROMPT}"
    );
}

#[test]
fn test_system_prompt_under_2000_chars() {
    // Sent with every LLM call — keep it compact
    assert!(
        SYSTEM_PROMPT.len() < 2000,
        "SYSTEM_PROMPT must be under 2000 characters; actual length: {}",
        SYSTEM_PROMPT.len()
    );
}

#[test]
fn test_system_prompt_contains_named_group_instruction() {
    // LLMs must be explicitly told to use named capture groups — numbered groups break oo
    assert!(
        SYSTEM_PROMPT.contains("(?P<name>") || SYSTEM_PROMPT.contains("named capture"),
        "SYSTEM_PROMPT must contain instruction about named capture groups; got:\n{SYSTEM_PROMPT}"
    );
}

#[test]
fn test_system_prompt_contains_examples() {
    // At least 2 TOML examples — a test runner and a build/lint tool
    let success_count = SYSTEM_PROMPT.matches("[success]").count();
    assert!(
        success_count >= 2,
        "SYSTEM_PROMPT must contain at least 2 TOML [success] sections (one per example); found {success_count}"
    );
}

#[test]
fn test_call_anthropic_success() {
    let mut server = mockito::Server::new();
    let mock = server
        .mock("POST", "/v1/messages")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(include_str!("../tests/fixtures/anthropic_success.json"))
        .create();

    let result = call_anthropic(
        &format!("{}/v1/messages", server.url()),
        "test-key",
        "test-model",
        "test prompt",
    );
    assert!(result.is_ok(), "expected Ok, got: {result:?}");
    assert!(
        result.unwrap().contains("command_match"),
        "response must contain pattern content"
    );
    mock.assert();
}

#[test]
fn test_user_message_without_hint() {
    // When hint is None, user message should not contain Hint: prefix
    let dir = tempfile::TempDir::new().expect("tempdir");
    let patterns_dir = dir.path().join("patterns");
    let learn_status_path = dir.path().join("learn-status.log");

    let config = LearnConfig {
        provider: "anthropic".into(),
        model: "claude-haiku-4-5".into(),
        api_key_env: "ANTHROPIC_API_KEY".into(),
    };

    let _params = LearnParams {
        config: &config,
        api_key: "test-key",
        base_url: "https://api.anthropic.com/v1/messages",
        patterns_dir: &patterns_dir,
        learn_status_path: &learn_status_path,
        hint: None,
    };

    // Check the user message format without hint
    let user_msg = format!(
        "Command: cargo test\nExit code: 0\nOutput:\n{}",
        truncate_for_prompt("test result: ok. 5 passed")
    );

    assert!(
        !user_msg.contains("Hint:"),
        "user message without hint should not contain 'Hint:'"
    );
    assert!(
        user_msg.contains("Command:"),
        "user message should contain 'Command:'"
    );
    assert!(
        user_msg.contains("Exit code:"),
        "user message should contain 'Exit code:'"
    );
    assert!(
        user_msg.contains("Output:"),
        "user message should contain 'Output:'"
    );
}

#[test]
fn test_user_message_with_hint() {
    // When hint is Some, user message should contain Hint: prefix with the hint text
    let dir = tempfile::TempDir::new().expect("tempdir");
    let patterns_dir = dir.path().join("patterns");
    let learn_status_path = dir.path().join("learn-status.log");

    let config = LearnConfig {
        provider: "anthropic".into(),
        model: "claude-haiku-4-5".into(),
        api_key_env: "ANTHROPIC_API_KEY".into(),
    };

    let _params = LearnParams {
        config: &config,
        api_key: "test-key",
        base_url: "https://api.anthropic.com/v1/messages",
        patterns_dir: &patterns_dir,
        learn_status_path: &learn_status_path,
        hint: Some("capture summary line only"),
    };

    // Check the user message format with hint
    let user_msg = format!(
        "Command: cargo test\nExit code: 0\nHint: capture summary line only\nOutput:\n{}",
        truncate_for_prompt("test result: ok. 5 passed")
    );

    assert!(
        user_msg.contains("Hint: capture summary line only"),
        "user message with hint should contain the hint text"
    );
    assert!(
        user_msg.contains("Command:"),
        "user message should contain 'Command:'"
    );
    assert!(
        user_msg.contains("Exit code:"),
        "user message should contain 'Exit code:'"
    );
    assert!(
        user_msg.contains("Output:"),
        "user message should contain 'Output:'"
    );
}

#[test]
fn test_call_anthropic_malformed_response() {
    // Server returns 200 but with a body that doesn't match the expected schema.
    let mut server = mockito::Server::new();
    let mock = server
        .mock("POST", "/v1/messages")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"id":"msg-test","type":"message","content":[]}"#)
        .create();

    let result = call_anthropic(
        &format!("{}/v1/messages", server.url()),
        "test-key",
        "test-model",
        "test prompt",
    );
    // Empty content array → missing text field → should return Err
    assert!(
        result.is_err(),
        "expected Err on malformed response, got Ok"
    );
    mock.assert();
}
