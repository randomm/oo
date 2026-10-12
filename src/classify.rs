//! Command output classification and intelligent truncation.
//!
//! This module is the core of `oo`'s context-efficient output handling. It analyzes
//! command results and produces one of five [`Classification`] outcomes:
//!
//! - **Failure**: Non-zero exit codes → filtered error output (full output indexed for recall when the slice does not match it)
//! - **Passthrough**: Small successful outputs (≤ 4 KiB, 4096 bytes) → verbatim
//! - **Success**: Large successful outputs with pattern match → compressed summary
//! - **Bounded**: Large Content/Unknown output → full output indexed, byte-bounded display
//! - **Large**: Large Data output without pattern → indexed for recall
//!
//! The [`classify`] function combines pattern matching with automatic command category
//! detection to make intelligent decisions about how to present output.

#[cfg(test)]
use crate::classify_bounded::cut_boundaries;
pub use crate::classify_bounded::{bounded_truncate, bounded_truncate_indexed};
use crate::exec::CommandOutput;
use crate::pattern::{self, Pattern};

/// 4 KiB (4096 bytes). Successful output of at most this many bytes passes
/// through verbatim; larger output is classified.
pub const SMALL_THRESHOLD: usize = 4096;

/// Minimum savings (bytes) for the `[saved …]` indicator-line suffix.
///
/// Deliberately independent of [`SMALL_THRESHOLD`]/[`DISPLAY_CAP`] — the same
/// 4096 value is a coincidence: this is a noise floor on a savings *delta*,
/// whereas `SMALL_THRESHOLD` is the passthrough byte budget.
///
/// Below this floor the suffix is suppressed entirely: a `[saved 12 B]` tag
/// on every command is noise and would itself waste context. The floor also
/// subsumes non-positive deltas (summary nearly as long as the input)
/// and sub-KiB values that `humansize` would render as `0 B` / `996 B`,
/// so no separate rule is needed for those cases.
pub const MIN_SAVINGS: usize = 4096;

/// Total byte budget for the display slice of a bounded (Content/Unknown) output.
///
/// Defined as `SMALL_THRESHOLD` so the invariant "we never display more bytes
/// than the passthrough budget" is compiler-enforced rather than maintained
/// by convention. Split 60 % head / 40 % tail, mirroring [`smart_truncate`]'s
/// ratio.
pub const DISPLAY_CAP: usize = SMALL_THRESHOLD;

/// Output up to this many lines is returned unchanged (head/tail joins would fuse lines); longer output is capped here.
const MAX_LINES: usize = 120;

/// Command category — determines default output handling when no pattern matches.
///
/// Categories are auto-detected from command strings using [`detect_category`].
/// When a large output has no matching pattern, the category determines the fallback
/// behavior:
///
/// - **Status**: Test runners, builds, linters → quiet success (empty summary)
/// - **Content**: File viewers and diffs → bounded display + indexed full output
/// - **Data**: Listing and querying commands → index for recall
/// - **Unknown**: Anything else → bounded display + indexed full output
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandCategory {
    /// test runners, linters, builds — agent wants pass/fail (quiet success)
    Status,
    /// git show, git diff, cat — agent wants the actual output (bounded + indexed)
    Content,
    /// git log, gh api, ls — structured/queryable data (index for recall)
    Data,
    /// anything else — bounded display + indexed (safe default)
    Unknown,
}

