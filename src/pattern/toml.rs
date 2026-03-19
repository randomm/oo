use regex::Regex;
use serde::Deserialize;
use std::path::Path;

use super::{FailurePattern, FailureStrategy, Pattern, SuccessPattern};
use crate::error::Error;

// ---------------------------------------------------------------------------
// TOML deserialization types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct PatternFile {
    pub command_match: String,
    pub success: Option<SuccessSection>,
    pub failure: Option<FailureSection>,
}

#[derive(Deserialize)]
pub struct SuccessSection {
    pub pattern: String,
    pub summary: String,
}

#[derive(Deserialize)]
pub struct FailureSection {
    pub(crate) strategy: Option<String>,
    pub(crate) lines: Option<usize>,
    #[serde(rename = "grep")]
    pub(crate) grep_pattern: Option<String>,
    pub(crate) start: Option<String>,
    pub(crate) end: Option<String>,
}

// ---------------------------------------------------------------------------
// User patterns (TOML on disk)
// ---------------------------------------------------------------------------

/// Load user-defined patterns from a directory of TOML files.
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
