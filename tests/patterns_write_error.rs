//! A stdout write failure that is not a closed pipe (here ENOSPC from /dev/full)
//! must be reported on stderr and exit 1, not end silently.

#![cfg(target_os = "linux")]

use std::process::{Command, Stdio};
use tempfile::TempDir;

#[test]
fn stdout_write_failure_prints_error_and_exits_1() {
    let config = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let full = std::fs::File::options()
        .write(true)
        .open("/dev/full")
        .expect("/dev/full exists on Linux");

    let output = Command::new(env!("CARGO_BIN_EXE_oo"))
        .arg("patterns")
        .env("OO_CONFIG_DIR", config.path())
        .env("OO_DATA_DIR", data.path())
        .stdout(Stdio::from(full))
        .stderr(Stdio::piped())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1), "{:?}", output.status);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.starts_with("oo: "), "stderr: {stderr}");
    assert!(
        stderr.contains("No space left on device"),
        "stderr: {stderr}"
    );
}
