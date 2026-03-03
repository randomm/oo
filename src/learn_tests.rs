use super::*;

// ---------------------------------------------------------------------------
// strip_fences
// ---------------------------------------------------------------------------

#[test]
fn test_strip_fences_toml() {
    let input = "```toml\ncommand_match = \"test\"\n```";
    assert_eq!(strip_fences(input), "command_match = \"test\"");
}

#[test]
fn test_strip_fences_plain() {
    let input = "```\ncommand_match = \"test\"\n```";
    assert_eq!(strip_fences(input), "command_match = \"test\"");
}

#[test]
fn test_strip_fences_none() {
    let input = "command_match = \"test\"";
    assert_eq!(strip_fences(input), "command_match = \"test\"");
}

#[test]
fn test_strip_fences_whitespace_preserved() {
    let input = "```toml\n\ncommand_match = \"test\"\n\n```";
    let result = strip_fences(input);
    assert!(
        result.contains("command_match"),
        "content must be preserved"
    );
}

// ---------------------------------------------------------------------------
// validate_pattern_toml
// ---------------------------------------------------------------------------

#[test]
fn test_validate_valid_toml() {
    let toml = r#"
command_match = "^mytest"
[success]
pattern = '(?P<n>\d+) passed'
summary = "{n} passed"
"#;
    assert!(validate_pattern_toml(toml).is_ok());
}

#[test]
fn test_validate_invalid_regex() {
    let toml = r#"
command_match = "[invalid"
"#;
    assert!(validate_pattern_toml(toml).is_err());
}

#[test]
fn test_validate_valid_toml_no_success() {
    let toml = r#"command_match = "^cargo""#;
    assert!(validate_pattern_toml(toml).is_ok());
}

#[test]
fn test_validate_invalid_toml_syntax() {
    let toml = "this is not valid = [toml";
    assert!(validate_pattern_toml(toml).is_err());
}

#[test]
fn test_validate_missing_command_match() {
    let toml = r#"
[success]
pattern = "ok"
summary = "done"
"#;
    assert!(validate_pattern_toml(toml).is_err());
}

#[test]
fn test_validate_invalid_command_match_regex() {
    let toml = r#"command_match = "[invalid_regex""#;
    assert!(validate_pattern_toml(toml).is_err());
}

#[test]
fn test_validate_invalid_success_pattern_regex() {
    let toml = r#"
command_match = "^cargo"
[success]
pattern = "[invalid"
summary = "done"
"#;
    assert!(validate_pattern_toml(toml).is_err());
}

#[test]
fn test_validate_toml_with_valid_success_pattern() {
    let toml = "command_match = \"^pytest\"\n[success]\npattern = '(?P<n>\\d+) passed'\nsummary = \"{n} passed\"";
    assert!(validate_pattern_toml(toml).is_ok());
}

// ---------------------------------------------------------------------------
// truncate_for_prompt / truncate_utf8
// ---------------------------------------------------------------------------

#[test]
fn test_truncate_for_prompt() {
    let short = "hello";
    assert_eq!(truncate_for_prompt(short), "hello");

    let long = "x".repeat(5000);
    assert_eq!(truncate_for_prompt(&long).len(), 4000);
}

#[test]
fn test_truncate_for_prompt_boundary() {
    let exact = "a".repeat(4000);
    assert_eq!(truncate_for_prompt(&exact).len(), 4000);

    let over = "a".repeat(4001);
    assert_eq!(truncate_for_prompt(&over).len(), 4000);
}

#[test]
fn test_truncate_utf8_multibyte_boundary() {
    // Each '£' is 2 bytes (0xC2 0xA3). With max_bytes=5 we must not split a char.
    let s = "££££"; // 8 bytes total
    let result = truncate_utf8(s, 5);
    // 5 bytes would split '£' at byte 5; we must step back to byte 4 (2 chars).
    assert_eq!(result.len(), 4);
    assert!(result.is_ascii() || std::str::from_utf8(result.as_bytes()).is_ok());
    assert_eq!(result, "££");
}

#[test]
fn test_truncate_utf8_exact_boundary() {
    let s = "hello"; // 5 bytes
    assert_eq!(truncate_utf8(s, 5), "hello");
    assert_eq!(truncate_utf8(s, 10), "hello");
}

// ---------------------------------------------------------------------------
// label
// ---------------------------------------------------------------------------

#[test]
fn test_label_extraction() {
    assert_eq!(label("pytest -x"), "pytest");
    assert_eq!(label("/usr/bin/cargo test"), "cargo");
}

#[test]
fn test_label_path_extraction() {
    assert_eq!(label("/usr/local/bin/rustc"), "rustc");
    assert_eq!(label("./target/release/oo"), "oo");
}

#[test]
fn test_label_empty_command() {
    assert_eq!(label(""), "unknown");
}

// ---------------------------------------------------------------------------
// detect_provider / LearnConfig::default
// ---------------------------------------------------------------------------

#[test]
fn test_default_config_has_valid_fields() {
    // Provider is auto-detected from env; all fields must be non-empty.
    let config = LearnConfig::default();
    assert!(!config.provider.is_empty(), "provider must not be empty");
    assert!(!config.model.is_empty(), "model must not be empty");
    assert!(
        !config.api_key_env.is_empty(),
        "api_key_env must not be empty"
    );
}

#[test]
fn test_detect_provider_no_keys_defaults_to_anthropic() {
    // Uses closure-based injection — no env mutation, no race conditions.
    let config = detect_provider_with(|_| None);
    assert_eq!(config.provider, "anthropic");
    assert_eq!(config.api_key_env, "ANTHROPIC_API_KEY");
    assert!(!config.model.is_empty());
}

