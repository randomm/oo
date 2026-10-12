//! A closed stdout pipe must end `oo` with SIGPIPE, not a panic from `println!`.
//! Rust leaves SIGPIPE ignored, so without the reset in `main` the write fails
//! with EPIPE and the process exits 101 with "failed printing to stdout".

#![cfg(unix)]

use std::os::unix::io::FromRawFd;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};
use tempfile::TempDir;

#[test]
fn closed_stdout_pipe_terminates_by_sigpipe_without_panic() {
    let config = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();

    // Close the read end before spawning, so the child's first write hits EPIPE.
    let mut fds = [0 as libc::c_int; 2];
    // SAFETY: pipe(2) writes two valid fds into the array on success.
    assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
    // SAFETY: fds[0] is the open read end; closing it is the test's intent.
    unsafe { libc::close(fds[0]) };
    // SAFETY: fds[1] is an open write end owned by nothing else.
    let stdout = unsafe { Stdio::from_raw_fd(fds[1]) };

    let output = Command::new(env!("CARGO_BIN_EXE_oo"))
        .arg("version")
        .env("OO_CONFIG_DIR", config.path())
        .env("OO_DATA_DIR", data.path())
        .stdout(stdout)
        .stderr(Stdio::piped())
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("Broken pipe") && !stderr.contains("panicked"),
        "oo panicked on a closed pipe: {stderr}"
    );
    assert_eq!(
        output.status.signal(),
        Some(libc::SIGPIPE),
        "expected termination by SIGPIPE, got {:?}",
        output.status
    );
}
