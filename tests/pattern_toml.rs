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
