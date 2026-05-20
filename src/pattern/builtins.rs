use regex::Regex;

use super::{FailurePattern, FailureStrategy, Pattern, SuccessPattern, SuccessStrategy};

/// Built-in pattern definitions for common commands.
pub fn builtin_patterns() -> Vec<Pattern> {
    vec![
        // pytest
        Pattern {
            command_match: Regex::new(r"(?:^|\b)pytest\b")
                .expect("valid regex: pytest command_match"),
            success: Some(SuccessPattern {
                strategy: SuccessStrategy::Regex {
                    pattern: Regex::new(r"(?P<passed>\d+) passed.*in (?P<time>[\d.]+)s")
                        .expect("valid regex: pytest success pattern"),
                    summary: "{passed} passed, {time}s".into(),
                },
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
                strategy: SuccessStrategy::Regex {
                    pattern: Regex::new(
                        r"test result: ok\. (?P<passed>\d+) passed; (?P<failed>\d+) failed.*finished in (?P<time>[\d.]+)s",
                    )
                    .expect("valid regex: cargo test success pattern"),
                    summary: "{passed} passed, {time}s".into(),
                },
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
                strategy: SuccessStrategy::Regex {
                    pattern: Regex::new(r"ok\s+\S+\s+(?P<time>[\d.]+)s")
                        .expect("valid regex: go test success pattern"),
                    summary: "ok ({time}s)".into(),
                },
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
                strategy: SuccessStrategy::Regex {
                    pattern: Regex::new(
                        r"Tests:\s+(?P<passed>\d+) passed.*Time:\s+(?P<time>[\d.]+)\s*s",
                    )
                    .expect("valid regex: jest/vitest success pattern"),
                    summary: "{passed} passed, {time}s".into(),
                },
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
                strategy: SuccessStrategy::Regex {
                    pattern: Regex::new(r"All checks passed")
                        .expect("valid regex: ruff check success pattern"),
                    summary: String::new(), // empty = quiet success
                },
            }),
            failure: None, // show all violations
        },
        // eslint
        Pattern {
            command_match: Regex::new(r"\beslint\b")
                .expect("valid regex: eslint command_match"),
            success: Some(SuccessPattern {
                strategy: SuccessStrategy::Regex {
                    pattern: Regex::new(r"(?s).*")
                        .expect("valid regex: eslint success pattern (always matches)"),
                    summary: String::new(),
                },
            }),
            failure: None,
        },
        // cargo build
        Pattern {
            command_match: Regex::new(r"\bcargo\s+build\b")
                .expect("valid regex: cargo build command_match"),
            success: Some(SuccessPattern {
                strategy: SuccessStrategy::Regex {
                    pattern: Regex::new(r"(?s).*")
                        .expect("valid regex: cargo build success pattern (always matches)"),
                    summary: String::new(),
                },
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
                strategy: SuccessStrategy::Regex {
                    pattern: Regex::new(r"(?s).*")
                        .expect("valid regex: go build success pattern (always matches)"),
                    summary: String::new(),
                },
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
                strategy: SuccessStrategy::Regex {
                    pattern: Regex::new(r"(?s).*")
                        .expect("valid regex: tsc success pattern (always matches)"),
                    summary: String::new(),
                },
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
                strategy: SuccessStrategy::Regex {
                    pattern: Regex::new(r"(?s).*")
                        .expect("valid regex: cargo clippy success pattern (always matches)"),
                    summary: String::new(),
                },
            }),
            failure: None,
        },
        // npm/yarn/pnpm/bun test (shared pattern)
        nodejs_test_pattern(r"npm\s+test"),
        nodejs_test_pattern(r"yarn\s+test"),
        nodejs_test_pattern(r"pnpm\s+test"),
        nodejs_test_pattern(r"bun\s+test"),
        // cargo tarpaulin (coverage)
        Pattern {
            command_match: Regex::new(r"\bcargo\s+tarpaulin\b")
                .expect("valid regex: cargo tarpaulin command_match"),
            success: Some(SuccessPattern {
                strategy: SuccessStrategy::Regex {
                    pattern: Regex::new(r"Overall coverage: (?P<cov>[\d.]+)%")
                        .expect("valid regex: cargo tarpaulin success pattern"),
                    summary: "{cov}% coverage".into(),
                },
            }),
            failure: Some(FailurePattern {
                strategy: FailureStrategy::Tail { lines: 20 },
            }),
        },
        // cargo fmt
        Pattern {
            command_match: Regex::new(r"\bcargo\s+fmt\b")
                .expect("valid regex: cargo fmt command_match"),
            success: Some(SuccessPattern {
                strategy: SuccessStrategy::Regex {
                    pattern: Regex::new(r"(?s).*")
                        .expect("valid regex: cargo fmt success pattern (always matches)"),
                    summary: String::new(),
                },
            }),
            failure: Some(FailurePattern {
                strategy: FailureStrategy::Grep {
                    pattern: Regex::new(r"Diff in").expect("valid regex: cargo fmt failure pattern"),
                },
            }),
        },
        // mypy
        Pattern {
            command_match: Regex::new(r"\bmypy\b")
                .expect("valid regex: mypy command_match"),
            success: Some(SuccessPattern {
                strategy: SuccessStrategy::Regex {
                    pattern: Regex::new(r"Success: no issues found")
                        .expect("valid regex: mypy success pattern"),
                    summary: "Success: no issues found".into(),
                },
            }),
            failure: Some(FailurePattern {
                strategy: FailureStrategy::Head { lines: 20 },
            }),
        },
        // rubocop
        Pattern {
            command_match: Regex::new(r"\brubocop\b")
                .expect("valid regex: rubocop command_match"),
            success: Some(SuccessPattern {
                strategy: SuccessStrategy::Regex {
                    pattern: Regex::new(r"(?P<offenses>\d+) offenses detected")
                        .expect("valid regex: rubocop success pattern"),
                    summary: "{offenses} offenses".into(),
                },
            }),
            failure: Some(FailurePattern {
                strategy: FailureStrategy::Head { lines: 30 },
            }),
        },
        // ruff format
        Pattern {
            command_match: Regex::new(r"\bruff\s+format\b")
                .expect("valid regex: ruff format command_match"),
            success: Some(SuccessPattern {
                strategy: SuccessStrategy::Regex {
                    pattern: Regex::new(r"(?P<count>\d+) files reformatted")
                        .expect("valid regex: ruff format success pattern"),
                    summary: "{count} files reformatted".into(),
                },
            }),
            failure: Some(FailurePattern {
                strategy: FailureStrategy::Head { lines: 20 },
            }),
        },
        // prettier
        Pattern {
            command_match: Regex::new(r"\bprettier\b")
                .expect("valid regex: prettier command_match"),
            success: Some(SuccessPattern {
                strategy: SuccessStrategy::Regex {
                    pattern: Regex::new(r"(?s).*")
                        .expect("valid regex: prettier success pattern (always matches)"),
                    summary: String::new(),
                },
            }),
            failure: Some(FailurePattern {
                strategy: FailureStrategy::Head { lines: 20 },
            }),
        },
        // npm run build
        Pattern {
            command_match: Regex::new(r"\bnpm\s+run\s+build\b")
                .expect("valid regex: npm run build command_match"),
            success: Some(SuccessPattern {
                strategy: SuccessStrategy::Regex {
                    pattern: Regex::new(r"(?s).*")
                        .expect("valid regex: npm run build success pattern (always matches)"),
                    summary: String::new(),
                },
            }),
            failure: Some(FailurePattern {
                strategy: FailureStrategy::Head { lines: 20 },
            }),
        },
        // yarn build
        Pattern {
            command_match: Regex::new(r"\byarn\s+build\b")
                .expect("valid regex: yarn build command_match"),
            success: Some(SuccessPattern {
                strategy: SuccessStrategy::Regex {
                    pattern: Regex::new(r"Done in (?P<time>[\d.]+)s")
                        .expect("valid regex: yarn build success pattern"),
                    summary: "Done in {time}s".into(),
                },
            }),
            failure: Some(FailurePattern {
                strategy: FailureStrategy::Head { lines: 20 },
            }),
        },
        // pnpm build (quiet)
        nodejs_quiet_build_pattern(r"pnpm\s+build"),
        // bun build (quiet)
        nodejs_quiet_build_pattern(r"bun\s+build"),
    ]
}

