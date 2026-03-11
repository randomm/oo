use super::*;

fn s(s: &str) -> String {
    s.to_string()
}

// ---------------------------------------------------------------------------
// parse_action: patterns subcommand (Part 3)
// ---------------------------------------------------------------------------

#[test]
fn test_parse_action_patterns() {
    assert!(matches!(parse_action(&[s("patterns")]), Action::Patterns));
}

// ---------------------------------------------------------------------------
// cmd_patterns (Part 3)
// ---------------------------------------------------------------------------

#[test]
fn test_cmd_patterns_empty_dir() {
    let dir = tempfile::TempDir::new().unwrap();
    // Non-existent patterns dir → "no learned patterns yet"
    let code = cmd_patterns_in(dir.path().join("patterns").as_path());
    assert_eq!(code, 0, "empty patterns dir must exit 0");
}

#[test]
fn test_cmd_patterns_valid_toml() {
    let dir = tempfile::TempDir::new().unwrap();
    let patterns_dir = dir.path().join("patterns");
    std::fs::create_dir_all(&patterns_dir).unwrap();
    std::fs::write(
        patterns_dir.join("pytest.toml"),
        "command_match = \"^pytest\"\n[success]\npattern = '(?P<n>\\d+) passed'\nsummary = \"{n} passed\"\n",
    )
    .unwrap();
    let code = cmd_patterns_in(&patterns_dir);
    assert_eq!(code, 0, "valid pattern file must exit 0");
}

// ---------------------------------------------------------------------------
// Status file write / read / delete (Part 2)
// ---------------------------------------------------------------------------

#[test]
fn test_write_learn_status_creates_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let status_path = dir.path().join("learn-status.log");
    write_learn_status(&status_path, "git-status", &status_path).unwrap();
    assert!(status_path.exists(), "status file must be created");
    let content = std::fs::read_to_string(&status_path).unwrap();
    assert!(
        content.contains("git-status"),
        "status file must contain command name"
    );
}

#[test]
fn test_check_and_clear_learn_status_reads_and_deletes() {
    let dir = tempfile::TempDir::new().unwrap();
    let status_path = dir.path().join("learn-status.log");
    std::fs::write(
        &status_path,
        "learned pattern for git-status → /some/path.toml\n",
    )
    .unwrap();
    check_and_clear_learn_status(&status_path);
    assert!(
        !status_path.exists(),
        "status file must be deleted after reading"
    );
}

#[test]
fn test_check_and_clear_learn_status_missing_file_is_no_op() {
    let dir = tempfile::TempDir::new().unwrap();
    let status_path = dir.path().join("nonexistent-status.log");
    // Must not panic when file does not exist
    check_and_clear_learn_status(&status_path);
}

// ---------------------------------------------------------------------------
// write_learn_status: append mode (Part 2 extended)
// ---------------------------------------------------------------------------

#[test]
fn test_write_learn_status_appends_multiple_lines() {
    let dir = tempfile::TempDir::new().unwrap();
    let status_path = dir.path().join("learn-status.log");
    write_learn_status(&status_path, "git", &status_path).unwrap();
    write_learn_status(&status_path, "cargo", &status_path).unwrap();
    let content = std::fs::read_to_string(&status_path).unwrap();
    assert!(content.contains("git"), "first entry must be present");
    assert!(content.contains("cargo"), "second entry must be present");
    // Both lines must coexist — not overwritten
    assert_eq!(content.lines().count(), 2, "must have 2 lines");
}

#[test]
fn test_learn_config_default_has_provider() {
    // Validates LearnConfig::default() returns a usable config even without env vars.
    // The provider field is accessed directly (provider_name_from_config was inlined).
    let config = learn::LearnConfig::default();
    assert!(
        !config.provider.is_empty(),
        "default config must have a provider"
    );
    assert!(!config.model.is_empty(), "default config must have a model");
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
    assert!(matches!(
        parse_action(&[s("init")]),
        Action::Init(InitFormat::Claude)
    ));
}

#[test]
fn test_parse_action_init_format_claude() {
    let args = vec![s("init"), s("--format"), s("claude")];
    assert!(matches!(
        parse_action(&args),
        Action::Init(InitFormat::Claude)
    ));
}

#[test]
fn test_parse_action_init_format_generic() {
    let args = vec![s("init"), s("--format"), s("generic")];
    assert!(matches!(
        parse_action(&args),
        Action::Init(InitFormat::Generic)
    ));
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
    // Unknown category defaults to passthrough (safe)
    assert!(matches!(result, Classification::Passthrough { .. }));
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
    // Pattern exists but success regex doesn't match
    // pytest is Status category → returns Success with empty summary instead of Large
    let patterns = pattern::builtins();
    let refs: Vec<&pattern::Pattern> = patterns.iter().collect();
    let out = make_output(0, &"x\n".repeat(3000));
    assert!(matches!(
        classify_with_refs(&out, "pytest tests/", &refs),
        Classification::Success { summary, .. } if summary.is_empty()
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

// ---------------------------------------------------------------------------
// check_and_clear_learn_status: failure format
// ---------------------------------------------------------------------------

#[test]
fn test_check_and_clear_learn_status_failure() {
    let dir = tempfile::TempDir::new().unwrap();
    let status_path = dir.path().join("learn-status.log");

    // Write a FAILED entry directly
    write_learn_status_failure(
        &status_path,
        "cargo-test",
        "Anthropic API error: 401 Unauthorized",
    )
    .unwrap();

    // File must exist before check
    assert!(status_path.exists(), "status file must exist before check");

    // check_and_clear must not panic and must delete the file
    check_and_clear_learn_status(&status_path);

    assert!(
        !status_path.exists(),
        "status file must be deleted after check_and_clear"
    );
}

#[test]
fn test_write_learn_status_failure_format() {
    let dir = tempfile::TempDir::new().unwrap();
    let status_path = dir.path().join("learn-status.log");

    write_learn_status_failure(
        &status_path,
        "npm-run",
        "Set ANTHROPIC_API_KEY to use oo learn",
    )
    .unwrap();

    let content = std::fs::read_to_string(&status_path).unwrap();
    assert!(
        content.starts_with("FAILED npm-run:"),
        "failure line must start with 'FAILED <cmd>:'; got: {content}"
    );
    assert!(
        content.contains("ANTHROPIC_API_KEY"),
        "failure line must contain the error message; got: {content}"
    );
}

#[test]
fn test_write_learn_status_failure_multiline_error() {
    // Error with newlines should be truncated to the first line only,
    // so check_and_clear_learn_status can correctly parse the status file.
    let dir = tempfile::TempDir::new().unwrap();
    let status_path = dir.path().join("learn-status.log");

    write_learn_status_failure(
        &status_path,
        "git-log",
        "API error\ndetailed body\nmore lines",
    )
    .unwrap();

    let content = std::fs::read_to_string(&status_path).unwrap();
    // Status file must contain exactly one line (with trailing newline)
    assert_eq!(
        content.lines().count(),
        1,
        "multiline error must be truncated to a single line; got: {content:?}"
    );
    // That line must contain the first line of the error message only
    assert!(
        content.contains("API error"),
        "first line of error must be present; got: {content:?}"
    );
    assert!(
        !content.contains("detailed body"),
        "subsequent error lines must not appear in status file; got: {content:?}"
    );
}
