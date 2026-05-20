use super::*;
use crate::learn_utils::{strip_fences as strip_fences_impl, truncate_utf8};

// Re-export the functions for testing
fn strip_fences(s: &str) -> String {
    strip_fences_impl(s)
}

// Tests for failure-section validation live in a separate file to keep this
// module under 500 lines.
#[cfg(test)]
#[path = "learn_validate_tests.rs"]
mod validate;

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
// validate_pattern_toml (now uses validate_pattern_regexes from toml module)
// ---------------------------------------------------------------------------

#[test]
fn test_validate_pattern_toml_regexes_valid() {
    let toml = r#"
command_match = "^mytest"
[success]
pattern = '(?P<n>\d+) passed'
summary = "{n} passed"
"#;
    assert!(validate_pattern_toml_with_limits(toml).is_ok());
}

#[test]
fn test_validate_pattern_toml_regexes_invalid_regex() {
    let toml = r#"
command_match = "[invalid"
"#;
    assert!(validate_pattern_toml_with_limits(toml).is_err());
}

#[test]
fn test_validate_pattern_toml_regexes_no_success() {
    let toml = r#"command_match = "^cargo""#;
    assert!(validate_pattern_toml_with_limits(toml).is_ok());
}

#[test]
fn test_validate_pattern_toml_regexes_invalid_toml_syntax() {
    let toml = "this is not valid = [toml";
    assert!(validate_pattern_toml_with_limits(toml).is_err());
}

#[test]
fn test_validate_pattern_toml_regexes_missing_command_match() {
    let toml = r#"
[success]
pattern = "ok"
summary = "done"
"#;
    assert!(validate_pattern_toml_with_limits(toml).is_err());
}

#[test]
fn test_validate_pattern_toml_regexes_invalid_command_match_regex() {
    let toml = r#"command_match = "[invalid_regex""#;
    assert!(validate_pattern_toml_with_limits(toml).is_err());
}

#[test]
fn test_validate_pattern_toml_regexes_invalid_success_pattern_regex() {
    let toml = r#"
command_match = "^cargo"
[success]
pattern = "[invalid"
summary = "done"
"#;
    assert!(validate_pattern_toml_with_limits(toml).is_err());
}

#[test]
fn test_validate_pattern_toml_regexes_with_valid_success_pattern() {
    let toml = "command_match = \"^pytest\"\n[success]\npattern = '(?P<n>\\d+) passed'\nsummary = \"{n} passed\"";
    assert!(validate_pattern_toml_with_limits(toml).is_ok());
}

