use super::*;
use crate::exec::CommandOutput;

fn make_output(exit_code: i32, stdout: &str) -> CommandOutput {
    CommandOutput {
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
        exit_code,
    }
}

#[test]
fn test_passthrough_small_output() {
    let out = make_output(0, "hello world\n");
    let result = classify(&out, "echo hello", &[]);
    assert!(matches!(result, Classification::Passthrough { output } if output == "hello world\n"));
}

#[test]
fn test_failure_output() {
    let out = make_output(1, "error: something broke\n");
    let result = classify(&out, "some_cmd", &[]);
    match result {
        Classification::Failure { label, output } => {
            assert_eq!(label, "some_cmd");
            assert!(output.contains("something broke"));
        }
        _ => panic!("expected Failure"),
    }
}

#[test]
fn test_large_output_no_pattern() {
    let big = "x\n".repeat(3000); // > 4KB
    let out = make_output(0, &big);
    let result = classify(&out, "unknown_cmd", &[]);
    match result {
        Classification::Bounded {
            label,
            output,
            display,
            size,
        } => {
            assert_eq!(label, "unknown_cmd");
            assert_eq!(output, big, "full output must be preserved for indexing");
            assert_eq!(size, big.len());
            // Display must be bounded: ≤ DISPLAY_CAP + marker (marker varies with byte count)
            assert!(
                display.len() <= DISPLAY_CAP + 200,
                "display ({} bytes) exceeds cap + marker allowance",
                display.len()
            );
            // Display must contain the truncation marker exactly once
            let marker_count = display.matches("bytes truncated").count();
            assert_eq!(marker_count, 1, "exactly one truncation marker expected");
        }
        _ => panic!("expected Bounded for unknown command with large output"),
    }
}

#[test]
fn test_large_output_with_pattern() {
    let patterns = pattern::builtins();
    let big = format!("{}\n47 passed in 3.2s\n", ".\n".repeat(3000));
    let out = make_output(0, &big);
    let result = classify(&out, "pytest tests/", patterns);
    match result {
        Classification::Success { label, summary } => {
            assert_eq!(label, "pytest");
            assert_eq!(summary, "47 passed, 3.2s");
        }
        _ => panic!("expected Success"),
    }
}

#[test]
fn test_smart_truncation_short() {
    let lines: String = (0..50).map(|i| format!("line {i}\n")).collect();
    let result = smart_truncate(&lines);
    assert_eq!(result, lines);
    assert!(!result.contains("truncated"));
}

#[test]
fn test_smart_truncation_long() {
    let lines: String = (0..200)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let result = smart_truncate(&lines);
    assert!(result.contains("line 0"));
    assert!(result.contains("line 199"));
    assert!(result.contains("truncated"));
    // Should not exceed MAX_LINES + marker
    let result_lines: Vec<&str> = result.lines().collect();
    assert!(result_lines.len() <= MAX_LINES + 1); // +1 for truncation marker
}

#[test]
fn test_label_derivation() {
    assert_eq!(label("pytest -x"), "pytest");
    assert_eq!(label("cargo test"), "cargo");
    assert_eq!(label("gh issue list"), "gh");
    assert_eq!(label("/usr/bin/python test.py"), "python");
    // Prefix stripping: sudo/env prefixes are stripped before binary extraction.
    assert_eq!(label("sudo cargo test"), "cargo");
    assert_eq!(label("env FOO=bar cargo test"), "cargo");
    assert_eq!(label("sudo git status"), "git");
    assert_eq!(label("env A=1 B=2 pytest"), "pytest");
    // Degenerate inputs: stripping leaves no token → fall back to original first token.
    assert_eq!(label("sudo"), "sudo");
    assert_eq!(label("env"), "env");
}

#[test]
fn test_failure_with_pattern() {
    let patterns = pattern::builtins();
    let big_fail: String = (0..100).map(|i| format!("error line {i}\n")).collect();
    let out = make_output(1, &big_fail);
    let result = classify(&out, "pytest -x", &patterns);
    match result {
        Classification::Failure { label, output } => {
            assert_eq!(label, "pytest");
            // pytest failure uses tail 30
            assert!(output.contains("error line 70"));
            assert!(output.contains("error line 99"));
        }
        _ => panic!("expected Failure"),
    }
}

#[test]
fn test_empty_output_passthrough() {
    let out = make_output(0, "");
    let result = classify(&out, "true", &[]);
    assert!(matches!(result, Classification::Passthrough { output } if output.is_empty()));
}

#[test]
fn test_success_with_empty_summary_is_quiet() {
    let patterns = pattern::builtins();
    let big = "Compiling foo\n".repeat(500);
    let out = make_output(0, &big);
    let result = classify(&out, "cargo build --release", &patterns);
    match result {
        Classification::Success { summary, .. } => {
            assert!(summary.is_empty()); // quiet success
        }
        _ => panic!("expected Success with empty summary"),
    }
}

// New tests for CommandCategory detection and behavior

