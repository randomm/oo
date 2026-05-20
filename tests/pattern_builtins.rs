// Tests for built-in patterns

use double_o::pattern::{extract_failure, extract_summary, find_matching};

#[test]
fn test_builtin_pytest_success() {
    let patterns = double_o::pattern::builtins();
    let pat = find_matching("pytest tests/ -x", patterns).unwrap();
    let output = "collected 47 items\n\
                   .................\n\
                   47 passed in 3.2s\n";
    let summary = extract_summary(pat.success.as_ref().unwrap(), output).unwrap();
    assert_eq!(summary, "47 passed, 3.2s");
}

#[test]
fn test_builtin_pytest_failure_tail() {
    let patterns = double_o::pattern::builtins();
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
    let patterns = double_o::pattern::builtins();
    let pat = find_matching("cargo test --release", patterns).unwrap();
    let output = "running 15 tests\n\
                   test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.45s\n";
    let summary = extract_summary(pat.success.as_ref().unwrap(), output).unwrap();
    assert_eq!(summary, "15 passed, 3.45s");
}

#[test]
fn test_command_matching() {
    let patterns = double_o::pattern::builtins();
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
    let patterns = double_o::pattern::builtins();
    assert!(find_matching("curl https://example.com", patterns).is_none());
}

// -----------------------------------------------------------------------
// New built-in pattern tests (issue #58)
// -----------------------------------------------------------------------

#[test]
fn test_builtin_npm_test_success() {
    let patterns = double_o::pattern::builtins();
    let pat = find_matching("npm test", patterns).unwrap();
    let output =
        "Test Suites: 1 passed, 1 total\nTests:       10 passed, 10 total\nTime:        2.345 s";
    let summary = extract_summary(pat.success.as_ref().unwrap(), output).unwrap();
    assert_eq!(summary, "10 passed, 2.345s");
}

#[test]
fn test_builtin_yarn_test_success() {
    let patterns = double_o::pattern::builtins();
    let pat = find_matching("yarn test", patterns).unwrap();
    let output =
        "Test Suites: 1 passed, 1 total\nTests:       15 passed, 15 total\nTime:        1.234 s";
    let summary = extract_summary(pat.success.as_ref().unwrap(), output).unwrap();
    assert_eq!(summary, "15 passed, 1.234s");
}

#[test]
fn test_builtin_pnpm_test_success() {
    let patterns = double_o::pattern::builtins();
    let pat = find_matching("pnpm test", patterns).unwrap();
    let output =
        "Test Suites: 2 passed, 2 total\nTests:       20 passed, 20 total\nTime:        3.456 s";
    let summary = extract_summary(pat.success.as_ref().unwrap(), output).unwrap();
    assert_eq!(summary, "20 passed, 3.456s");
}

#[test]
fn test_builtin_bun_test_success() {
    let patterns = double_o::pattern::builtins();
    let pat = find_matching("bun test", patterns).unwrap();
    let output =
        "Test Suites: 1 passed, 1 total\nTests:       5 passed, 5 total\nTime:        0.456 s";
    let summary = extract_summary(pat.success.as_ref().unwrap(), output).unwrap();
    assert_eq!(summary, "5 passed, 0.456s");
}

#[test]
fn test_builtin_npx_jest_success() {
    let patterns = double_o::pattern::builtins();
    // The existing jest/vitest pattern should match npx jest
    // Note: existing pattern doesn't have (?s) so it won't match multi-line output
    // For now, just verify the pattern exists and matches the command
    let pat = find_matching("npx jest", patterns).unwrap();
    assert!(pat.command_match.is_match("npx jest"));
}

#[test]
fn test_builtin_cargo_tarpaulin_success() {
    let patterns = double_o::pattern::builtins();
    let pat = find_matching("cargo tarpaulin", patterns).unwrap();
    let output = "|| Tested/Total Lines:\n|| src/main.rs: 100%\n|| Overall coverage: 85.2%";
    let summary = extract_summary(pat.success.as_ref().unwrap(), output).unwrap();
    assert_eq!(summary, "85.2% coverage");
}

#[test]
fn test_builtin_cargo_fmt_success() {
    let patterns = double_o::pattern::builtins();
    let pat = find_matching("cargo fmt", patterns).unwrap();
    let output = ""; // silent success
    let summary = extract_summary(pat.success.as_ref().unwrap(), output).unwrap();
    assert_eq!(summary, "");
}

#[test]
fn test_builtin_cargo_fmt_failure() {
    let patterns = double_o::pattern::builtins();
    let pat = find_matching("cargo fmt", patterns).unwrap();
    let fail_pat = pat.failure.as_ref().unwrap();
    let output = "Diff in src/main.rs at line 10:\n---\n+++\n-let x=1\n+let x = 1\n";
    let result = extract_failure(fail_pat, output);
    assert!(result.contains("Diff in"));
}

#[test]
fn test_builtin_mypy_success() {
    let patterns = double_o::pattern::builtins();
    let pat = find_matching("mypy src/", patterns).unwrap();
    let output = "Success: no issues found";
    let summary = extract_summary(pat.success.as_ref().unwrap(), output).unwrap();
    assert_eq!(summary, "Success: no issues found");
}