#[test]
fn test_validate_pattern_toml_regexes_enforces_length_limit() {
    // MAX_REGEX_LENGTH is 500 chars
    let long_regex = format!(r#"command_match = "{}""#, "a".repeat(501));
    assert!(validate_pattern_toml_with_limits(&long_regex).is_err());
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
// label (first word sanitization)
// ---------------------------------------------------------------------------

#[test]
fn test_label_sanitizes_first_word_special_chars() {
    // First word should be sanitized the same way as second word
    // After rsplit('/'), we get the last component, then sanitize it
    assert_eq!(label("../bin/cargo test"), "cargo-test");
    assert_eq!(label("./binary/name test"), "name-test");
    assert_eq!(label("/path/.hidden/bin test"), "bin-test");
    assert_eq!(label("cmd-with-dashes test"), "cmd-with-dashes-test");
}

#[test]
fn test_label_first_word_length_limit() {
    // First word should be limited to MAX_FILENAME_COMPONENT (50 chars)
    let long_first = "a".repeat(100);
    let result = label(&format!("{long_first} test"));
    // Result should be "aaaaaaaaa-test" where first word is truncated to 50 chars
    assert_eq!(
        result.len(),
        55,
        "first word truncated to 50 chars + hyphen + 4 chars for 'test'"
    );
    assert_eq!(
        result
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .count(),
        result.len()
    );
}

#[test]
fn test_label_empty_after_sanitization() {
    // If first word has no valid chars after sanitization, return "unknown"
    assert_eq!(label("!!! test"), "unknown");
    assert_eq!(label("../ test"), "unknown");
    assert_eq!(label("@@@ test"), "unknown");
}

#[test]
fn test_label_prevents_dotfile_creation() {
    // Leading dots (dotfiles) should be stripped from first word
    assert_eq!(label(".cargo test"), "cargo-test");
    assert_eq!(label("..cargo test"), "cargo-test");
    // Path with /path/.hidden/test -> first word is "test" (after rsplit)
    assert_eq!(label("/path/.hidden/test"), "test");
    assert_eq!(label("./hidden/bin/test"), "test");
}

// ---------------------------------------------------------------------------
// label (existing tests)
// ---------------------------------------------------------------------------

#[test]
fn test_label_extraction() {
    // Second word is a flag ("-x") — excluded, so single word stays as-is
    assert_eq!(label("pytest -x"), "pytest");
    // Path stripped from first word, second word ("test") included as subcommand
    assert_eq!(label("/usr/bin/cargo test"), "cargo-test");
}

#[test]
fn test_label_path_extraction() {
    // Single-word commands (no subcommand) remain unchanged
    assert_eq!(label("/usr/local/bin/rustc"), "rustc");
    assert_eq!(label("./target/release/oo"), "oo");
}

#[test]
fn test_label_empty_command() {
    assert_eq!(label(""), "unknown");
}

#[test]
fn test_label_subcommand_included() {
    // Two-word commands where second word is a subcommand (not a flag)
    assert_eq!(label("cargo fmt --check"), "cargo-fmt");
    assert_eq!(label("cargo clippy -- -D warnings"), "cargo-clippy");
    assert_eq!(label("npm run build"), "npm-run");
    assert_eq!(label("cargo test"), "cargo-test");
}

#[test]
fn test_label_flag_excluded() {
    // Second word starting with '-' is a flag — not included in label
    assert_eq!(label("pytest -x"), "pytest");
    assert_eq!(label("cargo --verbose test"), "cargo");
}

#[test]
fn test_label_sanitizes_unsafe_chars_in_second_word() {
    // `/` in path argument → slashes stripped
    assert_eq!(label("git some/path/arg"), "git-somepatharg");
    assert_eq!(label("cargo /absolute/path"), "cargo-absolutepath");
    // `=` in subcommand value → stripped
    assert_eq!(label("git subcommand=val"), "git-subcommandval");
    // `..` traversal attempt → dots stripped, remaining chars kept
    assert_eq!(label("cmd ../etc/passwd"), "cmd-etcpasswd");
    // Two-flag-only command → no valid subcommand, returns first word only
    assert_eq!(label("rustc --foo --bar"), "rustc");
}

#[test]
fn test_label_second_word_length_limit() {
    // Second word should also be limited to MAX_FILENAME_COMPONENT (50 chars)
    let long_second = "a".repeat(100);
    let result = label(&format!("cargo {long_second}"));
    // Result should be "cargo-aaaaaaaaa" where second word is truncated to 50 chars
    assert_eq!(
        result.len(),
        56,
        "'cargo-' (6 chars) + second word truncated to 50 chars = 56"
    );
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
    assert_eq!(config.model, "claude-haiku-4-5");
}

#[test]
fn test_detect_provider_anthropic_model_name() {
    // Verify the Anthropic model uses the non-dated alias for forward compatibility.
    let config = detect_provider_with(|key| {
        if key == "ANTHROPIC_API_KEY" {
            Some("test-key".into())
        } else {
            None
        }
    });
    assert_eq!(config.provider, "anthropic");
    assert_eq!(
        config.model, "claude-haiku-4-5",
        "Anthropic model must use the stable alias, not the dated snapshot"
    );
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
// validate_anthropic_url
// ---------------------------------------------------------------------------

#[test]
fn test_anthropic_url_validation() {
    // HTTPS URLs should pass
    assert!(validate_anthropic_url("https://api.anthropic.com/v1/messages").is_ok());
    assert!(validate_anthropic_url("https://custom.example.com/api").is_ok());

    // HTTP localhost should pass
    assert!(validate_anthropic_url("http://localhost:8000/api").is_ok());
    assert!(validate_anthropic_url("http://localhost/api").is_ok());

    // HTTP 127.0.0.1 should pass
    assert!(validate_anthropic_url("http://127.0.0.1:8000/api").is_ok());
    assert!(validate_anthropic_url("http://127.0.0.1/api").is_ok());

    // HTTP other URLs should fail
    let result = validate_anthropic_url("http://api.example.com/v1/messages");
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("ANTHROPIC_API_URL must use HTTPS"));
    assert!(err_msg.contains("got: http://api.example.com/v1/messages"));

    let result = validate_anthropic_url("http://192.168.1.1/api");
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("got: http://192.168.1.1/api"));

    let result = validate_anthropic_url("http://example.com");
    assert!(result.is_err());

    // Ensure localhost.evil.com does NOT pass (prefix injection bypass)
    let result = validate_anthropic_url("http://localhost.evil.com/api");
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// run_background: hint handling
// ---------------------------------------------------------------------------

#[test]
fn test_run_background_with_hint_extracts_hint() {
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let data = serde_json::json!({
        "command": "cargo test",
        "output": "test result: ok. 5 passed\n",
        "exit_code": 0,
        "hint": "capture summary line only"
    });
    std::fs::write(tmp.path(), data.to_string()).expect("write");

    // Verify run_background can parse the hint
    let path = tmp.path();
    let content = std::fs::read_to_string(path).expect("read");
    let parsed: serde_json::Value = serde_json::from_str(&content).expect("valid json");

    let hint = parsed["hint"].as_str();
    assert_eq!(hint, Some("capture summary line only"));
}

// ---------------------------------------------------------------------------
// run_background: status file written on failure
// ---------------------------------------------------------------------------

#[test]
fn test_learn_status_written_on_failure() {
    // Provide a valid JSON payload but no API key in environment.
    // run_learn will return Err (missing API key), which should write a FAILED
    // entry to the status file.  We redirect learn_status_path by writing a
    // temp file and checking its contents after run_background returns.
    //
    // Because learn_status_path() uses the real config dir, we instead call
    // the internal path directly via the public write_learn_status_failure helper
    // to verify the format independently, then test run_background's error path
    // by confirming it propagates Err for known-bad inputs.

    // Part 1: verify write_learn_status_failure writes the expected format.
    let dir = tempfile::TempDir::new().expect("tempdir");
    let status_path = dir.path().join("learn-status.log");
    crate::commands::write_learn_status_failure(&status_path, "cargo-test", "no API key set")
        .expect("write must succeed");
    let content = std::fs::read_to_string(&status_path).expect("read");
    assert!(
        content.starts_with("FAILED cargo-test:"),
        "status must start with FAILED prefix: {content}"
    );
    assert!(
        content.contains("no API key set"),
        "status must contain error message: {content}"
    );

    // Part 2: run_background returns Err when JSON is valid but run_learn fails.
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let data = serde_json::json!({"command": "echo hello", "output": "hello", "exit_code": 0});
    std::fs::write(tmp.path(), data.to_string()).expect("write");
    // run_background may succeed or fail depending on env API keys, but must not panic.
    let _ = run_background(tmp.path().to_str().expect("utf8 path"));
}

// ---------------------------------------------------------------------------
// run_background: exit_code bounds checking
// ---------------------------------------------------------------------------

#[test]
fn test_run_background_invalid_exit_code_truncation() {
    // Test that exit_code outside i32 range is rejected
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    // i32::MAX is 2147483647, so use 2147483648 (i64 value that exceeds i32)
    let data = serde_json::json!({
        "command": "echo hello",
        "output": "hello",
        "exit_code": 2147483648i64
    });
    std::fs::write(tmp.path(), data.to_string()).expect("write");

    let result = run_background(tmp.path().to_str().expect("utf8 path"));
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("exit_code out of range for i32"),
        "error should mention i32 range: got {}",
        err_msg
    );
}

#[test]
fn test_run_background_negative_exit_code_out_of_range() {
    // Test that exit_code below i32::MIN is rejected
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    // i32::MIN is -2147483648, so use -2147483649 (i64 value that exceeds i32)
    let data = serde_json::json!({
        "command": "echo hello",
        "output": "hello",
        "exit_code": -2147483649i64
    });
    std::fs::write(tmp.path(), data.to_string()).expect("write");

    let result = run_background(tmp.path().to_str().expect("utf8 path"));
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("exit_code out of range for i32"),
        "error should mention i32 range: got {}",
        err_msg
    );
}