#[test]
fn test_detect_category_status_commands() {
    assert_eq!(detect_category("cargo test"), CommandCategory::Status);
    assert_eq!(detect_category("cargo build"), CommandCategory::Status);
    assert_eq!(detect_category("cargo clippy"), CommandCategory::Status);
    assert_eq!(detect_category("cargo fmt"), CommandCategory::Status);
    assert_eq!(detect_category("pytest tests/"), CommandCategory::Status);
    assert_eq!(detect_category("jest"), CommandCategory::Status);
    assert_eq!(detect_category("eslint src/"), CommandCategory::Status);
    assert_eq!(detect_category("ruff check"), CommandCategory::Status);
    // Prefix stripping: sudo/env prefixes are stripped before binary lookup.
    assert_eq!(detect_category("sudo cargo test"), CommandCategory::Status);
    assert_eq!(detect_category("sudo cargo build"), CommandCategory::Status);
    assert_eq!(detect_category("env cargo test"), CommandCategory::Status);
    assert_eq!(
        detect_category("env FOO=bar cargo test"),
        CommandCategory::Status
    );
    assert_eq!(
        detect_category("env A=1 B=2 pytest"),
        CommandCategory::Status
    );
    // cargo nextest run is a Status command (deep-token inspection in the cargo arm).
    assert_eq!(
        detect_category("cargo nextest run"),
        CommandCategory::Status
    );
}

#[test]
fn test_detect_category_prefix_stripped_data_and_content() {
    // Prefix stripping also applies to Data and Content categories.
    assert_eq!(detect_category("sudo git status"), CommandCategory::Data);
    assert_eq!(detect_category("sudo git log"), CommandCategory::Data);
    assert_eq!(
        detect_category("env FOO=bar git show"),
        CommandCategory::Content
    );
    assert_eq!(
        detect_category("env A=1 git diff"),
        CommandCategory::Content
    );
    // Path-qualified sudo composes with prefix stripping.
    assert_eq!(
        detect_category("/usr/bin/sudo cargo test"),
        CommandCategory::Status
    );
    assert_eq!(
        detect_category("/usr/bin/env FOO=bar cargo test"),
        CommandCategory::Status
    );
}

#[test]
fn test_detect_category_content_commands() {
    assert_eq!(
        detect_category("git show HEAD:file"),
        CommandCategory::Content
    );
    assert_eq!(detect_category("git diff HEAD~1"), CommandCategory::Content);
    assert_eq!(detect_category("cat file.txt"), CommandCategory::Content);
    assert_eq!(detect_category("bat src/main.rs"), CommandCategory::Content);
}

#[test]
fn test_detect_category_data_commands() {
    assert_eq!(detect_category("git log"), CommandCategory::Data);
    assert_eq!(detect_category("git status"), CommandCategory::Data);
    assert_eq!(detect_category("gh issue list"), CommandCategory::Data);
    assert_eq!(detect_category("gh pr list"), CommandCategory::Data);
    assert_eq!(detect_category("ls -la"), CommandCategory::Data);
    assert_eq!(detect_category("find . -name test"), CommandCategory::Data);
    assert_eq!(detect_category("grep pattern file"), CommandCategory::Data);
}

#[test]
fn test_detect_category_unknown_defaults() {
    assert_eq!(
        detect_category("curl https://example.com"),
        CommandCategory::Unknown
    );
    assert_eq!(detect_category("wget file.zip"), CommandCategory::Unknown);
    assert_eq!(
        detect_category("docker run image"),
        CommandCategory::Unknown
    );
    assert_eq!(
        detect_category("random-binary arg"),
        CommandCategory::Unknown
    );
    // Locked Unknowns: the nextest change must not accidentally promote these.
    assert_eq!(detect_category("cargo run"), CommandCategory::Unknown);
    assert_eq!(detect_category("cargo doc"), CommandCategory::Unknown);
    // A stray "test" token in a later position must not match the cargo arm.
    assert_eq!(
        detect_category("cargo run test-runner"),
        CommandCategory::Unknown
    );
    // Never strip a `sudo`/`env` token at any later position.
    assert_eq!(
        detect_category("someunknown foo sudo bar"),
        CommandCategory::Unknown
    );
    // Stripping is single-shot: `sudo sudo cargo test` is not a supported case.
    assert_eq!(
        detect_category("sudo sudo cargo test"),
        CommandCategory::Unknown
    );
    // `sudo env ...` is a stacked prefix — also not a supported case.
    assert_eq!(
        detect_category("sudo env FOO=bar cargo test"),
        CommandCategory::Unknown
    );
}

#[test]
fn test_status_no_pattern_quiet_success() {
    let big = "x\n".repeat(3000); // > 4KB
    let out = make_output(0, &big);
    let result = classify(&out, "cargo test", &[]);
    match result {
        Classification::Success { label, summary } => {
            assert_eq!(label, "cargo");
            assert!(summary.is_empty()); // quiet success
        }
        _ => panic!("expected Success with empty summary for status command"),
    }
}

#[test]
fn test_nextest_run_quiet_success() {
    // cargo nextest run is a Status command: a large nextest run with no pattern
    // match must produce Classification::Success with an empty summary (quiet
    // success), NOT Bounded/Passthrough of the full output.
    let big = "x\n".repeat(3000); // > 4KB
    let out = make_output(0, &big);
    let result = classify(&out, "cargo nextest run", &[]);
    match result {
        Classification::Success { label, summary } => {
            assert_eq!(label, "cargo");
            assert!(summary.is_empty()); // quiet success
        }
        other => panic!("expected Success (quiet) for cargo nextest run, got: {other:?}"),
    }
}

