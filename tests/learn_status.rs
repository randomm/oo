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
    double_o::commands::write_learn_status_failure(&status, "cargo-test", "no API key set")
        .expect("write must create the missing directory");
    let content = std::fs::read_to_string(&status).unwrap();
    assert!(content.starts_with("FAILED cargo-test: no API key set"));
}

#[test]
fn learn_success_status_writer_creates_missing_config_dir() {
    let root = TempDir::new().unwrap();
    let status = root.path().join("missing").join("learn-status.log");
    double_o::commands::write_learn_status(&status, "git", &status)
        .expect("write must create the missing directory");
    assert!(status.exists());
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
fn forget_output_is_unchanged() {
    let data = TempDir::new().unwrap();
    oo().arg("forget")
        .env("OO_DATA_DIR", data.path())
        .assert()
        .success()
        .stdout("Cleared project data (0 entries)\n");
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

/// Runs the hidden `_learn_bg <file>` child directly, so the early-error
/// returns (parse failure, exit-code range) are observed on disk without
/// depending on a detached process.
fn run_bg_child(payload: &Path, cfg: &Path) {
    oo().args(["_learn_bg", payload.to_str().unwrap()])
        .env("OO_CONFIG_DIR", cfg)
        .env_remove("ANTHROPIC_API_KEY")
        .assert()
        .success();
}

#[test]
fn background_child_removes_payload_on_invalid_json() {
    let root = TempDir::new().unwrap();
    let payload = root.path().join(".tmpBadJson");
    std::fs::write(&payload, "not valid json {{{").unwrap();
    run_bg_child(&payload, &root.path().join("cfg"));
    assert!(
        !payload.exists(),
        "payload must be removed after a parse error"
    );
}

#[test]
fn background_child_removes_payload_on_exit_code_out_of_range() {
    let root = TempDir::new().unwrap();
    let payload = root.path().join(".tmpBadExit");
    let data = serde_json::json!({
        "command": "echo hello",
        "output": "hello",
        "exit_code": 2147483648i64
    });
    std::fs::write(&payload, data.to_string()).unwrap();
    run_bg_child(&payload, &root.path().join("cfg"));
    assert!(
        !payload.exists(),
        "payload must be removed after an exit-code error"
    );
}

#[test]
fn status_writer_unwritable_dir_fails_without_fallback_file() {
    let root = TempDir::new().unwrap();
    let blocker = root.path().join("not-a-dir");
    std::fs::write(&blocker, "x").unwrap();
    let status = blocker.join("learn-status.log");
    assert!(double_o::commands::write_learn_status_failure(&status, "git", "boom").is_err());
    assert!(!status.exists());
    assert!(tmp_leftovers(root.path()).is_empty());
}

#[test]
fn unwritable_status_dir_is_silent_for_next_invocation() {
    let root = TempDir::new().unwrap();
    let blocker = root.path().join("not-a-dir");
    std::fs::write(&blocker, "x").unwrap();
    let data = TempDir::new().unwrap();
    oo().args(["learn", "echo", "hi"])
        .env("OO_CONFIG_DIR", &blocker)
        .env("OO_DATA_DIR", data.path())
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .assert()
        .success();
    // Give the detached child time to fail; it must stay silent.
    std::thread::sleep(Duration::from_millis(1500));
    oo().arg("patterns")
        .env("OO_CONFIG_DIR", &blocker)
        .env("OO_DATA_DIR", data.path())
        .assert()
        .success()
        .stderr("");
}

#[test]
fn forget_error_text_is_unchanged() {
    let root = TempDir::new().unwrap();
    // A regular file as the data directory makes the store unopenable.
    let blocker = root.path().join("not-a-dir");
    std::fs::write(&blocker, "x").unwrap();
    oo().arg("forget")
        .env("OO_DATA_DIR", &blocker)
        .assert()
        .failure()
        // Pins the pre-existing prefix; the errno suffix is platform-specific.
        .stderr(predicate::str::starts_with("oo: store error: "))
        .stderr(predicate::str::contains("config error").not());
}
