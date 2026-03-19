use regex::Regex;
use serde::Deserialize;
use std::path::Path;

use super::{FailurePattern, FailureStrategy, Pattern, SuccessPattern};
use crate::error::Error;

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

#[derive(Deserialize)]
pub struct SuccessSection {
    /// Regex pattern with named capture groups.
    pub pattern: String,

    /// Summary template with {name} placeholders.
    pub summary: String,
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
/// - Unknown strategy namespub fn parse_pattern_str(content: &str) -> Result<Pattern, Error> {
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
    parse_pattern_str(&content)
}

pub fn parse_pattern_str(content: &str) -> Result<Pattern, Error> {
    let pf: PatternFile =
        toml::from_str(content).map_err(|e| Error::Pattern(format!("TOML parse: {e}")))?;

    let command_match =
        Regex::new(&pf.command_match).map_err(|e| Error::Pattern(format!("regex: {e}")))?;

    let success = pf
        .success
        .map(|s| -> Result<SuccessPattern, Error> {
            let pattern =
                Regex::new(&s.pattern).map_err(|e| Error::Pattern(format!("regex: {e}")))?;
            Ok(SuccessPattern {
                pattern,
                summary: s.summary,
            })
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
                    let pattern =
                        Regex::new(&pat).map_err(|e| Error::Pattern(format!("regex: {e}")))?;
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
