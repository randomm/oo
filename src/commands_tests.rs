use super::*;

fn s(s: &str) -> String {
    s.to_string()
}

#[test]
fn test_parse_action_no_args_is_help() {
    assert!(matches!(parse_action(&[]), Action::Help(None)));
}

#[test]
fn test_parse_action_recall_single_word() {
    let args = vec![s("recall"), s("cargo")];
    assert!(matches!(parse_action(&args), Action::Recall(q) if q == "cargo"));
}

#[test]
fn test_parse_action_recall_multi_word_joins() {
    let args = vec![s("recall"), s("hello"), s("world")];
    assert!(matches!(parse_action(&args), Action::Recall(q) if q == "hello world"));
}

#[test]
fn test_parse_action_recall_empty_query() {
    let args = vec![s("recall")];
    assert!(matches!(parse_action(&args), Action::Recall(q) if q.is_empty()));
}

#[test]
fn test_parse_action_forget() {
    assert!(matches!(parse_action(&[s("forget")]), Action::Forget));
}

#[test]
fn test_parse_action_learn() {
    let args = vec![s("learn"), s("cargo"), s("test")];
    assert!(matches!(parse_action(&args), Action::Learn(a) if a == vec!["cargo", "test"]));
}

#[test]
fn test_parse_action_learn_no_subargs() {
    let args = vec![s("learn")];
    assert!(matches!(parse_action(&args), Action::Learn(a) if a.is_empty()));
}

#[test]
fn test_parse_action_version() {
    assert!(matches!(parse_action(&[s("version")]), Action::Version));
}

#[test]
fn test_parse_action_help_no_cmd() {
    assert!(matches!(parse_action(&[s("help")]), Action::Help(None)));
}

#[test]
fn test_parse_action_help_with_cmd() {
    let args = vec![s("help"), s("ls")];
    assert!(matches!(parse_action(&args), Action::Help(Some(c)) if c == "ls"));
}

#[test]
fn test_parse_action_init() {
    assert!(matches!(parse_action(&[s("init")]), Action::Init));
}

#[test]
fn test_parse_action_run_unknown() {
    let args = vec![s("echo"), s("hi")];
    assert!(matches!(parse_action(&args), Action::Run(a) if a[0] == "echo"));
}

#[test]
fn test_parse_action_run_hyphen_arg() {
    let args = vec![s("ls"), s("-la")];
    assert!(matches!(parse_action(&args), Action::Run(a) if a == vec!["ls", "-la"]));
}

fn make_output(exit_code: i32, stdout: &str) -> exec::CommandOutput {
    exec::CommandOutput {
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
        exit_code,
    }
}

#[test]
fn test_classify_with_refs_passthrough_small() {
    let out = make_output(0, "hello\n");
    let result = classify_with_refs(&out, "echo hello", &[]);
    assert!(matches!(result, Classification::Passthrough { output } if output == "hello\n"));
}

#[test]
fn test_classify_with_refs_failure_no_pattern() {
    let out = make_output(1, "something went wrong\n");
    let result = classify_with_refs(&out, "bad_cmd", &[]);
    assert!(matches!(result, Classification::Failure { label, .. } if label == "bad_cmd"));
}

#[test]
fn test_classify_with_refs_large_no_pattern() {
    let out = make_output(0, &"x\n".repeat(3000));
    let result = classify_with_refs(&out, "some_tool", &[]);
    assert!(matches!(result, Classification::Large { label, .. } if label == "some_tool"));
}

#[test]
fn test_classify_with_refs_success_with_pattern() {
    let patterns = pattern::builtins();
    let refs: Vec<&pattern::Pattern> = patterns.iter().collect();
    let big = format!("{}47 passed in 3.2s\n", ".\n".repeat(3000));
    let out = make_output(0, &big);
    let result = classify_with_refs(&out, "pytest tests/", &refs);
    assert!(
        matches!(result, Classification::Success { summary, .. } if summary.contains("47 passed"))
    );
}

