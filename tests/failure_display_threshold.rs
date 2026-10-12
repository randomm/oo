//! Failure-display threshold behaviour (issue #194).
//!
//! A failing unrecognised command whose output is at most 120 lines must be
//! displayed in full and must not be indexed. Above 120 lines the display is
//! head/marker/tail and the full output must be indexed for `oo recall`.

use assert_cmd::Command;
use tempfile::TempDir;

fn oo() -> Command {
    Command::cargo_bin("oo").unwrap()
}

const HIDDEN_MARKER: &str = "oo194hiddenmarker7e2a";

/// Fake executable on PATH so the command is unrecognised but deterministic.
fn make_fake_bin(name: &str, script_body: &str) -> (TempDir, String) {
    use std::os::unix::fs::PermissionsExt;
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

fn isolated_store() -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path().join("oo-data");
    std::fs::create_dir_all(&data_dir).unwrap();
    (dir, data_dir)
}

#[test]
fn test_failure_100_lines_is_displayed_in_full_and_not_indexed() {
    let script = "#!/bin/sh\nfor i in $(seq 1 100); do echo \"line $i\"; done\nexit 1\n";
    let (_bin, path) = make_fake_bin("zfailtool194", script);
    let (_guard, data_dir) = isolated_store();

    let assert = oo()
        .args(["zfailtool194"])
        .env("PATH", &path)
        .env("OO_DATA_DIR", &data_dir)
        .assert()
        .failure();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();

    for i in 1..=100 {
        assert!(
            stdout.lines().any(|l| l == format!("line {i}")),
            "line {i} must be displayed in full; got: {stdout:?}"
        );
    }
    assert!(
        !stdout.contains("lines truncated"),
        "100 lines must not be truncated; got: {stdout:?}"
    );
    assert!(
        !stdout.contains("oo recall"),
        "100 lines must not advertise recall; got: {stdout:?}"
    );
}

#[test]
fn test_failure_200_lines_is_indexed_and_recallable() {
    // Marker at line 100 falls in the truncated middle of a 200-line output.
    let script = format!(
        "#!/bin/sh\nfor i in $(seq 1 200); do if [ $i -eq 100 ]; then echo {HIDDEN_MARKER}; else echo \"line $i\"; fi; done\nexit 1\n"
    );
    let (_bin, path) = make_fake_bin("zfailtool194", &script);
    let (_guard, data_dir) = isolated_store();

    let assert = oo()
        .args(["zfailtool194"])
        .env("PATH", &path)
        .env("OO_DATA_DIR", &data_dir)
        .assert()
        .failure();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();

    assert!(
        stdout.contains("lines truncated"),
        "200 lines must be truncated in display; got: {stdout:?}"
    );
    assert!(
        !stdout.contains(HIDDEN_MARKER),
        "marker must be outside the displayed slice; got: {stdout:?}"
    );
    assert!(
        stdout.contains("use `oo recall` to query"),
        "recall must be advertised for 200 lines; got: {stdout:?}"
    );

    let out = oo()
        .args(["recall", HIDDEN_MARKER])
        .env("OO_DATA_DIR", &data_dir)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let recall = String::from_utf8_lossy(&out).to_string();
    assert!(
        recall.contains(HIDDEN_MARKER),
        "recall must return the hidden marker; got: {recall:?}"
    );
}
