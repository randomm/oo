//! An empty or whitespace-only API key must be treated as unset: `oo learn`
//! reports the missing-key error and sends no HTTP request at all.
//!
//! Drives the hidden `_learn_bg` child directly (same argv `spawn_background`
//! uses) against a loopback listener standing in for the API. The listener is
//! non-blocking, so any connection the child makes is visible as an accepted
//! connection; a bug that sends the empty key would also leave the child
//! waiting on a response, so the child is killed after a bounded wait.

#![cfg(unix)]

use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tempfile::TempDir;

const PAYLOAD: &str = r#"{"command":"echo hi","output":"hi","exit_code":0}"#;

/// Run `_learn_bg` with `api_key` in `ANTHROPIC_API_KEY`, pointed at `listener`.
/// Returns once the child exits (or is killed after a bounded wait).
fn run_bg_with_key(root: &Path, api_key: &str, listener: &TcpListener) {
    let payload = root.join(".tmpAbC123");
    std::fs::write(&payload, PAYLOAD).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_oo"))
        .args([
            "_learn_bg",
            payload.to_str().unwrap(),
            root.to_str().unwrap(),
        ])
        .env("OO_CONFIG_DIR", root.join("cfg"))
        .env("OO_DATA_DIR", root.join("data"))
        .env("ANTHROPIC_API_KEY", api_key)
        .env(
            "ANTHROPIC_API_URL",
            format!("http://{}/v1/messages", listener.local_addr().unwrap()),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if child.try_wait().unwrap().is_some() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn assert_no_request_and_missing_key_status(api_key: &str) {
    let root = TempDir::new().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();

    run_bg_with_key(root.path(), api_key, &listener);

    assert!(
        listener.accept().is_err(),
        "API key {api_key:?} must not produce an HTTP request"
    );
    let status = std::fs::read_to_string(root.path().join("cfg").join("learn-status.log"))
        .expect("missing key must be recorded in learn-status.log");
    assert!(
        status.contains("Set ANTHROPIC_API_KEY environment variable"),
        "{status}"
    );
}

#[test]
fn empty_api_key_is_treated_as_unset() {
    assert_no_request_and_missing_key_status("");
}

#[test]
fn whitespace_only_api_key_is_treated_as_unset() {
    assert_no_request_and_missing_key_status("  \t ");
}