#[test]
fn test_sudo_nextest_run_quiet_success() {
    // The sudo prefix must be stripped before category detection, so
    // `sudo cargo nextest run` reaches the same Status arm as the bare form.
    let big = "x\n".repeat(3000); // > 4KB
    let out = make_output(0, &big);
    let result = classify(&out, "sudo cargo nextest run", &[]);
    match result {
        Classification::Success { label, summary } => {
            assert_eq!(label, "cargo");
            assert!(summary.is_empty()); // quiet success
        }
        other => panic!("expected Success (quiet) for sudo cargo nextest run, got: {other:?}"),
    }
}

#[test]
fn test_sudo_git_log_data_indexes() {
    // A prefixed Data command must reach the Large (index) arm, not Bounded.
    // Proves the prefix strip changes category behaviour end-to-end, not just
    // the label string.
    let big = "line\n".repeat(3000); // > 4KB
    let out = make_output(0, &big);
    let result = classify(&out, "sudo git log", &[]);
    match result {
        Classification::Large { label, size, .. } => {
            assert_eq!(label, "git");
            assert!(size > SMALL_THRESHOLD);
        }
        other => panic!("expected Large for sudo git log, got: {other:?}"),
    }
}

#[test]
fn test_env_git_show_content_bounded() {
    // A prefixed Content command must reach the Bounded arm with the git label.
    let big = "x\n".repeat(3000); // > 4KB
    let out = make_output(0, &big);
    let result = classify(&out, "env FOO=bar git show", &[]);
    match result {
        Classification::Bounded { label, .. } => {
            assert_eq!(label, "git");
        }
        other => panic!("expected Bounded for env FOO=bar git show, got: {other:?}"),
    }
}

#[test]
fn test_content_bounded_with_indexing() {
    let big = "x\n".repeat(3000); // > 4KB
    let out = make_output(0, &big);
    let result = classify(&out, "git show HEAD:file", &[]);
    match result {
        Classification::Bounded {
            label,
            output,
            display,
            size,
        } => {
            assert_eq!(label, "git");
            assert_eq!(output, big, "full output must be preserved for indexing");
            assert_eq!(size, big.len());
            assert!(
                display.len() <= DISPLAY_CAP + 200,
                "display must be byte-bounded, got {} bytes",
                display.len()
            );
            assert!(
                display.contains("bytes truncated"),
                "display must contain truncation marker"
            );
        }
        _ => panic!("expected Bounded for content command with large output"),
    }
}

#[test]
fn test_data_no_pattern_indexes() {
    let big = "line\n".repeat(3000); // > 4KB
    let out = make_output(0, &big);
    let result = classify(&out, "git log", &[]);
    match result {
        Classification::Large { label, size, .. } => {
            assert_eq!(label, "git");
            assert!(size > SMALL_THRESHOLD);
        }
        _ => panic!("expected Large (indexed) for data command"),
    }
}

#[test]
fn test_unknown_bounded_with_indexing() {
    let big = "x\n".repeat(3000); // > 4KB
    let out = make_output(0, &big);
    let result = classify(&out, "curl https://example.com", &[]);
    match result {
        Classification::Bounded {
            label,
            output,
            display,
            size,
        } => {
            assert_eq!(label, "curl");
            assert_eq!(output, big, "full output must be preserved for indexing");
            assert_eq!(size, big.len());
            assert!(
                display.len() <= DISPLAY_CAP + 200,
                "display must be byte-bounded, got {} bytes",
                display.len()
            );
            assert!(
                display.contains("bytes truncated"),
                "display must contain truncation marker"
            );
        }
        _ => panic!("expected Bounded for unknown command with large output"),
    }
}

#[test]
fn test_pattern_overrides_category() {
    let patterns = pattern::builtins();
    let big = format!("{}\n47 passed in 3.2s\n", ".\n".repeat(3000));
    let out = make_output(0, &big);
    // Status command (pytest) verified with pattern that extracts summary
    // Pattern matching overrides category classification
    let result = classify(&out, "pytest", &patterns);
    match result {
        Classification::Success { summary, .. } => {
            assert_eq!(summary, "47 passed, 3.2s");
        }
        _ => panic!("expected pattern-matched Success"),
    }
}

#[test]
fn test_category_detection_with_full_paths() {
    assert_eq!(
        detect_category("/usr/bin/cargo test"),
        CommandCategory::Status
    );
    assert_eq!(
        detect_category("/usr/local/bin/pytest"),
        CommandCategory::Status
    );
    assert_eq!(
        detect_category("/usr/bin/git show"),
        CommandCategory::Content
    );
    assert_eq!(
        detect_category("/bin/cat file.txt"),
        CommandCategory::Content
    );
    assert_eq!(
        detect_category("/usr/bin/gh issue list"),
        CommandCategory::Data
    );
    assert_eq!(detect_category("/bin/ls -la"), CommandCategory::Data);
}

#[test]
fn test_category_detection_full_paths_with_prefix_stripping() {
    // Path-qualified sudo composes with prefix stripping.
    assert_eq!(
        detect_category("/usr/bin/sudo cargo test"),
        CommandCategory::Status
    );
    // Path-qualified env composes with prefix stripping.
    assert_eq!(
        detect_category("/usr/bin/env FOO=bar cargo test"),
        CommandCategory::Status
    );
    // Path-qualified binary after stripping.
    assert_eq!(
        detect_category("sudo /usr/local/bin/cargo test"),
        CommandCategory::Status
    );
}

