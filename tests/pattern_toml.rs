// Tests for TOML pattern parsing

use double_o::pattern::{extract_summary, parse_pattern_str};

#[test]
fn test_success_strategy_toml_tail() {
    let toml = r#"
command_match = "^myapp"

[success]
strategy = "tail"
lines = 5
"#;
    let pat = parse_pattern_str(toml).unwrap();
    let output = (0..20).map(|i| format!("line {i}\n")).collect::<String>();
    let result = extract_summary(pat.success.as_ref().unwrap(), &output).unwrap();
    assert!(result.contains("line 15"));
    assert!(result.contains("line 19"));
}

#[test]
fn test_success_strategy_toml_head() {
    let toml = r#"
command_match = "^myapp"

[success]
strategy = "head"
lines = 3
"#;
    let pat = parse_pattern_str(toml).unwrap();
    let output = (0..10).map(|i| format!("line {i}\n")).collect::<String>();
    let result = extract_summary(pat.success.as_ref().unwrap(), &output).unwrap();
    assert_eq!(result, "line 0\nline 1\nline 2");
}

#[test]
fn test_success_strategy_toml_grep() {
    let toml = r#"
command_match = "^myapp"

[success]
strategy = "grep"
grep = "ERROR"
"#;
    let pat = parse_pattern_str(toml).unwrap();
    let output = "INFO ok\nERROR bad\nINFO fine\n";
    let result = extract_summary(pat.success.as_ref().unwrap(), &output).unwrap();
    assert_eq!(result, "ERROR bad");
}

#[test]
fn test_success_strategy_toml_regex_backward_compat() {
    // Verify old pattern+summary format still works
    let toml = r#"
command_match = "^myapp"

[success]
pattern = '(?P<count>\d+) passed'
summary = "{count} passed"
"#;
    let pat = parse_pattern_str(toml).unwrap();
    let result = extract_summary(pat.success.as_ref().unwrap(), "42 passed").unwrap();
    assert_eq!(result, "42 passed");
}

#[test]
fn test_success_strategy_toml_defaults() {
    let toml = r#"
command_match = "^myapp"

[success]
strategy = "tail"
"#;
    let pat = parse_pattern_str(toml).unwrap();
    // tail strategy should default to 30 lines (same as failure)
    assert!(matches!(
        pat.success.unwrap().strategy,
        double_o::pattern::SuccessStrategy::Tail { lines: 30 }
    ));
}

#[test]
fn test_invalid_toml_returns_error() {
    let result = parse_pattern_str("not valid toml {{{");
    assert!(result.is_err());
}

#[test]
fn test_invalid_regex_returns_error() {
    let toml = r#"
command_match = "[invalid"
"#;
    let result = parse_pattern_str(toml);
    assert!(result.is_err());
}

#[test]
fn test_load_pattern_from_toml() {
    let toml = r#"
command_match = "^myapp test"

[success]
pattern = '(?P<count>\d+) tests passed'
summary = "{count} tests passed"

[failure]
strategy = "tail"
lines = 20
"#;
    let pat = parse_pattern_str(toml).unwrap();
    assert!(pat.command_match.is_match("myapp test --verbose"));
    let summary = extract_summary(pat.success.as_ref().unwrap(), "42 tests passed").unwrap();
    assert_eq!(summary, "42 tests passed");
}

#[test]
fn test_user_patterns_override_builtins() {
    let user_pat = parse_pattern_str(
        r#"
command_match = "^pytest"
[success]
pattern = '(?P<n>\d+) ok'
summary = "{n} ok"
"#,
    )
    .unwrap();

    // User patterns should be checked first
    let mut all = vec![user_pat];
    all.extend(double_o::pattern::builtin_patterns());

    let pat = double_o::pattern::find_matching("pytest -x", &all).unwrap();
    let summary = extract_summary(pat.success.as_ref().unwrap(), "10 ok").unwrap();
    assert_eq!(summary, "10 ok");
}

// ---------------------------------------------------------------------------
// Regex validation tests
// ---------------------------------------------------------------------------

#[test]
fn test_command_match_too_long_rejected() {
    // Create a regex pattern that exceeds MAX_REGEX_LENGTH (500 chars)
    let long_pattern = "a".repeat(501); // 501 chars
    assert!(long_pattern.len() > 500);

    let toml = format!(
        r#"
command_match = "{}"
"#,
        long_pattern
    );

    let result = parse_pattern_str(&toml);
    assert!(result.is_err());
    let err_msg = match result {
        Err(e) => e.to_string(),
        Ok(_) => unreachable!(),
    };
    assert!(err_msg.contains("too long"));
}

