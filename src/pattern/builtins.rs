use regex::Regex;

use super::{FailurePattern, FailureStrategy, Pattern, SuccessPattern};

/// Built-in pattern definitions for common commands.
pub fn builtin_patterns() -> Vec<Pattern> {
    vec![
        // pytest
        Pattern {
            command_match: Regex::new(r"(?:^|\b)pytest\b")
                .expect("valid regex: pytest command_match"),
            success: Some(SuccessPattern {
                pattern: Regex::new(r"(?P<passed>\d+) passed.*in (?P<time>[\d.]+)s")
                    .expect("valid regex: pytest success pattern"),
                summary: "{passed} passed, {time}s".into(),
            }),
            failure: Some(FailurePattern {
                strategy: FailureStrategy::Tail { lines: 30 },
            }),
        },
        // cargo test
        Pattern {
            command_match: Regex::new(r"\bcargo\s+test\b")
                .expect("valid regex: cargo test command_match"),
            success: Some(SuccessPattern {
                pattern: Regex::new(
                    r"test result: ok\. (?P<passed>\d+) passed; (?P<failed>\d+) failed.*finished in (?P<time>[\d.]+)s",
                )
                .expect("valid regex: cargo test success pattern"),
                summary: "{passed} passed, {time}s".into(),
            }),
            failure: Some(FailurePattern {
                strategy: FailureStrategy::Tail { lines: 40 },
            }),
        },
        // go test
        Pattern {
            command_match: Regex::new(r"\bgo\s+test\b")
                .expect("valid regex: go test command_match"),
            success: Some(SuccessPattern {
                pattern: Regex::new(r"ok\s+\S+\s+(?P<time>[\d.]+)s")
                    .expect("valid regex: go test success pattern"),
                summary: "ok ({time}s)".into(),
            }),
            failure: Some(FailurePattern {
                strategy: FailureStrategy::Tail { lines: 30 },
            }),
        },
        // jest / vitest
        Pattern {
            command_match: Regex::new(r"\b(?:jest|vitest|npx\s+(?:jest|vitest))\b")
                .expect("valid regex: jest/vitest command_match"),
            success: Some(SuccessPattern {
                pattern: Regex::new(
                    r"Tests:\s+(?P<passed>\d+) passed.*Time:\s+(?P<time>[\d.]+)\s*s",
                )
                .expect("valid regex: jest/vitest success pattern"),
                summary: "{passed} passed, {time}s".into(),
            }),
            failure: Some(FailurePattern {
                strategy: FailureStrategy::Tail { lines: 30 },
            }),
        },
        // ruff
        Pattern {
            command_match: Regex::new(r"\bruff\s+check\b")
                .expect("valid regex: ruff check command_match"),
            success: Some(SuccessPattern {
                pattern: Regex::new(r"All checks passed")
                    .expect("valid regex: ruff check success pattern"),
                summary: String::new(), // empty = quiet success
            }),
            failure: None, // show all violations
        },
        // eslint
        Pattern {
            command_match: Regex::new(r"\beslint\b")
                .expect("valid regex: eslint command_match"),
            success: Some(SuccessPattern {
                pattern: Regex::new(r"(?s).*")
                    .expect("valid regex: eslint success pattern (always matches)"),
                summary: String::new(),
            }),
            failure: None,
        },
        // cargo build
        Pattern {
            command_match: Regex::new(r"\bcargo\s+build\b")
                .expect("valid regex: cargo build command_match"),
            success: Some(SuccessPattern {
                pattern: Regex::new(r"(?s).*")
                    .expect("valid regex: cargo build success pattern (always matches)"),
                summary: String::new(),
            }),
            failure: Some(FailurePattern {
                strategy: FailureStrategy::Head { lines: 20 },
            }),
        },
        // go build
        Pattern {
            command_match: Regex::new(r"\bgo\s+build\b")
                .expect("valid regex: go build command_match"),
            success: Some(SuccessPattern {
                pattern: Regex::new(r"(?s).*")
                    .expect("valid regex: go build success pattern (always matches)"),
                summary: String::new(),
            }),
            failure: Some(FailurePattern {
                strategy: FailureStrategy::Head { lines: 20 },
            }),
        },
        // tsc
        Pattern {
            command_match: Regex::new(r"\btsc\b")
                .expect("valid regex: tsc command_match"),
            success: Some(SuccessPattern {
                pattern: Regex::new(r"(?s).*")
                    .expect("valid regex: tsc success pattern (always matches)"),
                summary: String::new(),
            }),
            failure: Some(FailurePattern {
                strategy: FailureStrategy::Head { lines: 20 },
            }),
        },
        // cargo clippy
        Pattern {
            command_match: Regex::new(r"\bcargo\s+clippy\b")
                .expect("valid regex: cargo clippy command_match"),
            success: Some(SuccessPattern {
                pattern: Regex::new(r"(?s).*")
                    .expect("valid regex: cargo clippy success pattern (always matches)"),
                summary: String::new(),
            }),
            failure: None,
        },
    ]
}
