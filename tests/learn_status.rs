//! Background learn behaviour observed through the `oo` binary and the public
//! status writers: payload cleanup, TMPDIR handling, failure reporting when the
//! config directory is missing, and the foreground config error prefix.
//!
//! The background child is detached, so results are polled for (bounded) rather
//! than asserted immediately.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::Path;
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn oo() -> Command {
    assert_cmd::cargo::cargo_bin_cmd!("oo")
}

/// Poll until `path` exists and contains `needle`, or give up after 10 s.
fn wait_for_status(path: &Path, needle: &str) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if std::fs::read_to_string(path).is_ok_and(|c| c.contains(needle)) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// Names of `.tmp*` entries directly inside `dir`.
fn tmp_leftovers(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(".tmp"))
        .collect()
}

#[test]
fn status_writer_creates_missing_config_dir() {
    let root = TempDir::new().unwrap();
    let status = root.path().join("missing").join("learn-status.log");
    double_o::learn_status::write_learn_status_failure(&status, "cargo-test", "no API key set")
        .expect("write must create the missing directory");
    let content = std::fs::read_to_string(&status).unwrap();
    assert!(content.starts_with("FAILED cargo-test: no API key set"));
}

#[test]
fn foreground_config_error_has_single_prefix() {
    let cfg = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    std::fs::write(cfg.path().join("config.toml"), "[learn]\nmodel = \"x\"\n").unwrap();
    oo().args(["learn", "echo", "hi"])
        .env("OO_CONFIG_DIR", cfg.path())
        .env("OO_DATA_DIR", data.path())
        .env_remove("ANTHROPIC_API_KEY")
        .assert()
        .success()
        .stderr(predicate::str::contains("oo: config error: "))
        .stderr(predicate::str::contains("config error: config error:").not());
}

#[test]
fn background_failure_is_reported_when_config_dir_is_missing() {
    let root = TempDir::new().unwrap();
    let cfg = root.path().join("missing-config");
    let data = TempDir::new().unwrap();
    oo().args(["learn", "echo", "hi"])
        .env("OO_CONFIG_DIR", &cfg)
        .env("OO_DATA_DIR", data.path())
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .assert()
        .success();
    assert!(
        wait_for_status(&cfg.join("learn-status.log"), "FAILED"),
        "background failure must be recorded in learn-status.log"
    );
}

#[test]
fn background_payload_is_not_left_in_tmpdir_or_cwd() {
    let root = TempDir::new().unwrap();
    let cfg = root.path().join("cfg");
    let data = TempDir::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    oo().args(["learn", "echo", "hi"])
        .current_dir(cwd.path())
        .env("OO_CONFIG_DIR", &cfg)
        .env("OO_DATA_DIR", data.path())
        .env("TMPDIR", tmp.path())
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .assert()
        .success();
    // The status file is written only after the payload was consumed.
    assert!(wait_for_status(&cfg.join("learn-status.log"), "FAILED"));
    assert!(tmp_leftovers(tmp.path()).is_empty(), "TMPDIR leftovers");
    assert!(tmp_leftovers(cwd.path()).is_empty(), "cwd leftovers");
}

#[test]
fn empty_tmpdir_does_not_place_payload_in_cwd() {
    let root = TempDir::new().unwrap();
    let cfg = root.path().join("cfg");
    let data = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    oo().args(["learn", "echo", "hi"])
        .current_dir(cwd.path())
        .env("OO_CONFIG_DIR", &cfg)
        .env("OO_DATA_DIR", data.path())
        .env("TMPDIR", "")
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .assert()
        .success();
    assert!(wait_for_status(&cfg.join("learn-status.log"), "FAILED"));
    assert!(tmp_leftovers(cwd.path()).is_empty(), "cwd leftovers");
}

/// Runs the hidden `_learn_bg <file> <payload-dir>` child directly, with the
/// same argv `spawn_background` passes, so early-error returns are observed on
/// disk without depending on a detached process.
fn run_bg_child(payload: &Path, cfg: &Path) {
    oo().args([
        "_learn_bg",
        payload.to_str().unwrap(),
        payload.parent().unwrap().to_str().unwrap(),
    ])
    .env("OO_CONFIG_DIR", cfg)
    .env_remove("ANTHROPIC_API_KEY")
    .assert()
    .success();
}

