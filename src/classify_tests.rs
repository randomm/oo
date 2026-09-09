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