#[test]
fn test_package_manager_regression_lock() {
    // Regression locks: the package-manager arm matches on BINARY ONLY and
    // ignores the subcommand entirely. These MUST remain Status — do NOT add
    // subcommand inspection behind them.
    assert_eq!(detect_category("npm test"), CommandCategory::Status);
    assert_eq!(detect_category("npm run test"), CommandCategory::Status);
    assert_eq!(detect_category("yarn run build"), CommandCategory::Status);
    assert_eq!(detect_category("bun run test"), CommandCategory::Status);
    assert_eq!(detect_category("pnpm run test"), CommandCategory::Status);
    assert_eq!(detect_category("npm install"), CommandCategory::Status);
    // The go arm is also binary-only unconditional Status in this ticket.
    assert_eq!(detect_category("go run main.go"), CommandCategory::Status);
    assert_eq!(detect_category("go build"), CommandCategory::Status);
}

// ---------------------------------------------------------------------------
// Bounded classification — new tests for issue #148
// ---------------------------------------------------------------------------

#[test]
fn test_bounded_small_output_stays_passthrough() {
    // Output at exactly SMALL_THRESHOLD (4096 bytes) stays verbatim
    let small = "x".repeat(4096);
    let out = make_output(0, &small);
    let result = classify(&out, "cat file.txt", &[]);
    assert!(
        matches!(result, Classification::Passthrough { ref output } if output == &small),
        "output at exactly SMALL_THRESHOLD must stay Passthrough"
    );
}

#[test]
fn test_bounded_just_above_threshold() {
    // Output at 4097 bytes triggers Bounded
    let big = "x".repeat(4097);
    let out = make_output(0, &big);
    let result = classify(&out, "cat file.txt", &[]);
    match result {
        Classification::Bounded {
            output,
            display,
            size,
            ..
        } => {
            assert_eq!(size, 4097);
            assert_eq!(output, big, "full output preserved");
            // 4097 > DISPLAY_CAP (4096), so display must be truncated
            assert!(display.contains("bytes truncated"));
        }
        other => panic!("expected Bounded for 4097-byte output, got: {other:?}"),
    }
}

#[test]
fn test_bounded_display_has_head_and_tail() {
    // Head marker on first line, tail marker on last line
    let mut content = String::new();
    content.push_str("HEAD_MARKER_abc123 first line\n");
    for i in 0..2000 {
        content.push_str(&format!("middle line {i}\n"));
    }
    content.push_str("TAIL_MARKER_xyz789 last line\n");

    let out = make_output(0, &content);
    let result = classify(&out, "cat big_file.txt", &[]);
    match result {
        Classification::Bounded {
            display, output, ..
        } => {
            // Display must start with the first line (exact prefix)
            assert!(
                display.starts_with("HEAD_MARKER_abc123 first line"),
                "display must start with head content"
            );
            // Display must end with the last line (exact suffix)
            assert!(
                display.ends_with("TAIL_MARKER_xyz789 last line\n"),
                "display must end with tail content"
            );
            // Full output is preserved
            assert_eq!(output, content);
        }
        other => panic!("expected Bounded, got: {other:?}"),
    }
}

#[test]
fn test_bounded_single_line_no_newlines() {
    // 10KB single-line blob (minified JSON, base64) — no newlines at all
    let big = "a".repeat(10_000);
    let out = make_output(0, &big);
    let result = classify(&out, "curl https://api.example.com/data", &[]);
    match result {
        Classification::Bounded {
            display, output, ..
        } => {
            // Display must be byte-bounded
            assert!(
                display.len() <= DISPLAY_CAP + 200,
                "display ({} bytes) must be bounded",
                display.len()
            );
            // Display must contain the truncation marker
            assert!(display.contains("bytes truncated"));
            // Display must be valid UTF-8 (no split multi-byte chars)
            assert!(std::str::from_utf8(display.as_bytes()).is_ok());
            // Full output preserved
            assert_eq!(output, big);
        }
        other => panic!("expected Bounded, got: {other:?}"),
    }
}

#[test]
fn test_bounded_multibyte_char_safety() {
    // Output with multi-byte UTF-8 chars (é = 2 bytes, 中 = 3 bytes)
    let mut content = String::new();
    content.push_str("line with é and 中\n");
    for i in 0..3000 {
        content.push_str(&format!("line {i}: é中\n"));
    }
    content.push_str("last line with é and 中\n");

    let out = make_output(0, &content);
    let result = classify(&out, "cat utf8_file.txt", &[]);
    match result {
        Classification::Bounded {
            display, output, ..
        } => {
            // Display must be valid UTF-8 (no split multi-byte sequences)
            assert!(
                std::str::from_utf8(display.as_bytes()).is_ok(),
                "display must be valid UTF-8, no split multi-byte chars"
            );
            // Full output preserved
            assert_eq!(output, content);
            // Display contains the marker
            assert!(display.contains("bytes truncated"));
        }
        other => panic!("expected Bounded, got: {other:?}"),
    }
}

#[test]
fn test_bounded_marker_appears_exactly_once() {
    let big = "x\n".repeat(5000);
    let out = make_output(0, &big);
    let result = classify(&out, "cat big.txt", &[]);
    match result {
        Classification::Bounded { display, .. } => {
            let count = display.matches("bytes truncated").count();
            assert_eq!(count, 1, "truncation marker must appear exactly once");
        }
        other => panic!("expected Bounded, got: {other:?}"),
    }
}

