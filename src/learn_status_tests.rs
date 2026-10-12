//! Unit tests for the learn-status writers and the foreground reader/clearer.

use super::*;

// ---------------------------------------------------------------------------
// Status file write / read / delete (Part 2)
// ---------------------------------------------------------------------------

#[test]
fn test_write_learn_status_creates_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let status_path = dir.path().join("learn-status.log");
    write_learn_status(&status_path, "git-status", &status_path).unwrap();
    assert!(status_path.exists(), "status file must be created");
    let content = std::fs::read_to_string(&status_path).unwrap();
    assert!(
        content.contains("git-status"),
        "status file must contain command name"
    );
}

#[test]
fn test_check_and_clear_learn_status_reads_and_deletes() {
    let dir = tempfile::TempDir::new().unwrap();
    let status_path = dir.path().join("learn-status.log");
    std::fs::write(
        &status_path,
        "learned pattern for git-status → /some/path.toml\n",
    )
    .unwrap();
    check_and_clear_learn_status(&status_path);
    assert!(
        !status_path.exists(),
        "status file must be deleted after reading"
    );
}

#[test]
fn test_check_and_clear_learn_status_missing_file_is_no_op() {
    let dir = tempfile::TempDir::new().unwrap();
    let status_path = dir.path().join("nonexistent-status.log");
    // Must not panic when file does not exist
    check_and_clear_learn_status(&status_path);
}

// ---------------------------------------------------------------------------
// check_and_clear_learn_status: failure format
// ---------------------------------------------------------------------------

#[test]
fn test_check_and_clear_learn_status_failure() {
    let dir = tempfile::TempDir::new().unwrap();
    let status_path = dir.path().join("learn-status.log");

    // Write a FAILED entry directly
    write_learn_status_failure(
        &status_path,
        "cargo-test",
        "Anthropic API error: 401 Unauthorized",
    )
    .unwrap();

    // File must exist before check
    assert!(status_path.exists(), "status file must exist before check");

    // check_and_clear must not panic and must delete the file
    check_and_clear_learn_status(&status_path);

    assert!(
        !status_path.exists(),
        "status file must be deleted after check_and_clear"
    );
}

#[test]
fn test_write_learn_status_failure_format() {
    let dir = tempfile::TempDir::new().unwrap();
    let status_path = dir.path().join("learn-status.log");

    write_learn_status_failure(
        &status_path,
        "npm-run",
        "Set ANTHROPIC_API_KEY to use oo learn",
    )
    .unwrap();

    let content = std::fs::read_to_string(&status_path).unwrap();
    assert!(
        content.starts_with("FAILED npm-run:"),
        "failure line must start with 'FAILED <cmd>:'; got: {content}"
    );
    assert!(
        content.contains("ANTHROPIC_API_KEY"),
        "failure line must contain the error message; got: {content}"
    );
}

#[test]
fn test_write_learn_status_failure_multiline_error() {
    // Error with newlines should be truncated to the first line only,
    // so check_and_clear_learn_status can correctly parse the status file.
    let dir = tempfile::TempDir::new().unwrap();
    let status_path = dir.path().join("learn-status.log");

    write_learn_status_failure(
        &status_path,
        "git-log",
        "API error\ndetailed body\nmore lines",
    )
    .unwrap();

    let content = std::fs::read_to_string(&status_path).unwrap();
    // Status file must contain exactly one line (with trailing newline)
    assert_eq!(
        content.lines().count(),
        1,
        "multiline error must be truncated to a single line; got: {content:?}"
    );
    // That line must contain the first line of the error message only
    assert!(
        content.contains("API error"),
        "first line of error must be present; got: {content:?}"
    );
    assert!(
        !content.contains("detailed body"),
        "subsequent error lines must not appear in status file; got: {content:?}"
    );
}

// ---------------------------------------------------------------------------
// write_learn_status: append mode (Part 2 extended)
// ---------------------------------------------------------------------------

#[test]
fn test_write_learn_status_appends_multiple_lines() {
    let dir = tempfile::TempDir::new().unwrap();
    let status_path = dir.path().join("learn-status.log");
    write_learn_status(&status_path, "git", &status_path).unwrap();
    write_learn_status(&status_path, "cargo", &status_path).unwrap();
    let content = std::fs::read_to_string(&status_path).unwrap();
    assert!(content.contains("git"), "first entry must be present");
    assert!(content.contains("cargo"), "second entry must be present");
    // Both lines must coexist — not overwritten
    assert_eq!(content.lines().count(), 2, "must have 2 lines");
}

#[cfg(unix)]
#[test]
fn test_write_learn_status_creates_file_mode_0600() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::TempDir::new().unwrap();
    let status_path = dir.path().join("learn-status.log");
    write_learn_status_failure(&status_path, "git", "boom").unwrap();
    let mode = std::fs::metadata(&status_path)
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}