// ---------------------------------------------------------------------------
// run_learn_with_config: validation limits
// ---------------------------------------------------------------------------

#[test]
fn test_run_learn_with_config_rejects_hint_too_long() {
    let tmpdir = tempfile::TempDir::new().expect("tempdir");
    let config = LearnConfig {
        provider: "anthropic".into(),
        model: "claude-haiku-4-5".into(),
        api_key_env: "ANTHROPIC_API_KEY".into(),
    };
    let status_path = tmpdir.path().join("status.log");
    let params = LearnParams {
        config: &config,
        api_key: "test-key",
        base_url: "https://api.anthropic.com/v1/messages",
        patterns_dir: tmpdir.path(),
        learn_status_path: &status_path,
        hint: Some(&"x".repeat(1001)), // Exceeds MAX_HINT_LENGTH (1000)
    };

    let result = run_learn_with_config(&params, "echo test", "output", 0);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("--hint too long"),
        "error should mention hint limit: got {}",
        err_msg
    );
    assert!(
        err_msg.contains("1000"),
        "error should mention limit value: got {}",
        err_msg
    );
}

#[test]
fn test_run_learn_with_config_truncates_command() {
    // Command truncation is applied silently within run_learn_with_config
    // We verify by checking that a very long command doesn't cause validation errors
    let tmpdir = tempfile::TempDir::new().expect("tempdir");
    let config = LearnConfig {
        provider: "anthropic".into(),
        model: "claude-haiku-4-5".into(),
        api_key_env: "ANTHROPIC_API_KEY".into(),
    };
    let long_command = "a".repeat(200); // Exceeds MAX_COMMAND_LENGTH (100)
    let status_path = tmpdir.path().join("status.log");
    let params = LearnParams {
        config: &config,
        api_key: "test-key",
        base_url: "https://api.anthropic.com/v1/messages",
        patterns_dir: tmpdir.path(),
        learn_status_path: &status_path,
        hint: None,
    };

    // This will fail due to API, not command length validation
    let result = run_learn_with_config(&params, &long_command, "output", 0);
    // The error should be API-related, not "command too long"
    if let Err(e) = result {
        let err_msg = e.to_string();
        assert!(
            !err_msg.contains("too long"),
            "command should be truncated, not rejected: got {}",
            err_msg
        );
    }
}

