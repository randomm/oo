use regex::Regex;
use std::sync::LazyLock;

// Public API re-exports
pub use self::builtins::builtin_patterns;
pub use self::toml::{FailureSection, PatternFile, load_user_patterns, parse_pattern_str};

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
/// The `pattern` field contains a regex with named capture groups.
/// The `summary` field is a template string with placeholders like `{name}`
/// that are replaced with captured values.
pub struct SuccessPattern {
    /// Regex with named capture groups for extracting values.
    pub pattern: Regex,

    /// Template string with `{name}` placeholders for summary formatting.
    pub summary: String,
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

// ---------------------------------------------------------------------------
// Matching & extraction
// ---------------------------------------------------------------------------

/// Find the first pattern whose `command_match` matches `command`.
pub fn find_matching<'a>(command: &str, patterns: &'a [Pattern]) -> Option<&'a Pattern> {
    patterns.iter().find(|p| p.command_match.is_match(command))
}

/// Like `find_matching` but works with a slice of references.
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

// Submodules
mod builtins;
mod toml;

// Static builtin patterns
static BUILTINS: LazyLock<Vec<Pattern>> = LazyLock::new(builtin_patterns);

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
