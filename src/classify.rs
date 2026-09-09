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
    ///
    /// # Fields
    ///
    /// * `label` - Short label derived from the command (e.g., "cargo", "pytest").
    /// * `output` - Filtered error output, truncated if large.
    Failure {
        /// Short label derived from the command (e.g., "cargo", "pytest").
        label: String,
        /// Filtered error output, truncated if large.
        output: String,
    },

    /// Exit 0, output ≤ threshold. Verbatim.
    ///
    /// # Fields
    ///
    /// * `output` - The full command output (merged stdout and stderr).
    Passthrough {
        /// The full command output (merged stdout and stderr).
        output: String,
    },

    /// Exit 0, output > threshold, pattern matched with summary.
    ///
    /// # Fields
    ///
    /// * `label` - Short label derived from the command (e.g., "cargo", "pytest").
    /// * `summary` - Compressed summary extracted using the pattern's template.
    Success {
        /// Short label derived from the command (e.g., "cargo", "pytest").
        label: String,
        /// Compressed summary extracted using the pattern's template.
        summary: String,
    },

    /// Exit 0, output > threshold, no pattern. Content needs indexing.
    ///
    /// # Fields
    ///
    /// * `label` - Short label derived from the command (e.g., "git", "gh").
    /// * `output` - The full command output to be indexed for recall.
    /// * `size` - Size of the output in bytes.
    Large {
        /// Short label derived from the command (e.g., "git", "gh").
        label: String,
        /// The full command output to be indexed for recall.
        output: String,
        /// Size of the output in bytes.
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
///
/// # Examples
///
/// ```
/// use double_o::{classify, CommandOutput};
/// use double_o::pattern::builtins;
///
/// let output = CommandOutput {
///     stdout: b"test result: ok. 5 passed; 0 failed; finished in 0.3s".to_vec(),
///     stderr: Vec::new(),
///     exit_code: 0,
/// };
/// let patterns = builtins();
/// let result = classify(&output, "cargo test", patterns);
/// ```
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
// Tests live in `classify_tests.rs` (sibling module, see `#[path]` below) —
// this file holds only production code to stay under the 500-line cap.
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "classify_tests.rs"]
mod tests;