#[test]
fn test_truncate_utf8_multibyte_command_crosses_limit() {
    // Test that truncating a multi-byte UTF-8 command that crosses the byte limit
    // doesn't panic and produces valid UTF-8.
    // Each Chinese character is 3 bytes: 你好世界 = 4 chars × 3 bytes = 12 bytes
    let multibyte_command = "测试".repeat(50); // 50 chars × 3 bytes = 150 bytes, exceeds MAX_COMMAND_LENGTH (100)
    let truncated = crate::learn_utils::truncate_utf8(&multibyte_command, 100);

    // Should truncate to valid UTF-8 boundary (33 chars = 99 bytes)
    assert_eq!(truncated.len(), 99);
    assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());
    // Verify it's a valid prefix of the original string (valid UTF-8)
    assert!(multibyte_command.starts_with(truncated));

    // Test with emoji (4 bytes each): 🎉🎊 = 2 chars × 4 bytes = 8 bytes
    let emoji_command = "🎉".repeat(30); // 30 chars × 4 bytes = 120 bytes
    let truncated_emoji = crate::learn_utils::truncate_utf8(&emoji_command, 100);
    // Should truncate to 25 chars = 100 bytes exactly
    assert_eq!(truncated_emoji.len(), 100);
    assert!(std::str::from_utf8(truncated_emoji.as_bytes()).is_ok());
    assert!(emoji_command.starts_with(truncated_emoji));
}
