//! A peer that resets the connection mid-request must not kill the background
//! learn child with SIGPIPE: the write has to fail with an error that is recorded
//! in `learn-status.log`, and the child must exit normally.
//!
//! Rust std sends with `MSG_NOSIGNAL` (Linux/Android) or sets `SO_NOSIGPIPE` on
//! the socket (Apple, FreeBSD, NetBSD, DragonFly), so the process-wide SIGPIPE
//! disposition does not matter for std sockets. This test pins that behaviour.
//!
//! The listener accepts, then closes with `SO_LINGER` 0 so the kernel sends an
//! RST. The request body is sent after the reset has been queued, which is the
//! case where an unsuppressed write raises SIGPIPE.

#![cfg(unix)]

use std::net::TcpListener;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tempfile::TempDir;

const KEY_ENV: &str = "OO_TEST_DUMMY_LEARN_KEY";

/// Large command output, so the payload file is multi-megabyte as well.
/// The prompt truncates output to 4000 characters, so the request stays small.
fn payload() -> String {
    let output = "x".repeat(4 * 1024 * 1024);
    serde_json::json!({"command": "echo big", "output": output, "exit_code": 0}).to_string()
}

/// Accept one connection and reset it immediately with `SO_LINGER` 0.
fn reset_first_connection(listener: TcpListener) {
    use std::os::fd::AsRawFd;
    let Ok((stream, _)) = listener.accept() else {
        return;
    };
    let linger = libc::linger {
        l_onoff: 1,
        l_linger: 0,
    };
    // SAFETY: the fd belongs to `stream`, which is alive for this call, and the
    // option value points at a valid `linger` of the stated size.
    unsafe {
        libc::setsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_LINGER,
            (&linger as *const libc::linger).cast(),
            std::mem::size_of::<libc::linger>() as libc::socklen_t,
        );
    }
    drop(stream);
}

#[test]
fn reset_peer_does_not_kill_background_learn_child() {
    let root = TempDir::new().unwrap();
    let cfg = root.path().join("cfg");
    std::fs::create_dir_all(&cfg).unwrap();
    std::fs::write(
        cfg.join("config.toml"),
        format!(
            "[learn]\nprovider = \"anthropic\"\nmodel = \"test-model\"\napi_key_env = \"{KEY_ENV}\"\n"
        ),
    )
    .unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || reset_first_connection(listener));

    let payload_path = root.path().join(".tmpAbC123");
    std::fs::write(&payload_path, payload()).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_oo"))
        .args([
            "_learn_bg",
            payload_path.to_str().unwrap(),
            root.path().to_str().unwrap(),
        ])
        .env("OO_CONFIG_DIR", &cfg)
        .env("OO_DATA_DIR", root.path().join("data"))
        .env(KEY_ENV, "dummy-key")
        .env("ANTHROPIC_API_URL", format!("http://{addr}/v1/messages"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("_learn_bg did not exit within 10s after the peer reset");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    server.join().unwrap();

    assert!(
        status.signal().is_none(),
        "background learn child was killed by signal {:?}",
        status.signal()
    );
    assert_failed_status(&cfg.join("learn-status.log"));
    assert!(!payload_path.exists(), "payload must be removed after read");
}

fn assert_failed_status(path: &Path) {
    let status =
        std::fs::read_to_string(path).expect("failure must be recorded in learn-status.log");
    assert!(status.contains("FAILED"), "{status}");
}
