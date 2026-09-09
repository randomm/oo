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
    assert!(matches!(parse_action(&args), Action::Learn(a, None) if a == vec!["cargo", "test"]));
}

#[test]
fn test_parse_action_learn_no_subargs() {
    let args = vec![s("learn")];
    assert!(matches!(parse_action(&args), Action::Learn(a, None) if a.is_empty()));
}

#[test]
fn test_parse_action_learn_with_hint() {
    let args = vec![
        s("learn"),
        s("--hint"),
        s("keep last 10 lines"),
        s("cargo"),
        s("test"),
    ];
    assert!(
        matches!(parse_action(&args), Action::Learn(a, Some(h)) if a == vec!["cargo", "test"] && h == "keep last 10 lines")
    );
}

#[test]
fn test_parse_action_learn_hint_missing_value() {
    // When --hint is the last argument with no value, treat as no hint
    let args = vec![s("learn"), s("--hint")];
    assert!(matches!(parse_action(&args), Action::Learn(a, None) if a.is_empty()));
}

#[test]
fn test_parse_action_learn_with_hint_followed_by_command() {
    // When --hint is followed by a value, use it as the hint
    let args = vec![
        s("learn"),
        s("--hint"),
        s("capture summary only"),
        s("cargo"),
        s("test"),
    ];
    assert!(
        matches!(parse_action(&args), Action::Learn(a, Some(h)) if a == vec!["cargo", "test"] && h == "capture summary only")
    );
}