#[test]
fn background_child_removes_owned_payload_on_invalid_json() {
    let root = TempDir::new().unwrap();
    let payload = root.path().join(".tmpBadJs1");
    std::fs::write(&payload, "not valid json {{{").unwrap();
    run_bg_child(&payload, &root.path().join("cfg"));
    assert!(
        !payload.exists(),
        "payload must be removed after a parse error"
    );
}

#[test]
fn background_child_keeps_unowned_file() {
    let root = TempDir::new().unwrap();
    let other = root.path().join("notes.txt");
    std::fs::write(&other, "not a payload").unwrap();
    run_bg_child(&other, &root.path().join("cfg"));
    assert!(other.exists(), "_learn_bg must not delete arbitrary files");
}

#[test]
fn status_writer_unwritable_dir_fails_without_fallback_file() {
    let root = TempDir::new().unwrap();
    let blocker = root.path().join("not-a-dir");
    std::fs::write(&blocker, "x").unwrap();
    let status = blocker.join("learn-status.log");
    assert!(double_o::learn_status::write_learn_status_failure(&status, "git", "boom").is_err());
}

#[test]
fn background_parse_failure_is_recorded_in_status_log() {
    let root = TempDir::new().unwrap();
    let payload = root.path().join(".tmpBadJs1");
    std::fs::write(&payload, "not valid json {{{").unwrap();
    let cfg = root.path().join("cfg");
    run_bg_child(&payload, &cfg);
    let status = std::fs::read_to_string(cfg.join("learn-status.log"))
        .expect("an early parse error must still reach learn-status.log");
    assert!(status.starts_with("FAILED learn: "), "{status}");
}

#[test]
fn background_child_owns_payload_by_argv_dir_not_tmpdir() {
    let root = TempDir::new().unwrap();
    let payload = root.path().join(".tmpOwned1");
    std::fs::write(&payload, "not valid json {{{").unwrap();
    let other_tmp = TempDir::new().unwrap();
    oo().args([
        "_learn_bg",
        payload.to_str().unwrap(),
        root.path().to_str().unwrap(),
    ])
    .env("OO_CONFIG_DIR", root.path().join("cfg"))
    .env("TMPDIR", other_tmp.path())
    .env_remove("ANTHROPIC_API_KEY")
    .assert()
    .success();
    assert!(
        !payload.exists(),
        "the payload directory is passed explicitly; the child's TMPDIR must not decide ownership"
    );
}

#[cfg(unix)]
#[test]
fn undeletable_payload_is_reported_when_status_write_also_fails() {
    use std::os::unix::fs::PermissionsExt;
    let root = TempDir::new().unwrap();
    let payload_dir = root.path().join("payloads");
    std::fs::create_dir(&payload_dir).unwrap();
    let payload = payload_dir.join(".tmpLocks1");
    std::fs::write(
        &payload,
        r#"{"command":"echo hi","output":"","exit_code":0}"#,
    )
    .unwrap();
    // Read-only directory: the payload can be read but not removed.
    std::fs::set_permissions(&payload_dir, std::fs::Permissions::from_mode(0o500)).unwrap();
    // A regular file where the config dir should be makes the status write fail.
    let blocker = root.path().join("cfg-blocker");
    std::fs::write(&blocker, "x").unwrap();
    let output = oo()
        .args([
            "_learn_bg",
            payload.to_str().unwrap(),
            payload_dir.to_str().unwrap(),
        ])
        .env("OO_CONFIG_DIR", blocker.join("cfg"))
        .env_remove("ANTHROPIC_API_KEY")
        .output()
        .unwrap();
    // Restore before asserting, so a failed assertion cannot strand the TempDir.
    std::fs::set_permissions(&payload_dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(output.status.success());
    // The status write failed too, so the stderr fallback is the only report.
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("could not remove payload"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