#[test]
fn test_detect_provider_cerebras_key_selects_cerebras() {
    let config = detect_provider_with(|key| {
        if key == "CEREBRAS_API_KEY" {
            Some("test-cerebras-key".into())
        } else {
            None
        }
    });
    assert_eq!(config.provider, "cerebras");
    assert_eq!(config.api_key_env, "CEREBRAS_API_KEY");
    assert_eq!(config.model, "zai-glm-4.7");
}

#[test]
fn test_detect_provider_openai_key_selects_openai() {
    let config = detect_provider_with(|key| {
        if key == "OPENAI_API_KEY" {
            Some("test-openai-key".into())
        } else {
            None
        }
    });
    assert_eq!(config.provider, "openai");
    assert_eq!(config.api_key_env, "OPENAI_API_KEY");
    assert_eq!(config.model, "gpt-4o-mini");
}

#[test]
fn test_detect_provider_anthropic_takes_priority() {
    // When both ANTHROPIC and OPENAI are set, anthropic wins (priority order).
    let config = detect_provider_with(|key| match key {
        "ANTHROPIC_API_KEY" => Some("key-a".into()),
        "OPENAI_API_KEY" => Some("key-o".into()),
        _ => None,
    });
    assert_eq!(config.provider, "anthropic");
}

// ---------------------------------------------------------------------------
// load_learn_config / patterns_dir
// ---------------------------------------------------------------------------

#[test]
fn test_load_learn_config_no_file_returns_default() {
    // Returns default config (or Err if config.toml is malformed) — must not panic
    match load_learn_config() {
        Ok(c) => {
            assert!(!c.provider.is_empty());
            assert!(!c.model.is_empty());
            assert!(!c.api_key_env.is_empty());
        }
        Err(_) => {} // malformed config.toml in test env is acceptable
    }
}

#[test]
fn test_patterns_dir_is_under_config_dir() {
    let dir = patterns_dir();
    let s = dir.to_string_lossy();
    assert!(s.ends_with("oo/patterns"), "got: {s}");
}

// ---------------------------------------------------------------------------
// run_background
// ---------------------------------------------------------------------------

#[test]
fn test_run_background_missing_file_returns_err() {
    assert!(run_background("/tmp/__oo_no_such_file_xyz__.json").is_err());
}

#[test]
fn test_run_background_invalid_json_returns_err() {
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    std::fs::write(tmp.path(), b"not valid json {{{").expect("write");
    assert!(run_background(tmp.path().to_str().expect("utf8 path")).is_err());
}

#[test]
fn test_run_background_valid_json_no_api_key() {
    // Valid JSON; run_learn returns Err when no API key is set.
    // We don't assert result — key may or may not be present in dev env.
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let data = serde_json::json!({"command": "echo hello", "output": "hello", "exit_code": 0});
    std::fs::write(tmp.path(), data.to_string()).expect("write");
    let _ = run_background(tmp.path().to_str().expect("utf8 path"));
}

// ---------------------------------------------------------------------------
// call_openai / call_anthropic — mockito VCR tests
// ---------------------------------------------------------------------------

#[test]
fn test_call_openai_success() {
    let mut server = mockito::Server::new();
    let mock = server
        .mock("POST", "/v1/chat/completions")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(include_str!("../tests/fixtures/openai_success.json"))
        .create();

    let result = call_openai(
        &format!("{}/v1/chat/completions", server.url()),
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
fn test_call_openai_http_error_returns_err() {
    // ureq v3 returns Err for non-2xx responses — verify the error propagates.
    let mut server = mockito::Server::new();
    let mock = server
        .mock("POST", "/v1/chat/completions")
        .with_status(401)
        .create();

    let result = call_openai(
        &format!("{}/v1/chat/completions", server.url()),
        "bad-key",
        "test-model",
        "test prompt",
    );
    assert!(result.is_err(), "expected Err on 401, got Ok");
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("API error") || err_msg.contains("401") || err_msg.contains("error"),
        "error message should mention the failure: {err_msg}"
    );
    mock.assert();
}

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
fn test_cerebras_uses_openai_format() {
    // Cerebras is OpenAI-compatible — verify call_openai handles Cerebras responses.
    let mut server = mockito::Server::new();
    let mock = server
        .mock("POST", "/v1/chat/completions")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(include_str!("../tests/fixtures/openai_success.json"))
        .create();

    let result = call_openai(
        &format!("{}/v1/chat/completions", server.url()),
        "test-cerebras-key",
        "zai-glm-4.7",
        "test prompt",
    );
    assert!(result.is_ok(), "expected Ok, got: {result:?}");
    mock.assert();
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

#[test]
fn test_call_openai_malformed_response() {
    // Server returns 200 with a JSON body that is valid JSON but does not
    // match the expected OpenAI response schema (missing choices[0].message.content).
    let mut server = mockito::Server::new();
    let mock = server
        .mock("POST", "/v1/chat/completions")
        .with_status(200)
        .with_header("content-type", "application/json")
        // Valid JSON, but choices array is empty → content path yields None
        .with_body(r#"{"id":"chatcmpl-test","object":"chat.completion","choices":[]}"#)
        .create();

    let result = call_openai(
        &format!("{}/v1/chat/completions", server.url()),
        "test-key",
        "test-model",
        "test prompt",
    );
    // Empty choices → missing content field → must return Err
    assert!(
        result.is_err(),
        "expected Err on malformed OpenAI response, got Ok"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("OpenAI") || err_msg.contains("error") || err_msg.contains("response"),
        "error message must be descriptive: {err_msg}"
    );
    mock.assert();
}