#[test]
fn test_bounded_git_diff_content() {
    // git diff is Content category — must be Bounded, not Passthrough
    let mut diff = String::new();
    diff.push_str("diff --git a/file.rs b/file.rs\n");
    for i in 0..2000 {
        diff.push_str(&format!("-old line {i}\n+new line {i}\n"));
    }
    let out = make_output(0, &diff);
    let result = classify(&out, "git diff HEAD~1", &[]);
    match result {
        Classification::Bounded { label, display, .. } => {
            assert_eq!(label, "git");
            assert!(display.contains("bytes truncated"));
        }
        other => panic!("expected Bounded for git diff, got: {other:?}"),
    }
}

#[test]
fn test_bounded_jq_unknown() {
    // jq is Unknown category — must be Bounded
    let big_json = "x".repeat(10_000);
    let out = make_output(0, &big_json);
    let result = classify(&out, "jq . data.json", &[]);
    match result {
        Classification::Bounded { display, .. } => {
            assert!(display.contains("bytes truncated"));
        }
        other => panic!("expected Bounded for jq, got: {other:?}"),
    }
}

#[test]
fn test_bounded_cargo_run_unknown() {
    // cargo run is Unknown category (not Status) — must be Bounded
    let big = "x\n".repeat(3000);
    let out = make_output(0, &big);
    let result = classify(&out, "cargo run", &[]);
    match result {
        Classification::Bounded { .. } => {}
        other => panic!("expected Bounded for cargo run, got: {other:?}"),
    }
}

#[test]
fn test_bounded_sh_c_unknown() {
    // sh -c is ALWAYS Unknown regardless of inner command
    let big = "x\n".repeat(3000);
    let out = make_output(0, &big);
    let result = classify(&out, "sh -c 'echo hello'", &[]);
    match result {
        Classification::Bounded { .. } => {}
        other => panic!("expected Bounded for sh -c, got: {other:?}"),
    }
}

#[test]
fn test_bounded_truncate_fewer_than_two_newlines() {
    // Output with 0 newlines: single-line blob
    let big = "x".repeat(8000);
    let result = bounded_truncate(&big);
    assert!(result.contains("bytes truncated"));
    assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    assert!(result.len() <= DISPLAY_CAP + 200);

    // Output with 1 newline
    let mut one_nl = String::new();
    one_nl.push_str(&"a".repeat(4000));
    one_nl.push('\n');
    one_nl.push_str(&"b".repeat(4000));
    let result = bounded_truncate(&one_nl);
    assert!(result.contains("bytes truncated"));
    assert!(std::str::from_utf8(result.as_bytes()).is_ok());
}

#[test]
fn test_bounded_truncate_exact_threshold_no_truncation() {
    // Output exactly at DISPLAY_CAP should not be truncated
    let content = "x".repeat(DISPLAY_CAP);
    let result = bounded_truncate(&content);
    assert_eq!(result, content, "at DISPLAY_CAP, no truncation needed");
    assert!(!result.contains("bytes truncated"));
}

#[test]
fn test_bounded_truncate_head_fallback_multibyte_no_panic() {
    // Issue #148 panic regression: >= 2 newlines but none at/after head_budget.
    // Prefix "ab\ncd\n" (6 bytes, newlines at 2 and 5) followed by a long run of
    // 3-byte UTF-8 chars. raw head_budget (2457); no newline >= 2457 exists, so
    // the pre-fix `unwrap_or(head_budget) + 1` = 2458 landed inside the 中-run
    // (2458 % 3 == 1, i.e. inside the sequence 2457..2460) and `&output[..head_end]`
    // panicked: "end byte index 2458 is not a char boundary".
    let output = format!("ab\ncd\n{}", "中".repeat(4000));
    assert_eq!(
        output.len(),
        12_006,
        "sanity: 6-byte prefix + 12000 bytes of 中"
    );

    // Should not panic; result must be valid UTF-8 and bounded.
    let result = bounded_truncate(&output);
    assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    assert!(result.contains("bytes truncated"));
    assert!(
        result.len() <= DISPLAY_CAP + 200,
        "display ({} bytes) must be bounded",
        result.len()
    );
    // Head slice must keep the short lines and end on a char boundary
    assert!(result.starts_with("ab\ncd\n"));
}

#[test]
fn test_bounded_truncate_tail_fallback_overlap_guard_no_panic() {
    // Documents that the tail `unwrap_or(raw_tail)` fallback is shielded by the
    // overlap guard. Shape: two short ASCII lines, then one very long line of
    // 3-byte chars (no newline), then a short final line. So newlines are at
    // [2, 5, 6+3N]; the last is near the end. For N large enough that
    // 6+3N >= head_budget, the head's `find` SUCCEEDS at the last newline
    // (>= head_budget), giving head_end = 6+3N+1. The tail's `find(rev)` for a
    // newline < raw_tail: the last newline (6+3N) is >= raw_tail = len-1639
    // only when the final short line ("ef") is < 1639 bytes — always true. So
    // the tail takes the raw fallback, `raw_tail+1`, which is mid-char.
    // But head_end (6+3N+1) >= raw_tail (6+3N+1-... ) — we need head_end >=
    // tail_start to trigger the overlap guard. This holds because the head
    // snapped to a newline that is >= raw_tail, so head_end >= raw_tail+1 =
    // tail_start. The guard returns (head_end, len) and never uses the bad
    // tail_start. No panic, valid UTF-8.
    let output = format!("ab\ncd\n{}ef", "中".repeat(1000));
    let len = output.len();
    let head_budget = (DISPLAY_CAP as f64 * 0.6) as usize; // 2457
    let tail_budget = DISPLAY_CAP - head_budget; // 1639
    let raw_tail = len.saturating_sub(tail_budget);
    // Precondition: the last newline (6+3N) is >= head_budget so the head
    // `find` succeeds, and the tail `find` fails (all nl >= raw_tail) so the
    // tail takes the raw fallback which lands mid-char.
    let last_nl = 6 + 3 * 1000;
    assert!(last_nl >= head_budget, "head find must succeed");
    assert!(last_nl >= raw_tail, "tail find must fall back");
    assert!(
        !output.is_char_boundary(raw_tail),
        "precondition: raw_tail is mid-char"
    );

    let result = bounded_truncate(&output);
    assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    // Overlap guard returned (head_end, len): the final "ef" is kept at the end.
    assert!(result.starts_with("ab\ncd\n"));
    assert!(result.ends_with("ef"));
}

