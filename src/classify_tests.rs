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
        Classification::Passthrough { .. } => {
            // Unknown category → passthrough
        }
        _ => panic!("expected Passthrough for unknown command"),
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
fn test_content_always_passthrough() {
    let big = "x\n".repeat(3000); // > 4KB
    let out = make_output(0, &big);
    let result = classify(&out, "git show HEAD:file", &[]);
    match result {
        Classification::Passthrough { .. } => {
            // Correct: content commands always pass through
        }
        _ => panic!("expected Passthrough for content command"),
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
fn test_unknown_defaults_to_passthrough() {
    let big = "x\n".repeat(3000); // > 4KB
    let out = make_output(0, &big);
    let result = classify(&out, "curl https://example.com", &[]);
    match result {
        Classification::Passthrough { .. } => {
            // Correct: unknown commands pass through (safe default)
        }
        _ => panic!("expected Passthrough for unknown command"),
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
