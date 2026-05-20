use regex::Regex;
use std::sync::LazyLock;

// Public API re-exports
pub use self::builtins::builtin_patterns;
pub use self::toml::{FailureSection, PatternFile, load_user_patterns, parse_pattern_str};

// Internal re-export for learn module
#[doc(hidden)]
pub use self::toml::validate_pattern_regexes;

/// Get a reference to the static built-in patterns.
pub fn builtins() -> &'static [Pattern] {
    &BUILTINS
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A pattern for matching and extracting information from command output.
///
/// Patterns define how to compress command output using regex matching.
/// When a command matches the `command_match` regex, the pattern's
/// success or failure logic is applied to extract compressed output.
pub struct Pattern {
    /// Regex that matches the command line (e.g., `r"cargo test"`).
    pub command_match: Regex,

    /// Optional pattern for extracting a summary from successful command output.
    pub success: Option<SuccessPattern>,

    /// Optional strategy for filtering failed command output.
    pub failure: Option<FailurePattern>,
}

/// Pattern for extracting a summary from successful command output.
///
/// Uses a strategy-based approach to handle different extraction methods:
/// - Regex with template formatting (legacy)
/// - Tail/head line extraction
/// - Grep filtering
pub struct SuccessPattern {
    /// Strategy for extracting success output.
    pub strategy: SuccessStrategy,
}

/// Strategy for filtering failed command output.
///
/// When a command exits with a non-zero status, the failure strategy
/// extracts relevant error information (e.g., tail N lines, head N lines,
/// grep for error keywords, or extract text between delimiters).
pub struct FailurePattern {
    /// The strategy to apply for extracting error information.
    pub strategy: FailureStrategy,
}

/// Strategy for extracting error information from failed command output.
///
/// Each variant defines a different approach to identifying and extracting
/// the most relevant error information from command output.
pub enum FailureStrategy {
    /// Keep the last N lines of output (tail).
    Tail {
        /// Number of lines to keep from the end.
        lines: usize,
    },

    /// Keep the first N lines of output (head).
    Head {
        /// Number of lines to keep from the start.
        lines: usize,
    },

    /// Filter lines matching a regex pattern.
    Grep {
        /// Regex pattern to match error lines.
        pattern: Regex,
    },

    /// Extract text between two delimiter strings.
    Between {
        /// Starting delimiter string.
        start: String,

        /// Ending delimiter string.
        end: String,
    },
}

/// Strategy for extracting success output.
///
/// Mirrors failure strategies but for successful command output.
/// Used when a command succeeds with large output and a pattern matches.
pub enum SuccessStrategy {
    /// Legacy format: regex with named capture groups + summary template.
    Regex {
        /// Regex with named capture groups for extracting values.
        pattern: Regex,
        /// Template string with `{name}` placeholders for summary formatting.
        summary: String,
    },

    /// Keep the last N lines of output (tail).
    Tail {
        /// Number of lines to keep from the end.
        lines: usize,
    },

    /// Keep the first N lines of output (head).
    Head {
        /// Number of lines to keep from the start.
        lines: usize,
    },

    /// Filter lines matching a regex pattern.
    Grep {
        /// Regex pattern to match lines.
        pattern: Regex,
    },
}

// ---------------------------------------------------------------------------
// Matching & extraction
// ---------------------------------------------------------------------------

/// Extract lines matching a regex pattern.
///
/// Shared helper for both success and failure grep strategies.
fn extract_grep(output: &str, pattern: &Regex) -> String {
    let mut result = String::new();
    let mut first = true;
    for line in output.lines() {
        if pattern.is_match(line) {
            if !first {
                result.push('\n');
            }
            result.push_str(line);
            first = false;
        }
    }
    result
}

/// Extract the last N lines from output.
fn extract_tail(output: &str, lines: usize) -> Option<String> {
    let all: Vec<&str> = output.lines().collect();
    let start = all.len().saturating_sub(lines);
    if start >= all.len() {
        None
    } else {
        Some(all[start..].join("\n"))
    }
}

/// Extract the first N lines from output.
fn extract_head(output: &str, lines: usize) -> Option<String> {
    let all: Vec<&str> = output.lines().collect();
    let end = lines.min(all.len());
    if end == 0 {
        None
    } else {
        Some(all[..end].join("\n"))
    }
}

/// Find the first pattern whose `command_match` matches `command`.
pub fn find_matching<'a>(command: &str, patterns: &'a [Pattern]) -> Option<&'a Pattern> {
    patterns.iter().find(|p| p.command_match.is_match(command))
}

/// Like `find_matching` but works with a slice of references.
///
/// Useful when you have a slice of pattern references rather than values.
pub fn find_matching_ref<'a>(command: &str, patterns: &[&'a Pattern]) -> Option<&'a Pattern> {
    patterns
        .iter()
        .find(|p| p.command_match.is_match(command))
        .copied()
}

/// Apply a success pattern to output, returning the formatted summary if it matches.
pub fn extract_summary(pat: &SuccessPattern, output: &str) -> Option<String> {
    match &pat.strategy {
        SuccessStrategy::Regex { pattern, summary } => {
            let caps = pattern.captures(output)?;
            let mut result = String::with_capacity(summary.len() + output.len());
            let mut i = 0;
            while i < summary.len() {
                if let Some(j) = summary[i..].find('{') {
                    result.push_str(&summary[i..i + j]);
                    i += j + 1;
                    if let Some(k) = summary[i..].find('}') {
                        let placeholder = &summary[i..i + k];
                        if let Some(m) = caps.name(placeholder) {
                            result.push_str(m.as_str());
                        } else {
                            result.push('{');
                            result.push_str(placeholder);
                            result.push('}');
                        }
                        i += k + 1;
                    } else {
                        result.push('{');
                        result.push_str(&summary[i..]);
                        break;
                    }
                } else {
                    result.push_str(&summary[i..]);
                    break;
                }
            }
            Some(result)
        }
        SuccessStrategy::Tail { lines } => extract_tail(output, *lines),
        SuccessStrategy::Head { lines } => extract_head(output, *lines),
        SuccessStrategy::Grep { pattern } => {
            let result = extract_grep(output, pattern);
            if result.is_empty() {
                None
            } else {
                Some(result)
            }
        }
    }
}

/// Apply a failure strategy to extract actionable output.
pub fn extract_failure(pat: &FailurePattern, output: &str) -> String {
    match &pat.strategy {
        FailureStrategy::Tail { lines } => {
            let all: Vec<&str> = output.lines().collect();
            let start = all.len().saturating_sub(*lines);
            all[start..].join("\n")
        }
        FailureStrategy::Head { lines } => {
            let all: Vec<&str> = output.lines().collect();
            all[..*lines.min(&all.len())].join("\n")
        }
        FailureStrategy::Grep { pattern, .. } => extract_grep(output, pattern),
        FailureStrategy::Between { start, end } => {
            let mut capturing = false;
            let mut lines = Vec::new();
            for line in output.lines() {
                if !capturing && line.contains(start.as_str()) {
                    capturing = true;
                }
                if capturing {
                    lines.push(line);
                    if line.contains(end.as_str()) {
                        break;
                    }
                }
            }
            lines.join("\n")
        }
    }
}

// Submodules
mod builtins;
mod toml;

// Static builtin patterns
static BUILTINS: LazyLock<Vec<Pattern>> = LazyLock::new(builtin_patterns);
