//! Command output classification and intelligent truncation.
//!
//! This module is the core of `oo`'s context-efficient output handling. It analyzes
//! command results and produces one of five [`Classification`] outcomes:
//!
//! - **Failure**: Non-zero exit codes → filtered error output
//! - **Passthrough**: Small successful outputs (<4KB) → verbatim
//! - **Success**: Large successful outputs with pattern match → compressed summary
//! - **Bounded**: Large Content/Unknown output → full output indexed, byte-bounded display
//! - **Large**: Large Data output without pattern → indexed for recall
//!
//! The [`classify`] function combines pattern matching with automatic command category
//! detection to make intelligent decisions about how to present output.

use crate::exec::CommandOutput;
use crate::pattern::{self, Pattern};

/// 4 KB — below this, output passes through verbatim.
pub const SMALL_THRESHOLD: usize = 4096;

/// Total byte budget for the display slice of a bounded (Content/Unknown) output.
///
/// Defined as `SMALL_THRESHOLD` so the invariant "we never display more bytes
/// than the passthrough budget" is compiler-enforced rather than maintained
/// by convention. Split 60 % head / 40 % tail, mirroring [`smart_truncate`]'s
/// ratio.
pub const DISPLAY_CAP: usize = SMALL_THRESHOLD;

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
/// - **Failure**: Command exited non-zero. Contains filtered error output.
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