#[test]
fn test_classify_with_refs_failure_with_pattern() {
    let patterns = pattern::builtins();
    let refs: Vec<&pattern::Pattern> = patterns.iter().collect();
    let fail_output: String = (0..50).map(|i| format!("error line {i}\n")).collect();
    let out = make_output(1, &fail_output);
    match classify_with_refs(&out, "pytest -x", &refs) {
        Classification::Failure { label, output } => {
            assert_eq!(label, "pytest");
            assert!(output.contains("error line 49"));
        }
        _ => panic!("expected Failure"),
    }
}

#[test]
fn test_cmd_recall_empty_query_returns_1() {
    assert_eq!(cmd_recall(""), 1);
}

#[test]
fn test_cmd_learn_no_args_returns_1() {
    assert_eq!(cmd_learn(&[]), 1);
}

#[test]
fn test_cmd_run_empty_args_returns_1() {
    assert_eq!(cmd_run(&[]), 1);
}

#[test]
fn test_cmd_run_echo_exits_zero() {
    assert_eq!(cmd_run(&[s("echo"), s("hello")]), 0);
}

#[test]
fn test_cmd_run_false_exits_nonzero() {
    assert_ne!(cmd_run(&[s("false")]), 0);
}

#[test]
fn test_cmd_run_nonexistent_command_returns_1() {
    // Exec failure → Err branch → exit code 1
    assert_eq!(cmd_run(&[s("__oo_no_such_command_xyz__")]), 1);
}

#[test]
fn test_cmd_help_empty_cmd_returns_1() {
    assert_eq!(cmd_help(""), 1);
}

#[test]
fn test_classify_large_with_pattern_no_summary_match() {
    // Pattern exists but success regex doesn't match → Large (not Success)
    let patterns = pattern::builtins();
    let refs: Vec<&pattern::Pattern> = patterns.iter().collect();
    let out = make_output(0, &"x\n".repeat(3000));
    assert!(matches!(
        classify_with_refs(&out, "pytest tests/", &refs),
        Classification::Large { .. }
    ));
}

#[test]
fn test_classify_failure_pattern_extract_failure() {
    let patterns = pattern::builtins();
    let refs: Vec<&pattern::Pattern> = patterns.iter().collect();
    let fail: String = (0..30).map(|i| format!("FAILED test{i}\n")).collect();
    let out = make_output(1, &fail);
    assert!(matches!(
        classify_with_refs(&out, "pytest -v", &refs),
        Classification::Failure { .. }
    ));
}

#[test]
fn test_try_index_no_panic() {
    let _ = try_index("test command", "some output content");
}

#[test]
fn test_cmd_recall_does_not_panic() {
    // Verifies cmd_recall does not panic and returns a valid exit code.
    // We cannot guarantee the store opens in all test environments, so both
    // 0 (store ok, query ran) and 1 (store error) are acceptable outcomes.
    let code = cmd_recall("unique_recall_test_content_xyz_42");
    assert!(
        code == 0 || code == 1,
        "cmd_recall must return 0 or 1, got: {code}"
    );
}

#[test]
fn test_cmd_forget_does_not_panic() {
    // Verifies cmd_forget does not panic and returns a valid exit code.
    // We cannot guarantee the store opens in all test environments, so both
    // 0 (store ok, delete ran) and 1 (store error) are acceptable outcomes.
    let code = cmd_forget();
    assert!(
        code == 0 || code == 1,
        "cmd_forget must return 0 or 1, got: {code}"
    );
}

#[test]
fn test_cmd_learn_passthrough_small_output() {
    // cmd_learn with a command that produces small output (< 4 KiB) → Passthrough branch.
    // spawn_background will fail (no binary in PATH during test), but that is non-fatal.
    // We only care that the exit code matches the command's actual exit code.
    let code = cmd_learn(&[s("echo"), s("hello_learn_test")]);
    assert_eq!(code, 0, "echo must succeed, got: {code}");
}

#[test]
fn test_cmd_learn_failure_branch() {
    // cmd_learn with a command that fails → Failure branch in classification.
    let code = cmd_learn(&[s("false")]);
    assert_ne!(
        code, 0,
        "false must produce non-zero exit code, got: {code}"
    );
}
