# Testing Guide

This guide explains how to run, write, and understand oo's tests.

## Running Tests

### Run all tests

```bash
cargo test
```

This runs both unit tests (in-module) and integration tests from `tests/`.

### Run a specific test

```bash
cargo test test_echo_passthrough
```

### Run unit tests only

```bash
cargo test --lib
```

### Run integration tests only

```bash
cargo test --test integration
```

### Run tests with output

```bash
cargo test -- --nocapture
```

Useful when debugging test failures.

## Coverage

Test coverage is enforced via `cargo tarpaulin`:

```bash
cargo tarpaulin --fail-under 70
```

**Target**: 80%+ coverage for new code (currently 70% interim while migrating to VCR cassettes for network tests).

Coverage is primarily driven by integration tests, which exercise real CLI invocations through `assert_cmd`.

## Test Organization

### Unit Tests

Unit tests live in the same files as the code they test, using the `#[cfg(test)]` attribute.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() {
        // Test pure logic here
    }
}
```

**Where they're used**:
- [`src/classify.rs`](../src/classify.rs) — classification logic, category detection
- [`src/pattern/mod.rs`](../src/pattern/mod.rs) — pattern matching and extraction
- [`src/pattern/builtins.rs`](../src/pattern/builtins.rs) — built-in pattern definitions
- [`src/pattern/toml.rs`](../src/pattern/toml.rs) — TOML parsing and validation
- [`src/store.rs`](../src/store.rs) — storage backend logic
- [`src/session.rs`](../src/session.rs) — session and project ID detection
- [`src/exec.rs`](../src/exec.rs) — command execution

**What to unit test**:
- Pure functions without external dependencies
- Pattern extraction logic
- Classification rules
- Storage operations (using in-memory or test databases)
- Configuration parsing

### Integration Tests

Integration tests live in `tests/` and test the CLI as a whole.

```rust
use assert_cmd::Command;
use predicates::prelude::*;

fn oo() -> Command {
    Command::cargo_bin("oo").unwrap()
}

#[test]
fn test_echo_passthrough() {
    oo().args(["echo", "hello"])
        .assert()
        .success()
        .stdout("hello\n");
}
```

**Where they're used**:
- [`tests/integration.rs`](../tests/integration.rs) — comprehensive CLI behavior tests

**What to integration test**:
- CLI argument parsing
- Full command execution flow
- Exit code handling
- Output classification (success, failure, passthrough, large)
- Subcommands (`recall`, `forget`, `learn`, `help`, `init`)
- Real-world scenarios (git logs, test runners, etc.)

## Test Standards

### Mandatory Requirements

1. **TDD preferred**: Write tests before implementation
2. **80%+ coverage** for new code (enforced by `cargo tarpaulin`)
3. **Meaningful assertions**: No trivial `assert!(true)` or `assert_eq!(1, 1)`
4. **Real behavior**: Every test must exercise a real code path

### Network-Dependent Tests

Tests that make network calls must be marked `#[ignore]` with an explanation:

```rust
#[test]
#[ignore = "Requires network access - manual verification only"]
fn test_external_api_call() {
    // Network-dependent test
}
```

### Pattern Tests

Every new pattern in `src/pattern/` must have a corresponding test.

See [`src/pattern/builtins.rs`](../src/pattern/builtins.rs) for examples of pattern tests.

## Fixtures

Test fixtures reside in the repository's `tests/fixtures/` directory:

```
tests/fixtures/
├── anthropic_success.json   # Mocked Anthropic API success response
└── anthropic_invalid.json   # Mocked Anthropic API error response
```

**Purpose**: Provide deterministic inputs for testing LLM integration and external API handling without real network calls.

**Usage**: Load fixtures in tests to mock external dependencies:

```rust
use std::fs;
use serde_json::Value;

fn load_fixture(name: &str) -> Value {
    let path = format!("tests/fixtures/{}", name);
    let content = fs::read_to_string(path).unwrap();
    serde_json::from_str(&content).unwrap()
}
```

## Mocking Conventions

### External Dependencies

- **Mockito** for HTTP mocking (when needed)
- **tempfile** for temporary directories and files
- **Predicates** for readable assertions

Example using `tempfile`:

```rust
use tempfile::TempDir;

#[test]
fn test_with_temp_dir() {
    let dir = TempDir::new().unwrap();
    // Use dir.path() for test artifacts
    // Directory is automatically cleaned up on drop
}
```

### assertions

Use the `predicates` crate for readable assertions:

```rust
use predicates::prelude::*;

oo().args(["echo", "hello"])
    .assert()
    .success()
    .stdout(predicate::str::contains("hello"))
    .stdout(predicate::str::starts_with("h"))
    .stdout("hello\n"); // exact match
```

## Adding a New Pattern Test

When adding a new built-in pattern, follow this pattern:

1. Write the test first (TDD)
2. Implement the pattern
3. Run tests to verify

Example — adding a `npm test` pattern:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_npm_test_success_pattern() {
        let output = CommandOutput {
            stdout: b"Test Suites: 1 passed, 1 total\nTests:       10 passed, 10 total\nTime:        2.345s\n".to_vec(),
            stderr: vec![],
            exit_code: 0,
        };

        if let Classification::Success { summary, .. } = classify(&output, "npm test", &BUILTINS) {
            assert!(summary.contains("10 passed"));
        } else {
            panic!("Expected Success classification");
        }
    }

    #[test]
    fn test_npm_test_failure_pattern() {
        let output = CommandOutput {
            stdout: b"Test Suites: 1 failed, 1 total\n".to_vec(),
            stderr: b"FAIL src/test.js\n  expected true to be false\n".to_vec(),
            exit_code: 1,
        };

        if let Classification::Failure { output, .. } = classify(&output, "npm test", &BUILTINS) {
            assert!(output.contains("FAIL") || output.contains("failed"));
        } else {
            panic!("Expected Failure classification");
        }
    }
}
```

Then add the pattern to `src/pattern/builtins.rs`:

```rust
Pattern {
    command_match: Regex::new(r"^npm\s+test\b").unwrap(),
    success: Some(SuccessPattern {
        pattern: Regex::new(r"Tests:\s+(?P<passed>\d+)\s+passed").unwrap(),
        summary: "{passed} passed".into(),
    }),
    failure: Some(FailurePattern {
        strategy: FailureStrategy::Tail { lines: 30 },
    }),
},
```

## Debugging Tests

### Enable logging

```rust
env_logger::init();
```

Add to the top of `main.rs` before tests run.

### Print test output

```bash
cargo test -- --nocapture --show-output
```

### Run a single test with full output

```bash
cargo test test_name -- --exact --nocapture
```

## Common Issues

### "command not found" errors

Make sure the command you're testing exists in the test environment. Cross-platform tests should account for platform differences:

```rust
#[test]
fn test_platform_specific() {
    #[cfg(unix)]
    oo().args(["ls", "-la"]).assert().success();

    #[cfg(windows)]
    oo().args(["dir"]).assert().success();
}
```

### Flaky tests

Tests that depend on timing or external state may be flaky. Avoid these or mark them as ignored if unavoidable.

### Test hangs

If a test hangs, it may be waiting for input or stuck on a blocking operation. Use timeouts in tests that may hang:

```rust
std::thread::spawn(|| {
    // potentially blocking operation
});
std::thread::sleep(std::time::Duration::from_secs(2));
// continue test
```

## Further Reading

- [`AGENTS.md`](../AGENTS.md) — Agent-specific testing standards
- [`src/classify.rs`](../src/classify.rs) — Classification logic tests
- [`tests/integration.rs`](../tests/integration.rs) — Integration test examples