/// Create a test pattern for Node.js package managers (npm/yarn/pnpm/bun).
///
/// All these tools report test output in the same format:
/// - Success: "Tests: N passed... Time: X.XX s"
/// - Failure: show last 30 lines
fn nodejs_test_pattern(command_match: &str) -> Pattern {
    Pattern {
        command_match: Regex::new(command_match)
            .unwrap_or_else(|e| panic!("valid regex: {command_match} command_match: {e}")),
        success: Some(SuccessPattern {
            strategy: SuccessStrategy::Regex {
                pattern: Regex::new(
                    r"(?s)Tests:\s+(?P<passed>\d+) passed.*Time:\s+(?P<time>[\d.]+)\s*s",
                )
                .expect("valid regex: nodejs test success pattern"),
                summary: "{passed} passed, {time}s".into(),
            },
        }),
        failure: Some(FailurePattern {
            strategy: FailureStrategy::Tail { lines: 30 },
        }),
    }
}

/// Create a quiet build pattern for Node.js package managers (pnpm/bun).
///
/// These tools succeed with verbose output that should be hidden.
/// On failure, show the first 20 lines where errors typically appear.
fn nodejs_quiet_build_pattern(command_match: &str) -> Pattern {
    Pattern {
        command_match: Regex::new(command_match)
            .unwrap_or_else(|e| panic!("valid regex: {command_match} command_match: {e}")),
        success: Some(SuccessPattern {
            strategy: SuccessStrategy::Regex {
                pattern: Regex::new(r"(?s).*")
                    .expect("valid regex: nodejs quiet build success pattern (always matches)"),
                summary: String::new(),
            },
        }),
        failure: Some(FailurePattern {
            strategy: FailureStrategy::Head { lines: 20 },
        }),
    }
}
