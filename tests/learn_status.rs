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
    // The detached child writes into this temp dir; wait for it so it cannot
    // recreate the directory after the TempDir is dropped.
    assert!(wait_for_status(
        &cfg.path().join("learn-status.log"),
        "FAILED"
    ));
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
    // The next foreground invocation prints the recorded failure.
    oo().args(["echo", "hi"])
        .env("OO_CONFIG_DIR", &cfg)
        .env("OO_DATA_DIR", data.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("oo: learn failed for echo"));
    assert!(
        !cfg.join("learn-status.log").exists(),
        "status is cleared once shown"
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
fn background_child_removes_payload_shaped_file_on_invalid_json() {
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
fn background_child_keeps_non_payload_file() {
    let root = TempDir::new().unwrap();
    let other = root.path().join("notes.txt");
    std::fs::write(&other, "not a payload").unwrap();
    let cfg = root.path().join("cfg");
    run_bg_child(&other, &cfg);
    assert!(other.exists(), "_learn_bg must not delete arbitrary files");
    let status = std::fs::read_to_string(cfg.join("learn-status.log")).unwrap();
    assert!(status.contains("not a learn payload"), "{status}");
}

#[test]
fn status_writer_unwritable_dir_fails_without_fallback_file() {
    let root = TempDir::new().unwrap();
    let blocker = root.path().join("not-a-dir");
    std::fs::write(&blocker, "x").unwrap();
    let status = blocker.join("learn-status.log");
    assert!(double_o::learn_status::write_learn_status_failure(&status, "git", "boom").is_err());
    assert!(!root.path().join("learn-status.log").exists());
}

#[test]
fn unwritable_status_dir_stays_silent_for_child_and_next_invocation() {
    let root = TempDir::new().unwrap();
    let blocker = root.path().join("cfg-blocker");
    std::fs::write(&blocker, "x").unwrap();
    let cfg = blocker.join("cfg");
    let data = TempDir::new().unwrap();
    let payload = root.path().join(".tmpSilnt1");
    std::fs::write(
        &payload,
        r#"{"command":"echo hi","output":"","exit_code":0}"#,
    )
    .unwrap();
    let child = oo()
        .args([
            "_learn_bg",
            payload.to_str().unwrap(),
            root.path().to_str().unwrap(),
        ])
        .env("OO_CONFIG_DIR", &cfg)
        .env("OO_DATA_DIR", data.path())
        .env_remove("ANTHROPIC_API_KEY")
        .output()
        .unwrap();
    assert!(child.status.success());
    assert!(child.stderr.is_empty(), "child stderr must be silent");
    assert!(!payload.exists(), "payload is removed after the read");
    oo().args(["echo", "hi"])
        .env("OO_CONFIG_DIR", &cfg)
        .env("OO_DATA_DIR", data.path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
}

#[test]
fn background_child_records_missing_key_and_removes_payload() {
    let root = TempDir::new().unwrap();
    let payload = root.path().join(".tmpValid1");
    std::fs::write(
        &payload,
        r#"{"command":"echo hello","output":"hello","exit_code":0}"#,
    )
    .unwrap();
    let cfg = root.path().join("cfg");
    run_bg_child(&payload, &cfg);
    assert!(!payload.exists(), "valid payload must be removed");
    let status = std::fs::read_to_string(cfg.join("learn-status.log")).unwrap();
    assert!(status.starts_with("FAILED echo-hello: "), "{status}");
}

#[test]
fn forget_error_text_is_unchanged() {
    // A data path below a regular file makes the store unopenable.
    let root = TempDir::new().unwrap();
    let blocker = root.path().join("data-blocker");
    std::fs::write(&blocker, "x").unwrap();
    oo().arg("forget")
        .env("OO_CONFIG_DIR", root.path().join("cfg"))
        .env("OO_DATA_DIR", blocker.join("data"))
        .assert()
        .code(1)
        .stderr(predicate::str::starts_with("oo: store error: "))
        .stderr(predicate::str::contains("config error").not());
}

#[test]
fn forget_ignores_learn_config_errors() {
    let cfg = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    std::fs::write(cfg.path().join("config.toml"), "[learn\n").unwrap();
    oo().arg("forget")
        .env("OO_CONFIG_DIR", cfg.path())
        .env("OO_DATA_DIR", data.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Cleared project data"))
        .stderr(predicate::str::contains("config error").not());
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
fn background_child_takes_payload_dir_from_argv_not_tmpdir() {
    let root = TempDir::new().unwrap();
    let payload = root.path().join(".tmpArgv01");
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
        "the payload directory is passed explicitly; the child's TMPDIR must not decide which directory is checked"
    );
}

#[test]
fn background_payload_without_dir_argument_is_recorded_and_kept() {
    let root = TempDir::new().unwrap();
    let payload = root.path().join(".tmpNoDir1");
    let original = r#"{"command":"echo hi","output":"","exit_code":0}"#;
    std::fs::write(&payload, original).unwrap();
    let cfg = root.path().join("cfg");
    let data = TempDir::new().unwrap();
    oo().args(["_learn_bg", payload.to_str().unwrap()])
        .env("OO_CONFIG_DIR", &cfg)
        .env_remove("ANTHROPIC_API_KEY")
        .assert()
        .success();
    assert_eq!(
        std::fs::read_to_string(&payload).unwrap(),
        original,
        "a path without a directory argument must be neither read nor removed"
    );
    let status = std::fs::read_to_string(cfg.join("learn-status.log"))
        .expect("a missing directory argument must be recorded");
    assert!(
        status.starts_with("FAILED learn: _learn_bg needs a payload directory argument"),
        "{status}"
    );
    oo().args(["echo", "hi"])
        .env("OO_CONFIG_DIR", &cfg)
        .env("OO_DATA_DIR", data.path())
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "oo: learn failed for learn \u{2014} _learn_bg needs a payload directory argument",
        ));
}

#[cfg(unix)]
#[test]
fn undeletable_payload_is_silent_when_status_write_also_fails() {
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
    let Some(_locked) = lock_or_skip(&payload_dir) else {
        return;
    };
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
    assert!(output.status.success());
    // Both the removal and the status write fail: nothing is printed, by design.
    assert!(
        output.stderr.is_empty(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(payload.exists(), "the undeletable payload stays on disk");
}

/// Makes a directory read-only and restores it on drop, so a failing assert
/// cannot strand the TempDir.
#[cfg(unix)]
struct ReadOnlyDir(std::path::PathBuf);

#[cfg(unix)]
impl Drop for ReadOnlyDir {
    fn drop(&mut self) {
        set_mode(&self.0, 0o700);
    }
}

#[cfg(unix)]
fn set_mode(dir: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// Read-only `dir` for a permission test, or `None` (test skipped) when this
/// process can still write there, as root does regardless of mode bits.
#[cfg(unix)]
fn lock_or_skip(dir: &Path) -> Option<ReadOnlyDir> {
    set_mode(dir, 0o500);
    let guard = ReadOnlyDir(dir.to_path_buf());
    let probe = dir.join(".write-probe");
    if std::fs::write(&probe, b"").is_ok() {
        let _ = std::fs::remove_file(&probe);
        eprintln!(
            "skipped: {} stays writable (running as root?)",
            dir.display()
        );
        return None;
    }
    Some(guard)
}

#[cfg(unix)]
#[test]
fn undeletable_payload_error_is_recorded_when_status_dir_is_writable() {
    let root = TempDir::new().unwrap();
    let payload_dir = root.path().join("payloads");
    std::fs::create_dir(&payload_dir).unwrap();
    let payload = payload_dir.join(".tmpLocks2");
    std::fs::write(
        &payload,
        r#"{"command":"echo hi","output":"","exit_code":0}"#,
    )
    .unwrap();
    let Some(_locked) = lock_or_skip(&payload_dir) else {
        return;
    };
    let cfg = root.path().join("cfg");
    oo().args([
        "_learn_bg",
        payload.to_str().unwrap(),
        payload_dir.to_str().unwrap(),
    ])
    .env("OO_CONFIG_DIR", &cfg)
    .env_remove("ANTHROPIC_API_KEY")
    .assert()
    .success();
    assert!(payload.exists(), "the undeletable payload stays on disk");
    let status = std::fs::read_to_string(cfg.join("learn-status.log"))
        .expect("the removal failure must reach the writable status file");
    assert!(status.contains("could not remove payload"), "{status}");
}
