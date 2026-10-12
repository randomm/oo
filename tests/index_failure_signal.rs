//! Index-failure signal for the Large (Data) and Bounded (Content/Unknown)
//! tiers (issue #198). When the store cannot be opened, the agent must be told
//! the full output is NOT recoverable, and the Bounded slice must not advertise
//! `oo recall`. Success-path wording must stay unchanged.

use assert_cmd::Command;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

fn oo() -> Command {
    Command::cargo_bin("oo").unwrap()
}

/// Temp bin dir holding an executable `name` with `script_body`; returns the
/// PATH with that dir prepended.
fn make_fake_bin(name: &str, script_body: &str) -> (TempDir, String) {
    let dir = TempDir::new().unwrap();
    let bin_dir = dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let bin = bin_dir.join(name);
    std::fs::write(&bin, script_body).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    (dir, path)
}

/// Well over 4 KiB of output across 300 lines.
const BIG_SCRIPT: &str = "#!/bin/sh\nfor i in $(seq 1 300); do echo \"line $i padding padding padding padding padding padding padding\"; done\nexit 0\n";

/// A path under a regular file: `store::open` fails, so indexing fails.
fn unopenable_data_dir() -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let bogus = dir.path().join("not-a-dir");
    std::fs::write(&bogus, b"x").unwrap();
    (dir, bogus)
}

#[test]
fn large_tier_index_failure_says_not_indexed_and_warns() {
    let (_bin, path) = make_fake_bin("ls", BIG_SCRIPT);
    let (_data, bogus) = unopenable_data_dir();

    let assert = oo()
        .args(["ls"])
        .env("PATH", &path)
        .env("OO_DATA_DIR", &bogus)
        .assert()
        .success();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    assert!(
        stdout.contains("\u{25CF} ls (") && stdout.contains("NOT indexed, recall unavailable"),
        "Large failure must print the NOT-indexed header; got: {stdout:?}"
    );
    assert!(
        stderr.contains("could not index output for recall"),
        "Large failure must warn on stderr; got: {stderr:?}"
    );
    assert!(
        !stdout.contains("use `oo recall` to query") && !stdout.contains("(indexed "),
        "must not advertise recall when indexing failed; got: {stdout:?}"
    );
    assert!(
        stdout.contains("lines truncated"),
        "smart_truncate view must still be printed; got: {stdout:?}"
    );
}

#[test]
fn large_tier_index_success_keeps_indexed_line() {
    let (_bin, path) = make_fake_bin("ls", BIG_SCRIPT);
    let data = TempDir::new().unwrap();

    let assert = oo()
        .args(["ls"])
        .env("PATH", &path)
        .env("OO_DATA_DIR", data.path())
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();

    assert!(
        stdout.contains("\u{25CF} ls (indexed ") && stdout.contains("use `oo recall` to query)"),
        "indexed Large output must keep the indexed line; got: {stdout:?}"
    );
    assert!(!stdout.contains("NOT indexed"), "got: {stdout:?}");
}

#[test]
fn bounded_tier_index_failure_marker_omits_recall_hint() {
    let (_bin, path) = make_fake_bin("zunknowntool", BIG_SCRIPT);
    let (_data, bogus) = unopenable_data_dir();

    let assert = oo()
        .args(["zunknowntool"])
        .env("PATH", &path)
        .env("OO_DATA_DIR", &bogus)
        .assert()
        .success();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    assert!(
        stdout.contains("NOT indexed, recall unavailable"),
        "Bounded failure must print the NOT-indexed header; got: {stdout:?}"
    );
    assert!(
        stderr.contains("could not index output for recall"),
        "Bounded failure must warn on stderr; got: {stderr:?}"
    );
    assert!(
        stdout.contains("bytes truncated] ...\n"),
        "unindexed slice marker must omit the recall hint; got: {stdout:?}"
    );
    assert!(
        !stdout.contains("oo recall"),
        "must not advertise recall anywhere when indexing failed; got: {stdout:?}"
    );
}

#[test]
fn bounded_tier_index_success_keeps_recall_marker() {
    let (_bin, path) = make_fake_bin("zunknowntool", BIG_SCRIPT);
    let data = TempDir::new().unwrap();

    let assert = oo()
        .args(["zunknowntool"])
        .env("PATH", &path)
        .env("OO_DATA_DIR", data.path())
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();

    assert!(
        stdout.contains("use `oo recall` to query"),
        "indexed Bounded output must keep the recall hint; got: {stdout:?}"
    );
    assert!(
        stdout.contains("bytes truncated \u{2192} use `oo recall` to query] ...\n"),
        "indexed slice marker must be unchanged; got: {stdout:?}"
    );
    assert!(!stdout.contains("NOT indexed"), "got: {stdout:?}");
}
