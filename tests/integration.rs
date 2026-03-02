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

#[test]
fn test_multiword_echo() {
    oo().args(["echo", "hello", "world"])
        .assert()
        .success()
        .stdout("hello world\n");
}

#[test]
fn test_false_failure() {
    oo().args(["false"])
        .assert()
        .failure()
        .stdout(predicate::str::starts_with("\u{2717}")); // ✗
}

#[test]
fn test_exit_code_preserved() {
    oo().args(["sh", "-c", "exit 42"]).assert().code(42);
}

#[test]
fn test_version() {
    oo().arg("version")
        .assert()
        .success()
        .stdout(predicate::str::contains("0.1.0"));
}

#[test]
fn test_no_args_shows_help() {
    oo().assert()
        .success()
        .stdout(predicate::str::contains("Usage"));
}

#[test]
fn test_large_output_gets_indicator() {
    // seq 1 10000 produces ~49KB which is > 4KB threshold
    oo().args(["seq", "1", "10000"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("\u{25CF}")); // ●
}

#[test]
fn test_stderr_included_in_failure() {
    oo().args(["sh", "-c", "echo failure_msg >&2; exit 1"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("failure_msg"));
}

#[test]
fn test_forget_runs() {
    oo().arg("forget")
        .assert()
        .success()
        .stdout(predicate::str::contains("Cleared session data"));
}

#[test]
fn test_help_no_args_shows_usage() {
    oo().arg("help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage:"));
}

#[test]
fn test_help_includes_help_cmd_in_usage() {
    // Verify the help command itself appears in the no-args usage output
    oo().assert()
        .success()
        .stdout(predicate::str::contains("oo help <cmd>"));
}

#[test]
fn test_help_empty_arg() {
    Command::cargo_bin("oo")
        .unwrap()
        .args(&["help", ""])
        .assert()
        .failure();
}