#[test]
fn test_command_match_max_length_accepted() {
    // Create a regex pattern that is exactly at the limit
    let pattern = "[a-z]+".repeat(10); // 50 chars, well under limit
    assert!(pattern.len() < 500);

    let toml = format!(
        r#"
command_match = "{}"
"#,
        pattern
    );

    let result = parse_pattern_str(&toml);
    assert!(result.is_ok());
}

#[test]
fn test_success_regex_too_long_rejected() {
    let long_pattern = "(?P<test>".to_string() + &"a".repeat(500) + ")"; // > 500 chars
    assert!(long_pattern.len() > 500);

    let toml = format!(
        r#"
command_match = "^test"

[success]
pattern = '{}'
summary = "{{test}}"
"#,
        long_pattern
    );

    let result = parse_pattern_str(&toml);
    assert!(result.is_err());
    let err_msg = match result {
        Err(e) => e.to_string(),
        Ok(_) => unreachable!(),
    };
    assert!(err_msg.contains("too long"));
}

#[test]
fn test_success_grep_too_long_rejected() {
    let long_pattern = "a".repeat(501); // 501 chars
    assert!(long_pattern.len() > 500);

    let toml = format!(
        r#"
command_match = "^test"

[success]
strategy = "grep"
grep = "{}"
"#,
        long_pattern
    );

    let result = parse_pattern_str(&toml);
    assert!(result.is_err());
    let err_msg = match result {
        Err(e) => e.to_string(),
        Ok(_) => unreachable!(),
    };
    assert!(err_msg.contains("too long"));
}

#[test]
fn test_failure_grep_too_long_rejected() {
    let long_pattern = "a".repeat(501); // 501 chars
    assert!(long_pattern.len() > 500);

    let toml = format!(
        r#"
command_match = "^test"

[failure]
strategy = "grep"
grep = "{}"
"#,
        long_pattern
    );

    let result = parse_pattern_str(&toml);
    assert!(result.is_err());
    let err_msg = match result {
        Err(e) => e.to_string(),
        Ok(_) => unreachable!(),
    };
    assert!(err_msg.contains("too long"));
}

#[test]
fn test_invalid_regex_proper_error_message() {
    let toml = r#"
command_match = "[unclosed("  
"#;

    let result = parse_pattern_str(toml);
    assert!(result.is_err());
    let err_msg = match result {
        Err(e) => e.to_string(),
        Ok(_) => unreachable!(),
    };
    // Should mention it's a regex compilation error
    assert!(err_msg.contains("regex") || err_msg.contains("compilation"));
}

#[test]
fn test_all_user_regexes_apply_limits() {
    // Complex pattern with all regex fields
    let long_pattern = "[a-z]+".repeat(101); // 505 chars, too long

    let toml = format!(
        r#"
command_match = "^test"

[success]
strategy = "grep"
grep = "{}"

[failure]
strategy = "grep"
grep = "[a-z]+"  # This one is fine
"#,
        long_pattern
    );

    // Should fail because success.grep is too long
    let result = parse_pattern_str(&toml);
    assert!(result.is_err());
}

#[test]
fn test_normal_regex_patterns_still_work() {
    // Verify normal patterns still work correctly with validation
    let toml = r#"
command_match = "^cargo test"

[success]
pattern = '(?P<passed>\d+) passed.*(?P<time>[\d.]+)s'
summary = "{passed} passed in {time}s"

[failure]
strategy = "grep"
grep = "error|warning|failed"
"#;

    let pat = parse_pattern_str(toml).unwrap();
    assert!(pat.command_match.is_match("cargo test"));
}

#[test]
fn test_complex_but_safe_regex_accepted() {
    // Complex pattern that is safe (under 500 chars)
    let complex_pattern = r#"(?P<file>[^:]+):(?P<line>\d+):(?P<col>\d+):\s+(?P<level>error|warning):\s+(?P<message>.+)"#;
    assert!(complex_pattern.len() < 500);

    let toml = format!(
        r#"
command_match = "^myapp"

[failure]
strategy = "grep"
grep = '{}'
"#,
        complex_pattern
    );

    let result = parse_pattern_str(&toml);
    assert!(result.is_ok());
}
