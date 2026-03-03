use crate::exec::CommandOutput;
use crate::pattern::{self, Pattern};

/// 4 KB — below this, output passes through verbatim.
const SMALL_THRESHOLD: usize = 4096;

/// Maximum lines to show in failure output before smart truncation kicks in.
const TRUNCATION_THRESHOLD: usize = 80;

/// Hard cap on total lines shown after truncation.
const MAX_LINES: usize = 120;

pub enum Classification {
    /// Exit ≠ 0. Filtered failure output.
    Failure { label: String, output: String },
    /// Exit 0, output ≤ threshold. Verbatim.
    Passthrough { output: String },
    /// Exit 0, output > threshold, pattern matched with summary.
    Success { label: String, summary: String },
    /// Exit 0, output > threshold, no pattern. Content needs indexing.
    Large {
        label: String,
        output: String,
        size: usize,
    },
}

/// Derive label from command string (first path component's filename or word).
pub fn label(command: &str) -> String {
    command
        .split_whitespace()
        .next()
        .unwrap_or("command")
        .rsplit('/')
        .next()
        .unwrap_or("command")
        .to_string()
}

pub fn classify(output: &CommandOutput, command: &str, patterns: &[Pattern]) -> Classification {
    let merged = output.merged_lossy();
    let lbl = label(command);

    // Failure path
    if output.exit_code != 0 {
        let filtered = match pattern::find_matching(command, patterns) {
            Some(pat) => {
                if let Some(failure) = &pat.failure {
                    pattern::extract_failure(failure, &merged)
                } else {
                    smart_truncate(&merged)
                }
            }
            None => smart_truncate(&merged),
        };
        return Classification::Failure {
            label: lbl,
            output: filtered,
        };
    }

    // Success, small output → passthrough
    if merged.len() <= SMALL_THRESHOLD {
        return Classification::Passthrough { output: merged };
    }

    // Success, large output — try pattern
    if let Some(pat) = pattern::find_matching(command, patterns) {
        if let Some(sp) = &pat.success {
            if let Some(summary) = pattern::extract_summary(sp, &merged) {
                return Classification::Success {
                    label: lbl,
                    summary,
                };
            }
        }
    }

    // Large, no pattern match → index
    let size = merged.len();
    Classification::Large {
        label: lbl,
        output: merged,
        size,
    }
}

/// Smart truncation: first 60% + marker + last 40%, capped at MAX_LINES.
pub fn smart_truncate(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    let total = lines.len();

    if total <= TRUNCATION_THRESHOLD {
        return output.to_string();
    }

    let budget = total.min(MAX_LINES);
    let head_count = (budget as f64 * 0.6).ceil() as usize;
    let tail_count = budget - head_count;
    let truncated = total - head_count - tail_count;

    let mut result = lines[..head_count].join("\n");
    if truncated > 0 {
        result.push_str(&format!("\n... [{truncated} lines truncated] ...\n"));
    }
    if tail_count > 0 {
        result.push_str(&lines[total - tail_count..].join("\n"));
    }
    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::CommandOutput;
    use std::time::Duration;

    fn make_output(exit_code: i32, stdout: &str) -> CommandOutput {
        CommandOutput {
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
            exit_code,
            duration: Duration::from_millis(100),
        }
    }

    #[test]
    fn test_passthrough_small_output() {
        let out = make_output(0, "hello world\n");
        let result = classify(&out, "echo hello", &[]);
        assert!(
            matches!(result, Classification::Passthrough { output } if output == "hello world\n")
        );
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
            Classification::Large { label, size, .. } => {
                assert_eq!(label, "unknown_cmd");
                assert!(size > SMALL_THRESHOLD);
            }
            _ => panic!("expected Large"),
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
}
