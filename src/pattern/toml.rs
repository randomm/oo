use regex::Regex;
use regex::RegexBuilder;
use serde::Deserialize;
use std::path::Path;

use super::{FailurePattern, FailureStrategy, Pattern, SuccessPattern, SuccessStrategy};
use crate::error::Error;

// ---------------------------------------------------------------------------
// Regex validation limits
// ---------------------------------------------------------------------------

/// Maximum allowed length for user-provided regex patterns.
///
/// This limit prevents overly complex regex patterns that could cause
/// performance issues or unexpected ReDOS attacks.
const MAX_REGEX_LENGTH: usize = 500;

/// Size limit for regex compilation (in bytes).
///
/// Prevents pathological regex patterns from consuming excessive memory.
/// Set to 1 MB - high enough for reasonable patterns but limits pathological cases.
const REGEX_SIZE_LIMIT: usize = 1024 * 1024; // 1 MB

/// Validate and compile a user-provided regex string with safety limits.
///
/// This function checks that the regex string is not overly long and compiles
/// it with a reasonable size limit to prevent resource exhaustion issues.
///
/// # Arguments
///
/// * `pattern` - The regex pattern string to compile
///
/// # Errors
///
/// Returns `Error::Pattern` if the regex is too long or fails to compile.
fn validate_and_compile_regex(pattern: &str) -> Result<Regex, Error> {
    if pattern.len() > MAX_REGEX_LENGTH {
        return Err(Error::Pattern(format!(
            "regex too long ({} > {} chars)",
            pattern.len(),
            MAX_REGEX_LENGTH
        )));
    }

    RegexBuilder::new(pattern)
        .size_limit(REGEX_SIZE_LIMIT)
        .build()
        .map_err(|e| Error::Pattern(format!("regex compilation failed: {e}")))
}

// ---------------------------------------------------------------------------
// TOML deserialization types
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// TOML deserialization types
// ---------------------------------------------------------------------------

/// TOML representation of a pattern file.
///
/// This struct deserializes from user-defined TOML pattern files
/// loaded from `~/.config/oo/patterns/`. Each file defines a single pattern
/// with optional success and failure configurations.
#[derive(Deserialize)]
pub struct PatternFile {
    /// Regex that matches the command line.
    pub command_match: String,

    /// Optional success pattern configuration.
    pub success: Option<SuccessSection>,

    /// Optional failure pattern configuration.
    pub failure: Option<FailureSection>,
}

/// TOML configuration for success output extraction.
///
/// Supports both legacy pattern+summary format and new strategy-based format.
#[derive(Deserialize)]
pub struct SuccessSection {
    /// Strategy name: "regex" (legacy), "tail", "head", or "grep".
    #[serde(default)]
    pub(crate) strategy: Option<String>,

    /// Regex pattern with named capture groups (for legacy format or grep strategy).
    #[serde(rename = "pattern")]
    pub(crate) success_pattern: Option<String>,

    /// Summary template with {name} placeholders (for legacy format).
    pub(crate) summary: Option<String>,

    /// Number of lines (for tail/head strategies).
    pub(crate) lines: Option<usize>,

    /// Grep pattern (for grep strategy).
    #[serde(rename = "grep")]
    pub(crate) grep_pattern: Option<String>,
}

/// TOML configuration for failure output filtering.
///
/// Defines how to extract relevant error information from failed command output.
/// Multiple strategies are supported: tail, head, grep, and between.
#[derive(Deserialize)]
pub struct FailureSection {
    /// Strategy name: "tail", "head", "grep", or "between".
    pub(crate) strategy: Option<String>,

    /// Number of lines (for tail/head strategies).
    pub(crate) lines: Option<usize>,

    /// Grep pattern (for grep strategy).
    #[serde(rename = "grep")]
    pub(crate) grep_pattern: Option<String>,

    /// Start delimiter (for between strategy).
    pub(crate) start: Option<String>,

    /// End delimiter (for between strategy).
    pub(crate) end: Option<String>,
}

// ---------------------------------------------------------------------------
// User patterns (TOML on disk)
// ---------------------------------------------------------------------------

/// Load user-defined patterns from a directory of TOML files.
///
/// Invalid files are silently skipped.
pub fn load_user_patterns(dir: &Path) -> Vec<Pattern> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut patterns = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "toml") {
            if let Ok(p) = load_pattern_file(&path) {
                patterns.push(p);
            }
        }
    }
    patterns
}