#[test]
fn test_bounded_truncate_overlap_guard_tail_max_is_load_bearing() {
    // Proves `ceil_char_boundary(output, raw_tail).max(tail_start)` in
    // bounded_truncate is LOAD-BEARING in the overlap-guard case
    // (cut_boundaries returns tail_start = output.len()).
    //
    // Setup: one very long line (bytes 0..5000), newline at 5000, then two
    // short lines "ab\ncd\n" (bytes 5001..5007, newlines at 5003 and 5006,
    // trailing newline at 5006, len = 5007 > DISPLAY_CAP = 4096). raw_tail =
    // 5007 - 1639 = 3368. head_budget = 2457. Newline at/after head_budget:
    // YES — newline at 5000 >= 2457, so head_end = 5001. Last newline before
    // raw_tail (3368): none (newlines at 5000, 5003, 5006 are all >= 3368).
    // So tail_start falls back to ceil_char_boundary(3368) = 3368. head_end
    // (5001) >= tail_start (3368) → overlap guard fires, returns
    // (5001, len=5007): tail = "" (empty).
    //
    // If the `.max(tail_start)` were removed, clamped_tail would collapse to
    // ceil_char_boundary(output, 3368) = 3368, and the display would be
    // output[..2457] + marker + output[3368..] — i.e. the region
    // output[3368..5000] (1632 bytes of x) appearing TWICE (once in the
    // head via the floor clamp, once in the tail). That is the bug the
    // `.max` prevents.
    //
    // With the `.max` in place, the guard wins: tail stays `output[len..]`
    // = "". clamped_head = floor(2457).min(5001) = 2457. Display =
    // output[..2457] + marker + "" — no duplication.
    let mut output = String::new();
    output.push_str(&"x".repeat(5000));
    output.push('\n'); // byte 5000
    output.push_str("ab\n"); // bytes 5001..5004
    output.push_str("cd\n"); // bytes 5004..5007 (last byte 5006 is '\n')
    assert_eq!(output.len(), 5007, "sanity: 5000 x's + \n + ab\n + cd\n");
    let head_budget = (DISPLAY_CAP as f64 * 0.6) as usize; // 2457
    let tail_budget = DISPLAY_CAP - head_budget; // 1639
    let raw_tail = output.len().saturating_sub(tail_budget); // 5007 - 1639 = 3368
    assert_eq!(raw_tail, 3368, "raw_tail for this shape");

    let (head_end, tail_start) = cut_boundaries(&output, head_budget, tail_budget);
    assert_eq!(
        tail_start,
        output.len(),
        "overlap guard must return len as tail_start"
    );
    assert_eq!(head_end, 5001, "head must snap to newline at 5000 (+1)");

    let result = bounded_truncate(&output);
    assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    assert!(result.contains("bytes truncated"));
    assert!(
        result.len() <= DISPLAY_CAP + 200,
        "display ({} bytes) must be bounded",
        result.len()
    );
    // Tail slice must be empty (guard returned len): the display ends right
    // after the marker, NOT with the x-run re-appended. If .max(tail_start)
    // were removed, output[3368..] would be re-appended after the marker,
    // and the "x" run (bytes 3368..5000) would appear twice.
    // The marker is `... [N bytes truncated → use `oo recall` to query] ...\n`
    // — it ends with `] ...\n` (three dots, not `]]`). Split on `to query] ...
    // to get the tail after the marker.
    let after_marker = result.split("to query] ...\n").nth(1).map(|s| s);
    assert!(
        after_marker.is_some_and(|tail| tail.is_empty()),
        "tail after marker must be empty when overlap guard wins, got: {after_marker:?}"
    );
}

#[test]
fn test_bounded_truncate_long_lines_cap_still_enforced() {
    // HIGH-1 defect: head/tail cuts "snap to newline boundaries, at most one
    // line of drift", but a line can be enormous (minified JS/JSON, base64).
    // 300KB output whose only newlines are at 100000 and 200000: pre-fix the
    // head snapped to 100001 and the tail to 200001, yielding a ~200KB display
    // — ~50x DISPLAY_CAP — while the marker still claimed the output was
    // bounded. Line-snapping may only ever SHRINK a slice relative to the
    // byte budget, never grow it past it.
    let mut output = String::new();
    output.push_str(&"a".repeat(100_000));
    output.push('\n');
    output.push_str(&"b".repeat(100_000));
    output.push('\n');
    output.push_str(&"c".repeat(99_999));
    assert_eq!(
        output.len(),
        300_001,
        "sanity: two 100KB lines + 99999 tail"
    );

    let result = bounded_truncate(&output);
    assert!(
        result.len() <= DISPLAY_CAP + 200,
        "display ({} bytes) must be bounded by DISPLAY_CAP + marker allowance for \
         output with only {} newlines",
        result.len(),
        output.bytes().filter(|b| *b == b'\n').count()
    );
    assert!(
        std::str::from_utf8(result.as_bytes()).is_ok(),
        "display must remain valid UTF-8"
    );
    assert!(
        result.contains("bytes truncated"),
        "display must still carry the truncation marker"
    );
}

