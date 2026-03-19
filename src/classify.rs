//! Command output classification and intelligent truncation.
//!
//! This module is the core of `oo`'s context-efficient output handling. It analyzes
//! command results and produces one of four [`Classification`] outcomes:
//!
//! - **Failure**: Non-zero exit codes → filtered error output
//! - **Passthrough**: Small successful outputs (<4KB) → verbatim
//! - **Success**: Large successful outputs with pattern match → compressed summary
//! - **Large**: Large successful outputs without pattern → indexed for recall
//!
//! The [`classify`] function combines pattern matching with automatic command category
//! detection to make intelligent decisions about how to present output.

use crate::exec::CommandOutput;
use crate::pattern::{self, Pattern};

/// 4 KB — below this, output passes through verbatim.
pub const SMALL_THRESHOLD: usize = 4096;

/// Maximum lines to show in failure output before smart truncation kicks in.
const TRUNCATION_THRESHOLD: usize = 80;

/// Hard cap on total lines shown after truncation.
const MAX_LINES: usize = 120;

/// Command category — determines default output handling when no pattern matches.
///
/// Categories are auto-detected from command strings using [`detect_category`].
/// When a large output has no matching pattern, the category determines the fallback
/// behavior:
///
/// - **Status**: Test runners, builds, linters → quiet success (empty summary)
/// - **Content**: File viewers and diffs → always passthrough (never index)
/// - **Data**: Listing and querying commands → index for recall
/// - **Unknown**: Anything else → passthrough (safe default)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandCategory {
    /// test runners, linters, builds — agent wants pass/fail (quiet success)
    Status,
    /// git show, git diff, cat — agent wants the actual output (passthrough)
    Content,
    /// git log, gh api, ls — structured/queryable data (index for recall)
    Data,
    /// anything else — defaults to passthrough (safe)
    Unknown,
}

/// Command output classification result.
///
/// Represents the outcome of analyzing a command's exit code and output.
/// Each variant determines how the output should be presented to the AI agent.
///
/// # Variants
///
/// - **Failure**: Command exited non-zero. Contains filtered error output.
/// - **Passthrough**: Command succeeded with small output. Contains verbatim output.
/// - **Success**: Command succeeded with large output and pattern match. Contains compressed summary.
/// - **Large**: Command succeeded with large output and no pattern. Output is indexed for recall.
///
/// The classification is produced by the [`classify`] function.
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

/// Derive a short label from a command string.
///
/// Extracts the first word of the command (typically the binary name),
/// stripping any path prefix. For example:
/// - "cargo test" → "cargo"
/// - "/usr/bin/python script.py" → "python"
/// - "gh issue list" → "gh"
///
/// # Arguments
///
/// * `command` - The command string
///
/// # Returns
///
/// A short label derived from the command.
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

/// Detect command category from command string.
///
/// Analyzes the command string to determine its category, which is used as
/// a fallback when no pattern matches for large outputs.
///
/// # Categories
///
/// - **Status**: Test runners, builds, linters → quiet success
/// - **Content**: File viewers and diffs → always passthrough
/// - **Data**: Listing and querying commands → index for recall
/// - **Unknown**: Anything else → passthrough (safe default)
///
/// # Arguments
///
/// * `command` - The command string to analyze
///
/// # Returns
///
/// A [`CommandCategory`] indicating the command's type.
pub fn detect_category(command: &str) -> CommandCategory {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return CommandCategory::Unknown;
    }

    // Extract binary name (strip path prefix)
    let binary = parts[0].rsplit('/').next().unwrap_or(parts[0]);
    let subcommand = parts.get(1).copied().unwrap_or("");

    match binary {
        // Status: test runners, build systems, linters
        "cargo" => match subcommand {
            "test" | "clippy" | "build" | "fmt" | "check" => CommandCategory::Status,
            _ => CommandCategory::Unknown,
        },
        "pytest" | "jest" | "vitest" | "go" | "npm" | "yarn" | "pnpm" | "bun" | "eslint"
        | "ruff" | "mypy" | "tsc" | "make" | "rubocop" => CommandCategory::Status,

        // Content: file viewers and diffs
        "git" => match subcommand {
            "show" | "diff" => CommandCategory::Content,
            "log" | "status" | "branch" | "tag" => CommandCategory::Data,
            _ => CommandCategory::Unknown,
        },
        "cat" | "bat" | "less" => CommandCategory::Content,

        // Data: listing and querying
        "gh" => CommandCategory::Data,
        "ls" | "find" | "grep" | "rg" => CommandCategory::Data,

        _ => CommandCategory::Unknown,
    }
}

/// Classify command output using patterns and automatic category detection.
///
/// This is the main entry point for output classification. It analyzes the command's
/// exit code, output size, and applies pattern matching to determine the appropriate
/// presentation strategy.
///
/// # Algorithm
///
/// 1. **Failure path** (exit_code ≠ 0): Apply failure pattern or smart truncation
/// 2. **Small success** (output ≤ 4KB): Pass through verbatim
/// 3. **Pattern match**: Extract summary using success pattern
/// 4. **Category fallback**: Use command category to determine behavior
///
/// # Arguments
///
/// * `output` - The command's exit code, stdout, and stderr
/// * `command` - The command string (used for pattern matching and category detection)
/// * `patterns` - List of patterns to try (typically [`pattern::builtins`] + user patterns)
///
/// # Returns
///
/// A [`Classification`] indicating how to present the output.
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

    // Large, no pattern match — use category to determine behavior
    let category = detect_category(command);
    match category {
        CommandCategory::Status => {
            // Status commands: quiet success (empty summary)
            Classification::Success {
                label: lbl,
                summary: String::new(),
            }
        }
        CommandCategory::Content | CommandCategory::Unknown => {
            // Content and Unknown: always passthrough (never index)
            Classification::Passthrough { output: merged }
        }
        CommandCategory::Data => {
            // Data: index for recall
            let size = merged.len();
            Classification::Large {
                label: lbl,
                output: merged,
                size,
            }
        }
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
}