fn load_pattern_file(path: &Path) -> Result<Pattern, Error> {
    let content =
        std::fs::read_to_string(path).map_err(|e| Error::Pattern(format!("{path:?}: {e}")))?;
    parse_pattern_str(&content).map_err(|e| {
        // Add file path context to any parse errors
        if let Error::Pattern(msg) = e {
            Error::Pattern(format!("{path:?}: {msg}"))
        } else {
            e
        }
    })
}

/// Parse a pattern definition from TOML string content.
///
/// Deserializes a TOML pattern definition into a `Pattern` struct,
/// validating regex patterns and strategy configurations.
///
/// # Arguments
///
/// * `content` - TOML-formatted pattern definition
///
/// # Returns
///
/// A `Pattern` struct if parsing and validation succeed, or an `Error`
/// if TOML is malformed, regex is invalid, or strategy configuration is incomplete.
///
/// # Errors
///
/// Returns `Error::Pattern` for:
/// - TOML parsing failures
/// - Invalid regular expressions
/// - Missing required fields (e.g., grep pattern for grep strategy)
/// - Unknown strategy names
/// - Regex patterns exceeding maximum length (500 characters)
///
/// # Examples
///
/// ```
/// use double_o::pattern::parse_pattern_str;
///
/// let toml = r#"
/// command_match = "myapp test"
///
/// [success]
/// pattern = "(?P<passed>\\d+) passed"
/// summary = "{passed} tests passed"
/// "#;
/// let pattern = parse_pattern_str(toml).unwrap();
/// ```
pub fn parse_pattern_str(content: &str) -> Result<Pattern, Error> {
    let pf: PatternFile =
        toml::from_str(content).map_err(|e| Error::Pattern(format!("TOML parse: {e}")))?;

    // Validate and compile command_match regex with safety limits
    let command_match = validate_and_compile_regex(&pf.command_match)?;

    let success = pf
        .success
        .map(|s| -> Result<SuccessPattern, Error> {
            // Determine strategy: explicit strategy field, or default to "regex" for legacy format
            let strategy = match s.strategy.as_deref().unwrap_or("regex") {
                "tail" => SuccessStrategy::Tail {
                    lines: s.lines.unwrap_or(30),
                },
                "head" => SuccessStrategy::Head {
                    lines: s.lines.unwrap_or(20),
                },
                "grep" => {
                    let pat = s.grep_pattern.ok_or_else(|| {
                        Error::Pattern("grep strategy requires 'grep' field".into())
                    })?;
                    let pattern = validate_and_compile_regex(&pat)?;
                    SuccessStrategy::Grep { pattern }
                }
                "regex" => {
                    // Legacy format: pattern + summary
                    let pattern = s.success_pattern.ok_or_else(|| {
                        Error::Pattern("regex strategy requires 'pattern' field".into())
                    })?;
                    let summary = s.summary.ok_or_else(|| {
                        Error::Pattern("regex strategy requires 'summary' field".into())
                    })?;
                    let regex = validate_and_compile_regex(&pattern)?;
                    SuccessStrategy::Regex {
                        pattern: regex,
                        summary,
                    }
                }
                other => {
                    return Err(Error::Pattern(format!("unknown success strategy: {other}")));
                }
            };
            Ok(SuccessPattern { strategy })
        })
        .transpose()?;

    let failure = pf
        .failure
        .map(|f| -> Result<FailurePattern, Error> {
            let strategy = match f.strategy.as_deref().unwrap_or("tail") {
                "tail" => FailureStrategy::Tail {
                    lines: f.lines.unwrap_or(30),
                },
                "head" => FailureStrategy::Head {
                    lines: f.lines.unwrap_or(20),
                },
                "grep" => {
                    let pat = f.grep_pattern.ok_or_else(|| {
                        Error::Pattern("grep strategy requires 'grep' field".into())
                    })?;
                    let pattern = validate_and_compile_regex(&pat)?;
                    FailureStrategy::Grep { pattern }
                }
                "between" => {
                    let start = f.start.ok_or_else(|| {
                        Error::Pattern("between strategy requires 'start'".into())
                    })?;
                    let end = f
                        .end
                        .ok_or_else(|| Error::Pattern("between strategy requires 'end'".into()))?;
                    FailureStrategy::Between { start, end }
                }
                other => {
                    return Err(Error::Pattern(format!("unknown strategy: {other}")));
                }
            };
            Ok(FailurePattern { strategy })
        })
        .transpose()?;

    Ok(Pattern {
        command_match,
        success,
        failure,
    })
}
