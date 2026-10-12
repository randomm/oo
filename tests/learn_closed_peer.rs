//! A peer that resets the connection mid-request must not kill the background
//! learn child with SIGPIPE: the write has to fail with an error that is recorded
//! in `learn-status.log`, and the child must exit normally.
//!
//! Rust std suppresses SIGPIPE per send (`MSG_NOSIGNAL` on Linux) or per socket
//! (`SO_NOSIGPIPE` on Apple platforms). The test checks the Linux and macOS
//! behaviour; the process-wide SIGPIPE disposition is set to default in `main`.
//!
//! The listener accepts one connection, then closes it with `SO_LINGER` 0 so the
//! kernel sends an RST. Whether the child's write or its read observes the reset
//! depends on timing, so the test does not pin which one fails. It asserts that
//! the child exits normally and records an `Anthropic API error` (the request
//! path was reached), not a config or URL rejection.

#![cfg(unix)]

use std::net::TcpListener;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tempfile::TempDir;

const KEY_ENV: &str = "OO_TEST_DUMMY_LEARN_KEY";

/// How long the listener waits for a connection, and the child's exit bound.
const WAIT: Duration = Duration::from_secs(10);

/// Large command output, so the payload file is multi-megabyte as well.
/// The prompt truncates output to 4000 characters, so the request stays small.
fn payload() -> String {
    let output = "x".repeat(4 * 1024 * 1024);
    serde_json::json!({"command": "echo big", "output": output, "exit_code": 0}).to_string()
}

/// Accept one connection and reset it immediately with `SO_LINGER` 0.
///
/// Polls a non-blocking listener, so a child that never connects cannot block
/// this thread past `WAIT`. Returns true only when a peer connected and was reset,
/// so the caller can tell a real reset from a silent timeout.
fn reset_first_connection(listener: TcpListener) -> bool {
    use std::os::fd::AsRawFd;
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + WAIT;
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() > deadline {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return false,
        }
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
    true
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
        // ureq reads proxy variables from the environment; the loopback peer must be reached directly.
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .env_remove("ALL_PROXY")
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .env_remove("all_proxy")
        .env("NO_PROXY", "127.0.0.1,localhost")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let deadline = Instant::now() + WAIT;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("_learn_bg did not exit within {WAIT:?} after the peer reset");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let connected = server.join().unwrap();
    assert!(connected, "the child never connected to the listener");

    assert!(
        status.signal().is_none(),
        "background learn child was killed by signal {:?}",
        status.signal()
    );
    // `_learn_bg` always exits 0 after recording its status (see main.rs).
    assert_eq!(
        status.code(),
        Some(0),
        "background learn child exit: {status:?}"
    );
    assert_api_error_status(&cfg.join("learn-status.log"));
    assert!(!payload_path.exists(), "payload must be removed after read");
}

/// The failure must be a socket I/O error, which shows the request reached the
/// socket. The exact errno varies with timing (reset, broken pipe, or EINVAL on
/// macOS), so only the `io:` class is asserted. A config or URL rejection does
/// not produce an `io:` error.
fn assert_api_error_status(path: &Path) {
    let status =
        std::fs::read_to_string(path).expect("failure must be recorded in learn-status.log");
    assert!(
        status.contains("Anthropic API error: io: "),
        "expected a socket I/O error, got: {status}"
    );
}