#[test]
fn test_bounded_truncate_single_mid_output_newline() {
    // Only one newline exists (mid-output): the <2-newline path keeps both
    // halves within budget, not the whole line on either side.
    let mut output = String::new();
    output.push_str(&"x".repeat(150_000));
    output.push('\n');
    output.push_str(&"y".repeat(150_000));

    let result = bounded_truncate(&output);
    assert!(
        result.len() <= DISPLAY_CAP + 200,
        "display ({} bytes) must be bounded",
        result.len()
    );
    assert!(std::str::from_utf8(result.as_bytes()).is_ok());
}

#[test]
fn test_bounded_truncate_display_cap_boundary() {
    // Output just above DISPLAY_CAP: head+tail + marker must fit
    let big = "x".repeat(DISPLAY_CAP + 100);
    let result = bounded_truncate(&big);
    assert!(result.contains("bytes truncated"));
    // The marker line must be on its own line
    let marker_lines: Vec<&str> = result
        .lines()
        .filter(|l| l.contains("bytes truncated"))
        .collect();
    assert_eq!(marker_lines.len(), 1, "exactly one marker line");
}

// ---------------------------------------------------------------------------
// Issue #149 — sudo/env prefix stripping + cargo nextest run category
// ---------------------------------------------------------------------------
//
// These tests target the CATEGORY FALLBACK in `detect_category` and the label
// derived by `label`. They use `&[]` (empty pattern list) so the category
// fallback is the ONLY path — the built-in cargo build/test/clippy/fmt
// patterns would otherwise match before the category is consulted, masking
// the behaviour under test.
//
// Regression locks (npm test, npm run test, etc.) also use `&[]`: the issue
// says the npm arm "ignores subcommand", which is a property of the CATEGORY
// detector, not the pattern layer. Empty patterns isolate the category.
// ---------------------------------------------------------------------------

#[test]
fn test_detect_category_sudo_prefixed() {
    // `sudo cargo test` — sudo stripped, binary cargo, subcommand test → Status
    assert_eq!(detect_category("sudo cargo test"), CommandCategory::Status,);
    // `sudo pytest` — sudo stripped, binary pytest → Status
    assert_eq!(detect_category("sudo pytest"), CommandCategory::Status,);
    // `sudo git status` — sudo stripped, binary git, subcommand status → Data
    assert_eq!(detect_category("sudo git status"), CommandCategory::Data,);
}

#[test]
fn test_detect_category_env_prefixed() {
    // `env cargo test` — env stripped, binary cargo, subcommand test → Status
    assert_eq!(detect_category("env cargo test"), CommandCategory::Status,);
    // `env FOO=bar cargo test` — env + KEY=VALUE tokens skipped, binary cargo → Status
    assert_eq!(
        detect_category("env FOO=bar cargo test"),
        CommandCategory::Status,
    );
    // `env A=1 B=2 pytest` — env + multiple KEY=VALUE tokens skipped → Status
    assert_eq!(
        detect_category("env A=1 B=2 pytest"),
        CommandCategory::Status,
    );
}

#[test]
fn test_detect_category_cargo_nextest() {
    // `cargo nextest run` — cargo arm must recognise nextest run as Status
    assert_eq!(
        detect_category("cargo nextest run"),
        CommandCategory::Status,
    );
}

#[test]
fn test_detect_category_sudo_full_path() {
    // `/usr/bin/sudo cargo test` — path-qualified sudo must be stripped
    // (rsplit('/') applied to the prefix token, then sudo stripped)
    assert_eq!(
        detect_category("/usr/bin/sudo cargo test"),
        CommandCategory::Status,
    );
}

#[test]
fn test_detect_category_locked_unknowns() {
    // These must REMAIN Unknown after the fix — the nextest change must not
    // accidentally promote them, and the prefix strip must not over-fire.

    // cargo run — program stdout is not build status
    assert_eq!(detect_category("cargo run"), CommandCategory::Unknown,);
    // cargo doc — documentation generation, not build status
    assert_eq!(detect_category("cargo doc"), CommandCategory::Unknown,);
    // cargo run test-runner — deep-token scan must not match stray "test" token
    assert_eq!(
        detect_category("cargo run test-runner"),
        CommandCategory::Unknown,
    );
    // someunknown foo sudo bar — sudo at a later position must NOT be stripped
    assert_eq!(
        detect_category("someunknown foo sudo bar"),
        CommandCategory::Unknown,
    );
}

#[test]
fn test_detect_category_regression_locks() {
    // These must REMAIN Status — the npm|yarn|pnpm|bun arm matches on binary
    // only and ignores the subcommand. The fix must not add subcommand
    // inspection behind the package-manager arm (that would regress them).
    assert_eq!(detect_category("npm test"), CommandCategory::Status);
    assert_eq!(detect_category("npm run test"), CommandCategory::Status,);
    assert_eq!(detect_category("yarn run build"), CommandCategory::Status,);
    assert_eq!(detect_category("bun run test"), CommandCategory::Status,);
    assert_eq!(detect_category("pnpm run build"), CommandCategory::Status,);
    assert_eq!(detect_category("npm install"), CommandCategory::Status,);
}