/// Floor a byte offset to the nearest preceding char boundary.
///
/// Equivalent to `str::floor_char_boundary` (stable since 1.91) but implemented
/// for MSRV 1.85 compatibility.
fn floor_char_boundary(s: &str, idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    let mut i = idx;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Ceil a byte offset to the nearest following char boundary.
///
/// Equivalent to `str::ceil_char_boundary` (stable since 1.91) but implemented
/// for MSRV 1.85 compatibility.
fn ceil_char_boundary(s: &str, idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    let mut i = idx;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Byte-based truncation with UTF-8 char-boundary safety.
///
/// Produces a head+tail slice bounded by [`DISPLAY_CAP`] bytes total, with a
/// single truncation marker line between them. Cuts snap to `\n` boundaries
/// (at most one line of drift) as a nicety, but the cap is enforced on the
/// ASSEMBLED slices: line-snapping may only ever shrink a slice relative to
/// its byte budget, never grow it past it (a single line can be arbitrarily
/// long — minified JS/JSON, base64). If the output has fewer than 2 newlines,
/// cuts fall back to char-boundary-only slicing. Never splits a multi-byte
/// UTF-8 sequence.
///
/// Returns the input unchanged when it is ≤ [`DISPLAY_CAP`] bytes.
pub fn bounded_truncate(output: &str) -> String {
    if output.len() <= DISPLAY_CAP {
        return output.to_string();
    }

    let head_budget = (DISPLAY_CAP as f64 * 0.6) as usize; // 2457
    let tail_budget = DISPLAY_CAP - head_budget; // 1639

    let (head_end, tail_start) = cut_boundaries(output, head_budget, tail_budget);
    // Enforce the cap on the assembled result, not just the budgets: line
    // snapping must never let head + tail grow past the byte budget. Hard-clamp
    // the head down to its budget (floor) and the tail up to its budget (ceil).
    let clamped_head = floor_char_boundary(output, head_budget).min(head_end);
    let clamped_tail =
        ceil_char_boundary(output, output.len().saturating_sub(tail_budget)).max(tail_start);
    debug_assert!(
        clamped_head + (output.len() - clamped_tail) <= DISPLAY_CAP,
        "head+tail slices ({clamped_head} + {} bytes) must not exceed DISPLAY_CAP",
        output.len() - clamped_tail
    );

    let truncated_bytes = output.len() - clamped_head - (output.len() - clamped_tail);
    let marker =
        format!("... [{truncated_bytes} bytes truncated → use `oo recall` to query] ...\n");

    let mut result = String::with_capacity(DISPLAY_CAP + marker.len());
    result.push_str(&output[..clamped_head]);
    result.push_str(&marker);
    result.push_str(&output[clamped_tail..]);
    result
}

/// Compute head/tail cut byte offsets for [`bounded_truncate`].
///
/// Snaps the head cut forward to the next `\n` (≤ one line) and the tail cut
/// backward to the previous `\n` (≤ one line). A single forward O(1)-memory
/// pass finds the only three facts that matter: whether ≥ 2 newlines exist,
/// the first newline at/after `head_budget`, and the last newline strictly
/// before `raw_tail`. When fewer than 2 newlines exist in the entire output,
/// falls back to char-boundary-only cuts.
///
/// NOTE: the snapped positions may EXCEED the byte budgets (a line can be
/// arbitrarily long). [`bounded_truncate`] enforces the cap on the assembled
/// slices — line-snapping is a nicety that may only shrink, never grow.
fn cut_boundaries(output: &str, head_budget: usize, tail_budget: usize) -> (usize, usize) {
    let raw_tail = output.len().saturating_sub(tail_budget);
    let mut newline_count = 0usize;
    let mut first_nl_at_or_after_head: Option<usize> = None;
    let mut last_nl_before_raw_tail: Option<usize> = None;
    for (i, b) in output.as_bytes().iter().enumerate() {
        if *b != b'\n' {
            continue;
        }
        newline_count += 1;
        // Stop once neither fact can change: we already have a newline at/
        // after head_budget, and this newline is no longer < raw_tail.
        if i >= raw_tail && first_nl_at_or_after_head.is_some() {
            break;
        }
        if i >= head_budget && first_nl_at_or_after_head.is_none() {
            first_nl_at_or_after_head = Some(i);
        }
        if i < raw_tail {
            last_nl_before_raw_tail = Some(i);
        }
    }

    // Fewer than 2 newlines: line-snapping has nothing to snap to (there is
    // at most one newline in the entire output, so neither cut can land on
    // the "right" side of a line and still keep its slice near budget), so
    // use char-boundary-only cuts directly — exactly the bounds the clamp in
    // `bounded_truncate` would apply, keeping the display within budget.
    if newline_count < 2 {
        let head = floor_char_boundary(output, head_budget);
        let tail = ceil_char_boundary(output, raw_tail);
        return (head, tail);
    }

    // Snap head cut forward to next \n (at most one line of drift). When no
    // newline falls at/after head_budget, the raw budget itself is used as the
    // fallback and must be snapped to a char boundary before `+1` — the `+1`
    // is only a safe "skip the newline" when the offset actually is a newline.
    let head_end = match first_nl_at_or_after_head {
        Some(pos) => pos + 1, // include the newline in the head slice
        None => floor_char_boundary(output, head_budget),
    };

    // Snap tail cut backward to previous \n (at most one line of drift). Same
    // fallback hazard on the tail side: when no newline falls before raw_tail,
    // the raw budget must be ceiled to a char boundary. (In practice this
    // fallback is also shielded by the overlap guard below, but snapping it
    // keeps the invariant local and symmetric with the head cut.)
    let tail_start = match last_nl_before_raw_tail {
        Some(pos) => pos + 1, // start after the newline
        None => ceil_char_boundary(output, raw_tail),
    };

    // Ensure head doesn't overlap tail: returning `tail_start = output.len()`
    // encodes "no tail slice" — the head covers everything shown (in this
    // branch head_end ≤ output.len(), so the head is non-empty and the tail
    // is empty, which is always a valid display).
    if head_end >= tail_start {
        return (head_end, output.len());
    }

    (head_end, tail_start)
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

/// Tests live in `classify_tests.rs` (sibling module, see `#[path]` below) —
/// this file holds only production code to stay under the 500-line cap.
#[cfg(test)]
#[path = "classify_tests.rs"]
mod tests;