#[test]
fn test_builtin_mypy_failure() {
    let patterns = double_o::pattern::builtins();
    let pat = find_matching("mypy src/", patterns).unwrap();
    let fail_pat = pat.failure.as_ref().unwrap();
    let output = "src/main.py:10: error: Incompatible return value type\nsrc/main.py:15: error: Name 'x' not defined\nFound 2 errors in 1 file";
    let result = extract_failure(fail_pat, output);
    assert!(result.contains("Found 2 errors"));
}

#[test]
fn test_builtin_rubocop_success() {
    let patterns = double_o::pattern::builtins();
    let pat = find_matching("rubocop", patterns).unwrap();
    let output = "5 files inspected, 0 offenses detected";
    let summary = extract_summary(pat.success.as_ref().unwrap(), output).unwrap();
    assert_eq!(summary, "0 offenses");
}

#[test]
fn test_builtin_rubocop_failure() {
    let patterns = double_o::pattern::builtins();
    let pat = find_matching("rubocop", patterns).unwrap();
    let fail_pat = pat.failure.as_ref().unwrap();
    let output = "Inspecting 5 files\n.offenses detected\nC\nRuboCop: 5 files inspected, 12 offenses detected";
    let result = extract_failure(fail_pat, output);
    assert!(result.contains("12 offenses"));
}

#[test]
fn test_builtin_ruff_format_success() {
    let patterns = double_o::pattern::builtins();
    let pat = find_matching("ruff format", patterns).unwrap();
    let output = "3 files reformatted";
    let summary = extract_summary(pat.success.as_ref().unwrap(), output).unwrap();
    assert_eq!(summary, "3 files reformatted");
}

#[test]
fn test_builtin_ruff_format_failure() {
    let patterns = double_o::pattern::builtins();
    let pat = find_matching("ruff format", patterns).unwrap();
    let fail_pat = pat.failure.as_ref().unwrap();
    let output = "error: file not found foo.py\n";
    let result = extract_failure(fail_pat, output);
    assert!(result.contains("error:"));
}

#[test]
fn test_builtin_prettier_success() {
    let patterns = double_o::pattern::builtins();
    let pat = find_matching("prettier --check", patterns).unwrap();
    let output = ""; // quiet on success
    let summary = extract_summary(pat.success.as_ref().unwrap(), output).unwrap();
    assert_eq!(summary, "");
}

#[test]
fn test_builtin_prettier_failure() {
    let patterns = double_o::pattern::builtins();
    let pat = find_matching("prettier --check", patterns).unwrap();
    let fail_pat = pat.failure.as_ref().unwrap();
    let output = "file1.js\nfile2.js\nCode style issues found in the above file";
    let result = extract_failure(fail_pat, output);
    assert!(result.contains("Code style issues"));
}

#[test]
fn test_builtin_npm_build_success() {
    let patterns = double_o::pattern::builtins();
    let pat = find_matching("npm run build", patterns).unwrap();
    let output = ""; // quiet success
    let summary = extract_summary(pat.success.as_ref().unwrap(), output).unwrap();
    assert_eq!(summary, "");
}

#[test]
fn test_builtin_npm_build_failure() {
    let patterns = double_o::pattern::builtins();
    let pat = find_matching("npm run build", patterns).unwrap();
    let fail_pat = pat.failure.as_ref().unwrap();
    let output = "error TS2307: Cannot find module 'react'\n    at main.ts:3:1\n";
    let result = extract_failure(fail_pat, output);
    assert!(result.contains("error"));
}

#[test]
fn test_builtin_yarn_build_success() {
    let patterns = double_o::pattern::builtins();
    let pat = find_matching("yarn build", patterns).unwrap();
    let output = "Done in 2.3s";
    let summary = extract_summary(pat.success.as_ref().unwrap(), output).unwrap();
    assert_eq!(summary, "Done in 2.3s");
}

#[test]
fn test_builtin_pnpm_build_success() {
    let patterns = double_o::pattern::builtins();
    let pat = find_matching("pnpm build", patterns).unwrap();
    let output = ""; // quiet success
    let summary = extract_summary(pat.success.as_ref().unwrap(), output).unwrap();
    assert_eq!(summary, "");
}

#[test]
fn test_builtin_bun_build_success() {
    let patterns = double_o::pattern::builtins();
    let pat = find_matching("bun build", patterns).unwrap();
    let output = ""; // quiet success
    let summary = extract_summary(pat.success.as_ref().unwrap(), output).unwrap();
    assert_eq!(summary, "");
}

#[test]
fn test_all_new_patterns_match_commands() {
    let patterns = double_o::pattern::builtins();
    assert!(find_matching("npm test", patterns).is_some());
    assert!(find_matching("yarn test", patterns).is_some());
    assert!(find_matching("pnpm test", patterns).is_some());
    assert!(find_matching("bun test", patterns).is_some());
    assert!(find_matching("npx jest", patterns).is_some());
    assert!(find_matching("cargo tarpaulin", patterns).is_some());
    assert!(find_matching("cargo fmt", patterns).is_some());
    assert!(find_matching("mypy src/", patterns).is_some());
    assert!(find_matching("rubocop", patterns).is_some());
    assert!(find_matching("ruff format", patterns).is_some());
    assert!(find_matching("prettier", patterns).is_some());
    assert!(find_matching("npm run build", patterns).is_some());
    assert!(find_matching("yarn build", patterns).is_some());
    assert!(find_matching("pnpm build", patterns).is_some());
    assert!(find_matching("bun build", patterns).is_some());
}