#[test]
fn test_label_prefix_stripping() {
    // `sudo cargo test` → "cargo" (sudo stripped)
    assert_eq!(label("sudo cargo test"), "cargo");
    // `env FOO=bar cargo test` → "cargo" (env + KEY=VALUE tokens skipped)
    assert_eq!(label("env FOO=bar cargo test"), "cargo");
    // `env cargo test` → "cargo" (env stripped)
    assert_eq!(label("env cargo test"), "cargo");
    // `sudo git status` → "git" (sudo stripped)
    assert_eq!(label("sudo git status"), "git");
}

#[test]
fn test_label_degenerate_prefix_only() {
    // Decision 4: if no token remains after stripping, label degrades to the
    // original first token. Must not panic, must not return empty.
    assert_eq!(label("sudo"), "sudo");
    assert_eq!(label("env"), "env");
    // Single token with no prefix
    assert_eq!(label("cargo"), "cargo");
}

#[test]
fn test_classify_sudo_cargo_test_quiet_success() {
    // End-to-end through classify() with the cmd_learn pattern set.
    // >4KB non-matching output, command "sudo cargo test" → Success (quiet),
    // label "cargo" (not "sudo").
    //
    // Uses `&[]` (no patterns) so the category fallback is the only path.
    // The built-in `cargo test` pattern would match "cargo test" in the
    // command string and extract a summary — that would mask the category
    // fallback and the label fix.
    let big = "x\n".repeat(3000); // > 4KB
    let out = make_output(0, &big);
    let result = classify(&out, "sudo cargo test", &[]);
    match result {
        Classification::Success { label, summary } => {
            assert_eq!(
                label, "cargo",
                "label must be the stripped binary, not 'sudo'"
            );
            assert!(summary.is_empty(), "quiet success must have empty summary");
        }
        other => panic!("expected Success (quiet) for sudo cargo test, got: {other:?}"),
    }
}

#[test]
fn test_classify_env_cargo_test_quiet_success() {
    // End-to-end through classify() with env prefix + KEY=VALUE tokens.
    // >4KB non-matching output, command "env FOO=bar cargo test" → Success
    // (quiet), label "cargo" (not "env").
    let big = "x\n".repeat(3000); // > 4KB
    let out = make_output(0, &big);
    let result = classify(&out, "env FOO=bar cargo test", &[]);
    match result {
        Classification::Success { label, summary } => {
            assert_eq!(
                label, "cargo",
                "label must be the stripped binary, not 'env'"
            );
            assert!(summary.is_empty(), "quiet success must have empty summary");
        }
        other => panic!("expected Success (quiet) for env FOO=bar cargo test, got: {other:?}"),
    }
}

#[test]
fn test_classify_cargo_nextest_run_quiet_success() {
    // End-to-end through classify() with the real builtin pattern set.
    // `cargo nextest run` has NO matching builtin pattern (no `nextest` in
    // builtins.rs), so the category fallback must fire: detect_category
    // returns Status → Success (quiet), label "cargo".
    //
    // This is the exact behaviour the ticket describes: "a large successful
    // `oo cargo nextest run` produces `Classification::Success { summary: "" }
    // (quiet success `✓ cargo`) instead of Passthrough of the full 100KB+
    // output."
    let patterns = pattern::builtins();
    let big = "x\n".repeat(3000); // > 4KB
    let out = make_output(0, &big);
    let result = classify(&out, "cargo nextest run", &patterns);
    match result {
        Classification::Success { label, summary } => {
            assert_eq!(label, "cargo");
            assert!(summary.is_empty(), "quiet success must have empty summary");
        }
        other => panic!("expected Success (quiet) for cargo nextest run, got: {other:?}"),
    }
}

#[test]
fn test_classify_sudo_cargo_nextest_run_quiet_success() {
    // Same as above but with sudo prefix: `sudo cargo nextest run` →
    // Success (quiet), label "cargo" (not "sudo").
    let patterns = pattern::builtins();
    let big = "x\n".repeat(3000); // > 4KB
    let out = make_output(0, &big);
    let result = classify(&out, "sudo cargo nextest run", &patterns);
    match result {
        Classification::Success { label, summary } => {
            assert_eq!(
                label, "cargo",
                "label must be the stripped binary, not 'sudo'"
            );
            assert!(summary.is_empty(), "quiet success must have empty summary");
        }
        other => panic!("expected Success (quiet) for sudo cargo nextest run, got: {other:?}"),
    }
}

#[test]
fn test_classify_sudo_git_log_large() {
    // `sudo git log` — git arm, subcommand "log" → Data category → Large
    // (indexed for recall). Proves the prefix strip changes category
    // behaviour end-to-end, not just the label string.
    let big = "line\n".repeat(3000); // > 4KB
    let out = make_output(0, &big);
    let result = classify(&out, "sudo git log", &[]);
    match result {
        Classification::Large { label, size, .. } => {
            assert_eq!(
                label, "git",
                "label must be the stripped binary, not 'sudo'"
            );
            assert!(size > SMALL_THRESHOLD);
        }
        other => panic!("expected Large (Data category) for sudo git log, got: {other:?}"),
    }
}