#[test]
fn test_parse_action_learn_hint_multiple_words() {
    let args = vec![
        s("learn"),
        s("--hint"),
        s("show summary line and keep last 5 on failure"),
        s("pytest"),
    ];
    assert!(
        matches!(parse_action(&args), Action::Learn(a, Some(h)) if a == vec!["pytest"] && h == "show summary line and keep last 5 on failure")
    );
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
fn test_classify_passthrough_small() {
    let out = make_output(0, "hello\n");
    let result = classify::classify(&out, "echo hello", &[]);
    assert!(matches!(result, Classification::Passthrough { output } if output == "hello\n"));
}

#[test]
fn test_classify_failure_no_pattern() {
    let out = make_output(1, "something went wrong\n");
    let result = classify::classify(&out, "bad_cmd", &[]);
    assert!(matches!(result, Classification::Failure { label, .. } if label == "bad_cmd"));
}

#[test]
fn test_classify_large_no_pattern() {
    let out = make_output(0, &"x\n".repeat(3000));
    let result = classify::classify(&out, "some_tool", &[]);
    // Unknown category with large output → Bounded (indexed, byte-bounded display)
    match result {
        Classification::Bounded {
            output,
            display,
            size,
            ..
        } => {
            assert_eq!(size, 6000);
            assert!(display.len() <= classify::DISPLAY_CAP + 200);
            assert!(output.len() > classify::DISPLAY_CAP);
        }
        _ => panic!("expected Bounded for unknown command with large output"),
    }
}

#[test]
fn test_classify_success_with_pattern() {
    let patterns = pattern::builtins();
    let big = format!("{}47 passed in 3.2s\n", ".\n".repeat(3000));
    let out = make_output(0, &big);
    let result = classify::classify(&out, "pytest tests/", patterns);
    assert!(
        matches!(result, Classification::Success { summary, .. } if summary.contains("47 passed"))
    );
}

#[test]
fn test_classify_failure_with_pattern() {
    let patterns = pattern::builtins();
    let fail_output: String = (0..50).map(|i| format!("error line {i}\n")).collect();
    let out = make_output(1, &fail_output);
    match classify::classify(&out, "pytest -x", patterns) {
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
    assert_eq!(cmd_learn(&[], None), 1);
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
    let out = make_output(0, &"x\n".repeat(3000));
    assert!(matches!(
        classify::classify(&out, "pytest tests/", patterns),
        Classification::Success { summary, .. } if summary.is_empty()
    ));
}

#[test]
fn test_classify_failure_pattern_extract_failure() {
    let patterns = pattern::builtins();
    let fail: String = (0..30).map(|i| format!("FAILED test{i}\n")).collect();
    let out = make_output(1, &fail);
    assert!(matches!(
        classify::classify(&out, "pytest -v", patterns),
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
    let code = cmd_learn(&[s("echo"), s("hello_learn_test")], None);
    assert_eq!(code, 0, "echo must succeed, got: {code}");
}

#[test]
fn test_cmd_learn_failure_branch() {
    // cmd_learn with a command that fails → Failure branch in classification.
    let code = cmd_learn(&[s("false")], None);
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

// ---------------------------------------------------------------------------
// savings_suffix / render_classification: compression savings reporting
// ---------------------------------------------------------------------------

/// Golden: the exact `format_size(saved, BINARY)` string is pinned for a
/// fixed synthetic size (success-with-summary form).
#[test]
fn test_savings_suffix_golden_success_line() {
    // original 5000 B, base line "✓ pytest (47 passed)" (22 B: ✓ is 3 bytes)
    // -> 4978 saved -> "4.86 KiB".
    let line = "\u{2713} pytest (47 passed)";
    assert_eq!(line.len(), 22);
    assert_eq!(
        savings_suffix(5000, line.len()).as_deref(),
        Some(" [saved 4.86 KiB]")
    );
}

/// Golden: quiet success (empty summary) — the quiet `✓ {label}` form is
/// the largest compression win in the product and must carry the figure.
#[test]
fn test_savings_suffix_quiet_success_golden() {
    let line = "\u{2713} cargo build";
    assert_eq!(
        savings_suffix(100_000, line.len()).as_deref(),
        Some(" [saved 97.64 KiB]")
    );
}

/// Golden: the Failure indicator line is `✗ {label}` — the filtered output
/// lines that follow are NOT part of the displayed byte count.
#[test]
fn test_savings_suffix_failure_arm_golden() {
    let line = "\u{2717} pytest";
    assert_eq!(
        savings_suffix(50_000, line.len()).as_deref(),
        Some(" [saved 48.82 KiB]")
    );
}

/// Boundary: `saved == MIN_SAVINGS` exactly → no suffix (suppression is
/// `saved <= MIN_SAVINGS`, not `<`).
#[test]
fn test_savings_suffix_suppressed_at_or_below_threshold() {
    assert_eq!(savings_suffix(4096, 0), None);
    // just below
    assert_eq!(savings_suffix(5120, 1032), None);
}

/// Boundary: `saved == MIN_SAVINGS + 1` → suffix appears.
#[test]
fn test_savings_suffix_positive_just_above_threshold() {
    assert_eq!(
        savings_suffix(4097, 0).as_deref(),
        Some(" [saved 4.00 KiB]")
    );
}

/// Non-positive delta: summary nearly as long as the input (or longer) →
/// no suffix, no panic (saturating arithmetic).
#[test]
fn test_savings_suffix_non_positive_saving_is_suppressed() {
    assert_eq!(savings_suffix(5000, 5000), None);
    assert_eq!(savings_suffix(100, 200), None);
}

/// Non-UTF-8 input: `merged_lossy()` replaces invalid bytes with the
/// U+FFFD replacement char (3 bytes each), so the lossy byte length is what
/// feeds the suffix — the helper must not panic and must measure from the
/// lossy length, not the raw vec length.
#[test]
fn test_savings_suffix_non_utf8_no_panic() {
    // 1000 bytes of 0xff (invalid UTF-8) followed by 5000 valid bytes:
    // lossy length is 1000 * 3 + 5000 = 8000, NOT 6000.
    let mut raw = vec![0xffu8; 1000];
    raw.extend(std::iter::repeat(b'x').take(5000));
    let out = exec::CommandOutput {
        stdout: raw,
        stderr: Vec::new(),
        exit_code: 0,
    };
    let merged = out.merged_lossy();
    let merged_len = merged.len();
    assert_eq!(
        merged_len, 8000,
        "lossy length must account for replacement chars"
    );

    // line (10) + " [saved " (8) + "7.80 KiB" (8) + "]" (1) = 27 bytes
    // displayed, so the suffix must render exactly "7.80 KiB".
    let line = "\u{2713} cargo";
    let suffix = savings_suffix(merged_len, line.len()).expect("8 KiB must exceed MIN_SAVINGS");
    assert_eq!(suffix, " [saved 7.80 KiB]");
    assert_eq!(merged_len - (line.len() + suffix.len()), 7974,);
}

/// Passthrough: small output classifies as Passthrough — no indicator line,
/// so no suffix applies regardless of `original_size`.
#[test]
fn test_savings_passthrough_unchanged() {
    let c = classify::classify(&make_output(0, "hello\n"), "echo hi", &[]);
    assert!(matches!(c, Classification::Passthrough { .. }));
}

/// Regression lock: the Large arm's `size` field is carried through
/// unchanged — its `indexed N` wording must not gain a `[saved …]` figure.
#[test]
fn test_savings_large_arm_wording_unchanged() {
    let c = classify::classify(&make_output(0, &"x\n".repeat(3000)), "gh api x", &[]);
    match c {
        Classification::Large { size, .. } => assert_eq!(size, 6000),
        _ => panic!("expected Large for Data-category command with large output"),
    }
}
