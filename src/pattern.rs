use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;

use crate::error::Error;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

pub struct Pattern {
    pub command_match: Regex,
    pub success: Option<SuccessPattern>,
    pub failure: Option<FailurePattern>,
}

pub struct SuccessPattern {
    pub pattern: Regex,
    pub summary: String, // template with {name} placeholders
}

pub struct FailurePattern {
    pub strategy: FailureStrategy,
}

pub enum FailureStrategy {
    Tail { lines: usize },
    Head { lines: usize },
    Grep { pattern: Regex },
    Between { start: String, end: String },
}

// ---------------------------------------------------------------------------
// Matching & extraction
// ---------------------------------------------------------------------------

/// Find the first pattern whose `command_match` matches `command`.
pub fn find_matching<'a>(command: &str, patterns: &'a [Pattern]) -> Option<&'a Pattern> {
    patterns.iter().find(|p| p.command_match.is_match(command))
}

/// Like `find_matching` but works with a slice of references.
pub fn find_matching_ref<'a>(command: &str, patterns: &[&'a Pattern]) -> Option<&'a Pattern> {
    patterns
        .iter()
        .find(|p| p.command_match.is_match(command))
        .copied()
}

/// Apply a success pattern to output, returning the formatted summary if it matches.
pub fn extract_summary(pat: &SuccessPattern, output: &str) -> Option<String> {
    let caps = pat.pattern.captures(output)?;
    let mut summary = pat.summary.clone();
    for name in pat.pattern.capture_names().flatten() {
        if let Some(m) = caps.name(name) {
            summary = summary.replace(&format!("{{{name}}}"), m.as_str());
        }
    }
    Some(summary)
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
            let end = (*lines).min(all.len());
            all[..end].join("\n")
        }
        FailureStrategy::Grep { pattern } => output
            .lines()
            .filter(|l| pattern.is_match(l))
            .collect::<Vec<_>>()
            .join("\n"),
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

// ---------------------------------------------------------------------------
// Built-in patterns
// ---------------------------------------------------------------------------

static BUILTINS: LazyLock<Vec<Pattern>> = LazyLock::new(builtin_patterns);

pub fn builtins() -> &'static [Pattern] {
    &BUILTINS
}