/// Command output classification result.
///
/// Represents the outcome of analyzing a command's exit code and output.
/// Each variant determines how the output should be presented to the AI agent.
///
/// # Variants
///
/// - **Failure**: Command exited non-zero. Contains filtered error output and the full merged output.
/// - **Passthrough**: Command succeeded with small output. Contains verbatim output.
/// - **Success**: Command succeeded with large output and pattern match. Contains compressed summary.
/// - **Bounded**: Content/Unknown command with large output. Full output is indexed; `display` is a byte-bounded head+tail slice.
/// - **Large**: Data command with large output and no pattern. Output is indexed for recall.
///
/// The classification is produced by the [`classify`] function.
#[derive(Debug)]
pub enum Classification {
    /// Exit ≠ 0. Filtered failure output.
    ///
    /// # Fields
    ///
    /// * `label` - Short label derived from the command (e.g., "cargo", "pytest").
    /// * `output` - Filtered error output shown to the user, truncated if large.
    /// * `full_output` - Complete merged stdout+stderr, indexed for recall when
    ///   it differs from `output`.
    Failure {
        /// Short label derived from the command (e.g., "cargo", "pytest").
        label: String,
        /// Filtered error output shown to the user, truncated if large.
        output: String,
        /// Complete merged stdout+stderr, indexed for recall when it differs from `output`.
        full_output: String,
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

    /// Exit 0, output > threshold, no pattern, Content or Unknown category.
    ///
    /// The full output is indexed for recall; `display` is a byte-bounded
    /// head+tail slice (≤ [`DISPLAY_CAP`] bytes + truncation marker).
    ///
    /// # Fields
    ///
    /// * `label` - Short label derived from the command (e.g., "git", "gh").
    /// * `output` - The full command output to be indexed for recall.
    /// * `display` - Byte-bounded head+tail slice for display (≤ [`DISPLAY_CAP`] + marker).
    /// * `size` - Size of the full output in bytes.
    Bounded {
        /// Short label derived from the command (e.g., "git", "gh").
        label: String,
        /// The full command output to be indexed for recall.
        output: String,
        /// Byte-bounded head+tail slice for display.
        display: String,
        /// Size of the full output in bytes.
        size: usize,
    },

    /// Exit 0, output > threshold, no pattern. Data category — index for recall.
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
/// Extracts the binary name, skipping any leading `sudo`/`env` prefix and
/// stripping any path prefix. For example:
/// - "cargo test" → "cargo"
/// - "/usr/bin/python script.py" → "python"
/// - "gh issue list" → "gh"
/// - "sudo cargo test" → "cargo"
/// - "env FOO=bar cargo test" → "cargo"
///
/// If stripping the prefix leaves no token, the original first token is
/// returned (e.g., "sudo" → "sudo", "env" → "env").
///
/// # Arguments
///
/// * `command` - The command string
///
/// # Returns
///
/// A short label derived from the command.
pub fn label(command: &str) -> String {
    let parts: Vec<&str> = command.split_whitespace().collect();
    let Some(first) = parts.first() else {
        return "command".to_string();
    };
    // `binary_index` is the single source of truth for the sudo/env skip
    // rule. When stripping leaves no token (bare "sudo"/"env"), it returns
    // `None` and we degrade to the original first token (path-stripped).
    let name = match binary_index(&parts) {
        Some(i) => parts[i].rsplit('/').next().unwrap_or(parts[i]),
        None => first.rsplit('/').next().unwrap_or(first),
    };
    name.to_string()
}

/// Position of the binary in a split argv after stripping a leading
/// `sudo`/`env` prefix — the single source of truth for that skip rule.
///
/// Applies only at position 0, and only for exactly `sudo` (after path
/// stripping) or `env`. For `env`, consecutive leading `KEY=VALUE` assignment
/// tokens are skipped until the real binary is reached. Strips once — does
/// NOT loop, so `sudo sudo cargo test` does not strip the second `sudo`.
///
/// Returns `Some(0)` when there is no prefix (the binary is the first token
/// as-is), the index of the stripped binary otherwise, and `None` when
/// stripping leaves no token (e.g. bare "sudo" or "env").
fn binary_index(parts: &[&str]) -> Option<usize> {
    let first = parts.first()?;
    let base = first.rsplit('/').next().unwrap_or(first);
    match base {
        "sudo" => parts.get(1).map(|_| 1),
        "env" => {
            let mut i = 1;
            while i < parts.len() && is_var_value(parts[i]) {
                i += 1;
            }
            (i < parts.len()).then_some(i)
        }
        _ => Some(0),
    }
}

/// Returns true when a token looks like a shell `KEY=VALUE` assignment.
///
/// The leading-dash guard keeps `env` flags out of the skip loop: a flag
/// like `--split-string=x` contains `=`, and treating it as a `KEY=VALUE`
/// assignment would silently skip it — misclassifying the command as Status
/// and suppressing its output to a quiet `✓` line with no recall hint. A
/// flag stops the loop instead, so the binary becomes the flag token and the
/// category falls back to Unknown (output preserved).
fn is_var_value(token: &str) -> bool {
    !token.starts_with('-') && token.contains('=')
}

/// Detect command category from command string.
///
/// Analyzes the command string to determine its category, which is used as
/// a fallback when no pattern matches for large outputs.
///
/// # Categories
///
/// - **Status**: Test runners, builds, linters → quiet success
/// - **Content**: File viewers and diffs → bounded display + indexed full output
/// - **Data**: Listing and querying commands → index for recall
/// - **Unknown**: Anything else → bounded display + indexed full output
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

    // The binary index is the single source of truth for the sudo/env skip
    // rule; the binary name and the subcommand (the next token, "" if none)
    // are both derived from it. Stripping that leaves no token (bare
    // "sudo"/"env") degrades to the original token 0 — path-stripped, which
    // is a no-op for bare "sudo"/"env" — and the subcommand to the raw
    // token 1 ("" if none). `binary_index` is computed once per call; the
    // `nextest` arm below reuses the same index for its `i + 2` lookup.
    let binary_idx = binary_index(&parts);
    let (binary, subcommand) = match binary_idx {
        Some(i) => (
            parts[i].rsplit('/').next().unwrap_or(parts[i]),
            parts.get(i + 1).copied().unwrap_or(""),
        ),
        None => (
            parts[0].rsplit('/').next().unwrap_or(parts[0]),
            parts.get(1).copied().unwrap_or(""),
        ),
    };

    match binary {
        // Status: test runners, build systems, linters
        "cargo" => match subcommand {
            "test" | "clippy" | "build" | "fmt" | "check" => CommandCategory::Status,
            // `cargo nextest run` → Status (lookup fix only — not a general
            // argv parser; see issue #149). Use the KNOWN subcommand position
            // (token after the stripped binary) rather than scanning the whole
            // argv for "nextest": a later token that happens to equal
            // "nextest" (e.g. `env FOO=nextest cargo nextest run`) would
            // otherwise land on the wrong token. Everything after "run" is
            // irrelevant — any trailing argv (package filters, flags) is
            // accepted.
            "nextest" => {
                // `run` sits at a fixed offset after the stripped binary:
                // binary index + 2 (binary, "nextest", "run"). Reusing the
                // index computed above keeps the position single-sourced.
                let token_after_nextest = binary_idx.and_then(|i| parts.get(i + 2));
                match token_after_nextest.copied() {
                    Some("run") => CommandCategory::Status,
                    _ => CommandCategory::Unknown,
                }
            }
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
/// 2. **Small success** (output ≤ 4 KiB): Pass through verbatim
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
            full_output: merged,
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
            // Content and Unknown: bounded display, full output indexed for recall
            let size = merged.len();
            let display = bounded_truncate(&merged);
            Classification::Bounded {
                label: lbl,
                output: merged,
                display,
                size,
            }
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

    if total <= MAX_LINES {
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

/// Tests live in `classify_tests.rs` (sibling module, see `#[path]` below) —
/// this file holds only production code to stay under the 500-line cap.
#[cfg(test)]
#[path = "classify_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "classify_trunc_tests.rs"]
mod trunc_tests;
