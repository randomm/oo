use super::*;

// ---------------------------------------------------------------------------
// validate_pattern_toml — failure section regex validation
// ---------------------------------------------------------------------------

#[test]
fn test_validate_toml_with_valid_failure_grep() {
    // strategy="grep" with a valid regex must pass validation
    let toml = r#"
command_match = "^myapp"
[failure]
strategy = "grep"
grep = "error|Error|FAILED"
"#;
    assert!(
        validate_pattern_toml(toml).is_ok(),
        "valid failure grep section should pass"
    );
}

#[test]
fn test_validate_toml_with_invalid_failure_grep() {
    // Unclosed group "error(" is an invalid regex — must be rejected
    let toml = r#"
command_match = "^myapp"
[failure]
strategy = "grep"
grep = "error("
"#;
    let result = validate_pattern_toml(toml);
    assert!(result.is_err(), "invalid grep regex should fail validation");
    let err = format!("{}", result.unwrap_err());
    assert!(
        err.contains("grep") || err.contains("regex") || err.contains("invalid"),
        "error message should mention regex/grep issue: {err}"
    );
}

#[test]
fn test_validate_toml_with_missing_grep_for_grep_strategy() {
    // strategy="grep" without a grep field must fail
    let toml = r#"
command_match = "^myapp"
[failure]
strategy = "grep"
"#;
    let result = validate_pattern_toml(toml);
    assert!(
        result.is_err(),
        "missing grep field with grep strategy should fail"
    );
    let err = format!("{}", result.unwrap_err());
    assert!(
        err.contains("grep") || err.contains("missing") || err.contains("requires"),
        "error should mention missing grep field: {err}"
    );
}

#[test]
fn test_validate_toml_with_valid_failure_tail() {
    // strategy="tail" with a lines count — no regex to validate, must pass
    let toml = r#"
command_match = "^myapp"
[failure]
strategy = "tail"
lines = 10
"#;
    assert!(
        validate_pattern_toml(toml).is_ok(),
        "strategy=tail with lines count should pass"
    );
}

#[test]
fn test_validate_toml_with_empty_grep_fails() {
    // strategy="grep" with an empty grep string must be rejected — matches everything
    let toml = r#"
command_match = "^myapp"
[failure]
strategy = "grep"
grep = ""
"#;
    let result = validate_pattern_toml(toml);
    assert!(result.is_err(), "empty grep regex should fail validation");
    let err = format!("{}", result.unwrap_err());
    assert!(
        err.contains("empty") || err.contains("grep"),
        "error should mention empty grep: {err}"
    );
}

#[test]
fn test_validate_toml_with_unknown_strategy_fails() {
    // Unknown strategy names must be rejected (pattern.rs rejects them at load time)
    let toml = r#"
command_match = "^myapp"
[failure]
strategy = "bogus_strategy"
"#;
    let result = validate_pattern_toml(toml);
    assert!(result.is_err(), "unknown strategy should fail validation");
    let err = format!("{}", result.unwrap_err());
    assert!(
        err.contains("unknown") || err.contains("strategy"),
        "error should mention unknown strategy: {err}"
    );
}

#[test]
fn test_validate_toml_failure_and_success() {
    // Both success and failure sections present and valid — must pass
    let toml = r#"
command_match = "^myapp"
[success]
pattern = '(?P<n>\d+) passed'
summary = "{n} passed"
[failure]
strategy = "grep"
grep = "FAILED|ERROR"
"#;
    assert!(
        validate_pattern_toml(toml).is_ok(),
        "valid success + failure sections should pass"
    );
}

#[test]
fn test_validate_toml_with_valid_between_strategy() {
    // strategy="between" with valid start and end regexes must pass
    let toml = r#"
command_match = "^myapp"
[failure]
strategy = "between"
start = "^error"
end = "^$"
"#;
    assert!(
        validate_pattern_toml(toml).is_ok(),
        "valid between strategy should pass"
    );
}

#[test]
fn test_validate_toml_with_between_invalid_start_regex() {
    // strategy="between" with invalid start regex must fail
    let toml = r#"
command_match = "^myapp"
[failure]
strategy = "between"
start = "error("
end = "^$"
"#;
    let result = validate_pattern_toml(toml);
    assert!(
        result.is_err(),
        "invalid start regex should fail validation"
    );
    let err = format!("{}", result.unwrap_err());
    assert!(
        err.contains("start") || err.contains("regex") || err.contains("invalid"),
        "error message should mention start/regex issue: {err}"
    );
}

#[test]
fn test_validate_toml_with_between_missing_start() {
    // strategy="between" without start field must fail
    let toml = r#"
command_match = "^myapp"
[failure]
strategy = "between"
end = "^$"
"#;
    let result = validate_pattern_toml(toml);
    assert!(
        result.is_err(),
        "between strategy without start should fail"
    );
    let err = format!("{}", result.unwrap_err());
    assert!(
        err.contains("start") || err.contains("requires"),
        "error should mention missing start field: {err}"
    );
}

#[test]
fn test_validate_toml_with_between_missing_end() {
    // strategy="between" without end field must fail
    let toml = r#"
command_match = "^myapp"
[failure]
strategy = "between"
start = "^error"
"#;
    let result = validate_pattern_toml(toml);
    assert!(result.is_err(), "between strategy without end should fail");
    let err = format!("{}", result.unwrap_err());
    assert!(
        err.contains("end") || err.contains("requires"),
        "error should mention missing end field: {err}"
    );
}

#[test]
fn test_validate_toml_with_between_empty_start_fails() {
    // strategy="between" with an empty start string must be rejected
    let toml = r#"
command_match = "^myapp"
[failure]
strategy = "between"
start = ""
end = "^$"
"#;
    let result = validate_pattern_toml(toml);
    assert!(
        result.is_err(),
        "empty between start should fail validation"
    );
    let err = format!("{}", result.unwrap_err());
    assert!(
        err.contains("start") || err.contains("empty"),
        "error should mention empty start: {err}"
    );
}

#[test]
fn test_validate_toml_with_between_empty_end_fails() {
    // strategy="between" with an empty end string must be rejected
    let toml = r#"
command_match = "^myapp"
[failure]
strategy = "between"
start = "^error"
end = ""
"#;
    let result = validate_pattern_toml(toml);
    assert!(result.is_err(), "empty between end should fail validation");
    let err = format!("{}", result.unwrap_err());
    assert!(
        err.contains("end") || err.contains("empty"),
        "error should mention empty end: {err}"
    );
}