fn builtin_patterns() -> Vec<Pattern> {
    vec![
        // pytest
        Pattern {
            command_match: Regex::new(r"(?:^|\b)pytest\b").unwrap(),
            success: Some(SuccessPattern {
                pattern: Regex::new(r"(?P<passed>\d+) passed.*in (?P<time>[\d.]+)s").unwrap(),
                summary: "{passed} passed, {time}s".into(),
            }),
            failure: Some(FailurePattern {
                strategy: FailureStrategy::Tail { lines: 30 },
            }),
        },
        // cargo test
        Pattern {
            command_match: Regex::new(r"\bcargo\s+test\b").unwrap(),
            success: Some(SuccessPattern {
                pattern: Regex::new(
                    r"test result: ok\. (?P<passed>\d+) passed; (?P<failed>\d+) failed.*finished in (?P<time>[\d.]+)s",
                )
                .unwrap(),
                summary: "{passed} passed, {time}s".into(),
            }),
            failure: Some(FailurePattern {
                strategy: FailureStrategy::Tail { lines: 40 },
            }),
        },
        // go test
        Pattern {
            command_match: Regex::new(r"\bgo\s+test\b").unwrap(),
            success: Some(SuccessPattern {
                pattern: Regex::new(r"ok\s+\S+\s+(?P<time>[\d.]+)s").unwrap(),
                summary: "ok ({time}s)".into(),
            }),
            failure: Some(FailurePattern {
                strategy: FailureStrategy::Tail { lines: 30 },
            }),
        },
        // jest / vitest
        Pattern {
            command_match: Regex::new(r"\b(?:jest|vitest|npx\s+(?:jest|vitest))\b").unwrap(),
            success: Some(SuccessPattern {
                pattern: Regex::new(
                    r"Tests:\s+(?P<passed>\d+) passed.*Time:\s+(?P<time>[\d.]+)\s*s",
                )
                .unwrap(),
                summary: "{passed} passed, {time}s".into(),
            }),
            failure: Some(FailurePattern {
                strategy: FailureStrategy::Tail { lines: 30 },
            }),
        },
        // ruff
        Pattern {
            command_match: Regex::new(r"\bruff\s+check\b").unwrap(),
            success: Some(SuccessPattern {
                pattern: Regex::new(r"All checks passed").unwrap(),
                summary: String::new(), // empty = quiet success
            }),
            failure: None, // show all violations
        },
        // eslint
        Pattern {
            command_match: Regex::new(r"\beslint\b").unwrap(),
            success: Some(SuccessPattern {
                pattern: Regex::new(r"(?s).*").unwrap(), // always matches
                summary: String::new(),
            }),
            failure: None,
        },
        // cargo build
        Pattern {
            command_match: Regex::new(r"\bcargo\s+build\b").unwrap(),
            success: Some(SuccessPattern {
                pattern: Regex::new(r"(?s).*").unwrap(),
                summary: String::new(),
            }),
            failure: Some(FailurePattern {
                strategy: FailureStrategy::Head { lines: 20 },
            }),
        },
        // go build
        Pattern {
            command_match: Regex::new(r"\bgo\s+build\b").unwrap(),
            success: Some(SuccessPattern {
                pattern: Regex::new(r"(?s).*").unwrap(),
                summary: String::new(),
            }),
            failure: Some(FailurePattern {
                strategy: FailureStrategy::Head { lines: 20 },
            }),
        },
        // tsc
        Pattern {
            command_match: Regex::new(r"\btsc\b").unwrap(),
            success: Some(SuccessPattern {
                pattern: Regex::new(r"(?s).*").unwrap(),
                summary: String::new(),
            }),
            failure: Some(FailurePattern {
                strategy: FailureStrategy::Head { lines: 20 },
            }),
        },
        // cargo clippy
        Pattern {
            command_match: Regex::new(r"\bcargo\s+clippy\b").unwrap(),
            success: Some(SuccessPattern {
                pattern: Regex::new(r"(?s).*").unwrap(),
                summary: String::new(),
            }),
            failure: None,
        },
    ]
}

// ---------------------------------------------------------------------------
// User patterns (TOML on disk)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct PatternFile {
    command_match: String,
    success: Option<SuccessSection>,
    failure: Option<FailureSection>,
}

#[derive(Deserialize)]
struct SuccessSection {
    pattern: String,
    summary: String,
}

#[derive(Deserialize)]
pub(crate) struct FailureSection {
    pub(crate) strategy: Option<String>,
    pub(crate) lines: Option<usize>,
    #[serde(rename = "grep")]
    pub(crate) grep_pattern: Option<String>,
    pub(crate) start: Option<String>,
    pub(crate) end: Option<String>,
}

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

fn parse_pattern_str(content: &str) -> Result<Pattern, Error> {
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_pytest_success() {
        let patterns = builtins();
        let pat = find_matching("pytest tests/ -x", patterns).unwrap();
        let output = "collected 47 items\n\
                       .................\n\
                       47 passed in 3.2s\n";
        let summary = extract_summary(pat.success.as_ref().unwrap(), output).unwrap();
        assert_eq!(summary, "47 passed, 3.2s");
    }

    #[test]
    fn test_builtin_pytest_failure_tail() {
        let patterns = builtins();
        let pat = find_matching("pytest -x", patterns).unwrap();
        let fail_pat = pat.failure.as_ref().unwrap();
        let lines: String = (0..50).map(|i| format!("line {i}\n")).collect();
        let result = extract_failure(fail_pat, &lines);
        // tail 30 lines from 50 → lines 20..49
        assert!(result.contains("line 20"));
        assert!(result.contains("line 49"));
        assert!(!result.contains("line 0\n"));
    }

    #[test]
    fn test_builtin_cargo_test_success() {
        let patterns = builtins();
        let pat = find_matching("cargo test --release", patterns).unwrap();
        let output = "running 15 tests\n\
                       test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.45s\n";
        let summary = extract_summary(pat.success.as_ref().unwrap(), output).unwrap();
        assert_eq!(summary, "15 passed, 3.45s");
    }

    #[test]
    fn test_command_matching() {
        let patterns = builtins();
        assert!(find_matching("pytest tests/", patterns).is_some());
        assert!(find_matching("cargo test", patterns).is_some());
        assert!(find_matching("cargo build", patterns).is_some());
        assert!(find_matching("go test ./...", patterns).is_some());
        assert!(find_matching("ruff check src/", patterns).is_some());
        assert!(find_matching("eslint .", patterns).is_some());
        assert!(find_matching("tsc --noEmit", patterns).is_some());
        assert!(find_matching("cargo clippy", patterns).is_some());
    }

    #[test]
    fn test_no_match_unknown_command() {
        let patterns = builtins();
        assert!(find_matching("curl https://example.com", patterns).is_none());
    }

    #[test]
    fn test_summary_template_formatting() {
        let pat = SuccessPattern {
            pattern: Regex::new(r"(?P<a>\d+) things, (?P<b>\d+) items").unwrap(),
            summary: "{a} things and {b} items".into(),
        };
        let result = extract_summary(&pat, "found 5 things, 3 items here").unwrap();
        assert_eq!(result, "5 things and 3 items");
    }

    #[test]
    fn test_failure_strategy_head() {
        let strat = FailurePattern {
            strategy: FailureStrategy::Head { lines: 3 },
        };
        let output = "line1\nline2\nline3\nline4\nline5\n";
        let result = extract_failure(&strat, output);
        assert_eq!(result, "line1\nline2\nline3");
    }

    #[test]
    fn test_failure_strategy_grep() {
        let strat = FailurePattern {
            strategy: FailureStrategy::Grep {
                pattern: Regex::new(r"ERROR").unwrap(),
            },
        };
        let output = "INFO ok\nERROR bad\nINFO fine\nERROR worse\n";
        let result = extract_failure(&strat, output);
        assert_eq!(result, "ERROR bad\nERROR worse");
    }

    #[test]
    fn test_failure_strategy_between() {
        let strat = FailurePattern {
            strategy: FailureStrategy::Between {
                start: "FAILURES".into(),
                end: "summary".into(),
            },
        };
        let output = "stuff\nFAILURES\nerror 1\nerror 2\nshort test summary\nmore\n";
        let result = extract_failure(&strat, output);
        assert_eq!(result, "FAILURES\nerror 1\nerror 2\nshort test summary");
    }

    #[test]
    fn test_load_pattern_from_toml() {
        let toml = r#"
command_match = "^myapp test"

[success]
pattern = '(?P<count>\d+) tests passed'
summary = "{count} tests passed"

[failure]
strategy = "tail"
lines = 20
"#;
        let pat = parse_pattern_str(toml).unwrap();
        assert!(pat.command_match.is_match("myapp test --verbose"));
        let summary = extract_summary(pat.success.as_ref().unwrap(), "42 tests passed").unwrap();
        assert_eq!(summary, "42 tests passed");
    }

    #[test]
    fn test_invalid_toml_returns_error() {
        let result = parse_pattern_str("not valid toml {{{");
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_regex_returns_error() {
        let toml = r#"
command_match = "[invalid"
"#;
        let result = parse_pattern_str(toml);
        assert!(result.is_err());
    }

    #[test]
    fn test_user_patterns_override_builtins() {
        let user_pat = parse_pattern_str(
            r#"
command_match = "^pytest"
[success]
pattern = '(?P<n>\d+) ok'
summary = "{n} ok"
"#,
        )
        .unwrap();

        // User patterns should be checked first
        let mut all = vec![user_pat];
        all.extend(builtin_patterns());

        let pat = find_matching("pytest -x", &all).unwrap();
        let summary = extract_summary(pat.success.as_ref().unwrap(), "10 ok").unwrap();
        assert_eq!(summary, "10 ok");
    }
}
