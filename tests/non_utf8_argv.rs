//! Non-UTF-8 argv handling (issue #213): byte-exact passthrough on unix,
//! exit-2 rejection of non-UTF-8 reserved-argument values, and the
//! non-UTF-8 TMPDIR learn background child.

#![cfg(unix)]

use std::ffi::OsString;
use std::io::Read;
use std::net::TcpListener;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tempfile::TempDir;

const TIMEOUT: Duration = Duration::from_secs(10);

/// The non-UTF-8 bytes used throughout: 0xFF 0xFE 0x41 ('A').
fn bad_bytes() -> Vec<u8> {
    vec![0xff, 0xfe, 0x41]
}

/// Create a directory with a non-UTF-8 name (or a valid fallback on macOS,
/// where `std::fs` rejects invalid UTF-8 path names). The fallback still
/// exercises the same `Path`/`OsString` code paths.
fn make_bad_dir(root: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    let candidate = root.join(OsString::from_vec(bytes.to_vec()));
    if std::fs::create_dir(&candidate).is_ok() {
        return Ok(candidate);
    }
    let fallback = root.join("__non_utf8_dir");
    std::fs::create_dir(&fallback).map_err(|e| e.to_string())?;
    Ok(fallback)
}

/// Spawn a command with a bounded timeout: returns output if the child exits
/// within the deadline, or panics after killing and reaping it.
fn run_with_timeout(mut cmd: Command) -> std::process::Output {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("spawn failed: {e}"));

    let deadline = Instant::now() + TIMEOUT;
    loop {
        match child.try_wait().unwrap() {
            Some(status) => {
                let mut stdout_buf = Vec::new();
                if let Some(mut s) = child.stdout.take() {
                    s.read_to_end(&mut stdout_buf).unwrap();
                }
                let mut stderr_buf = Vec::new();
                if let Some(mut s) = child.stderr.take() {
                    s.read_to_end(&mut stderr_buf).unwrap();
                }
                return std::process::Output {
                    status,
                    stdout: stdout_buf,
                    stderr: stderr_buf,
                };
            }
            None => {
                if Instant::now() > deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
    // Bounded timeout exceeded: kill and reap.
    let _ = child.kill();
    let status = child.wait().unwrap();
    let mut stdout_buf = Vec::new();
    if let Some(mut s) = child.stdout.take() {
        s.read_to_end(&mut stdout_buf).unwrap();
    }
    let mut stderr_buf = Vec::new();
    if let Some(mut s) = child.stderr.take() {
        s.read_to_end(&mut stderr_buf).unwrap();
    }
    let out = std::process::Output {
        status,
        stdout: stdout_buf,
        stderr: stderr_buf,
    };
    panic!(
        "child timed out after {:?}; stderr: {}",
        TIMEOUT,
        String::from_utf8_lossy(&out.stderr)
    );
}

/// An `oo` command with a per-child env sandbox: temp OO_DATA_DIR,
/// OO_CONFIG_DIR, HOME, XDG_*, and TMPDIR.
fn oo_sandboxed() -> (Command, TempDir) {
    let root = TempDir::new().unwrap();
    let cfg = root.path().join("cfg");
    let data = root.path().join("data");
    let tmp = TempDir::new().unwrap();
    let home = root.path().join("home");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_oo"));
    cmd.env("OO_CONFIG_DIR", &cfg)
        .env("OO_DATA_DIR", &data)
        .env("TMPDIR", tmp.path())
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join("xdg-config"))
        .env("XDG_DATA_HOME", home.join("xdg-data"));
    (cmd, root)
}

#[test]
fn non_utf8_wrapped_arg_is_passed_byte_exactly() {
    let (mut cmd, root) = oo_sandboxed();
    let out_file = root.path().join("out.bin");

    cmd.args(["sh", "-c", "printf %s \"$1\" > \"$2\"", "x"])
        .arg(OsString::from_vec(bad_bytes()))
        .arg(&out_file);

    let output = run_with_timeout(cmd);
    assert_eq!(
        output.status.code(),
        Some(0),
        "oo must exit 0; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("panicked at"),
        "stderr must not contain a panic: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let file_bytes = std::fs::read(&out_file).unwrap();
    assert_eq!(
        file_bytes,
        bad_bytes(),
        "file must contain exactly the non-UTF-8 bytes"
    );
}

#[test]
fn non_utf8_wrapped_arg_preserves_exit_code() {
    let (mut cmd, _root) = oo_sandboxed();
    cmd.args(["sh", "-c", "exit 3", "x"])
        .arg(OsString::from_vec(bad_bytes()));

    let output = run_with_timeout(cmd);
    assert_eq!(
        output.status.code(),
        Some(3),
        "exit code must be 3; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("panicked at"),
        "stderr must not contain a panic"
    );
}

#[test]
fn non_utf8_recall_arg_rejected_with_exit_2() {
    let (mut cmd, _root) = oo_sandboxed();
    cmd.arg("recall").arg(OsString::from_vec(bad_bytes()));

    let output = run_with_timeout(cmd);
    assert_eq!(
        output.status.code(),
        Some(2),
        "oo recall <non-UTF-8> must exit 2; got {:?}",
        output.status.code()
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("argument is not valid UTF-8"),
        "stderr must contain the rejection message; got: {stderr}"
    );
    assert!(!stderr.contains("panicked at"), "no panic allowed");
}

#[test]
fn non_utf8_forget_arg_rejected_with_exit_2() {
    let (mut cmd, _root) = oo_sandboxed();
    cmd.arg("forget").arg(OsString::from_vec(bad_bytes()));

    let output = run_with_timeout(cmd);
    assert_eq!(
        output.status.code(),
        Some(2),
        "oo forget <non-UTF-8> must exit 2; got {:?}",
        output.status.code()
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("argument is not valid UTF-8"),
        "stderr must contain the rejection message; got: {stderr}"
    );
}

#[test]
fn non_utf8_learn_hint_rejected_with_exit_2() {
    let (mut cmd, _root) = oo_sandboxed();
    cmd.arg("learn")
        .arg("--hint")
        .arg(OsString::from_vec(bad_bytes()))
        .arg("echo")
        .arg("hi");

    let output = run_with_timeout(cmd);
    assert_eq!(
        output.status.code(),
        Some(2),
        "oo learn --hint <non-UTF-8> must exit 2; got {:?}",
        output.status.code()
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("argument is not valid UTF-8"),
        "stderr must contain the rejection message; got: {stderr}"
    );
}

#[test]
fn non_utf8_learn_wrapped_arg_passes_through() {
    let (mut cmd, root) = oo_sandboxed();
    let out_file = root.path().join("learn_out.bin");

    cmd.args(["learn", "sh", "-c", "printf %s \"$1\" > \"$2\"", "x"])
        .arg(OsString::from_vec(bad_bytes()))
        .arg(&out_file);

    let output = run_with_timeout(cmd);
    assert_eq!(
        output.status.code(),
        Some(0),
        "oo learn with non-UTF-8 wrapped arg must exit 0; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let file_bytes = std::fs::read(&out_file).unwrap();
    assert_eq!(
        file_bytes,
        bad_bytes(),
        "learn must pass the bytes to the wrapped command unchanged"
    );
}

/// A non-UTF-8 TMPDIR must not leave `.tmp*` payload files behind.
#[test]
fn non_utf8_tmpdir_learn_background_leaves_no_payload() {
    let root = TempDir::new().unwrap();
    let bad_tmp = make_bad_dir(root.path(), &[0xff, 0xfe, 0x41]).unwrap();

    let cfg = root.path().join("cfg");
    let data = root.path().join("data");
    let home = root.path().join("home");

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_oo"));
    cmd.args(["learn", "sh", "-c", "exit 1", "x"])
        .env("OO_CONFIG_DIR", &cfg)
        .env("OO_DATA_DIR", &data)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join("xdg-config"))
        .env("XDG_DATA_HOME", home.join("xdg-data"))
        .env("TMPDIR", &bad_tmp)
        .env("ANTHROPIC_API_KEY", "dummy-key")
        .env("ANTHROPIC_API_URL", format!("http://{addr}/v1/messages"))
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .env_remove("ALL_PROXY")
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .env_remove("all_proxy")
        .env("NO_PROXY", "127.0.0.1,localhost");

    let output = run_with_timeout(cmd);
    assert_eq!(
        output.status.code(),
        Some(1),
        "oo learn must exit 1 (sh -c exit 1); stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Poll for the status file to appear (bounded wait), then check for leftovers.
    let status_path = cfg.join("learn-status.log");
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if status_path.exists() {
            break;
        }
        if Instant::now() > deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    // Check for .tmp* leftovers in the non-UTF-8 TMPDIR.
    let leftovers: Vec<OsString> = std::fs::read_dir(&bad_tmp)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name())
        .filter(|n| n.to_string_lossy().starts_with(".tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "non-UTF-8 TMPDIR must not have .tmp* leftovers: {leftovers:?}"
    );

    // The status file should record a FAILED line.
    if status_path.exists() {
        let status = std::fs::read_to_string(&status_path).unwrap();
        assert!(
            status.contains("FAILED"),
            "learn-status.log must record a FAILED line; got: {status}"
        );
    }
}

/// `_learn_bg <payload-shaped file> <non-UTF-8 dir containing it>` removes the file.
#[test]
fn learn_bg_removes_payload_in_non_utf8_dir() {
    let root = TempDir::new().unwrap();
    let bad_dir = make_bad_dir(root.path(), &[0xff, 0xfe, 0x41]).unwrap();

    let payload = bad_dir.join(".tmpAbC123");
    std::fs::write(
        &payload,
        r#"{"command":"echo hi","output":"hi","exit_code":0}"#,
    )
    .unwrap();

    let cfg = root.path().join("cfg");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_oo"));
    cmd.args(["_learn_bg"])
        .arg(&payload)
        .arg(&bad_dir)
        .env("OO_CONFIG_DIR", &cfg)
        .env_remove("ANTHROPIC_API_KEY");

    let output = run_with_timeout(cmd);
    assert_eq!(
        output.status.code(),
        Some(0),
        "_learn_bg must exit 0; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !payload.exists(),
        "payload must be removed after _learn_bg processes it"
    );
}

/// `_learn_bg <file> <other dir>` keeps the file and records `not a learn payload`.
#[test]
fn learn_bg_keeps_non_payload_file_with_byte_exact_message() {
    let root = TempDir::new().unwrap();
    let file = root.path().join("notes.txt");
    std::fs::write(&file, "not a payload").unwrap();

    let cfg = root.path().join("cfg");
    let other_dir = root.path().join("other");
    std::fs::create_dir(&other_dir).unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_oo"));
    cmd.args(["_learn_bg"])
        .arg(&file)
        .arg(&other_dir)
        .env("OO_CONFIG_DIR", &cfg)
        .env_remove("ANTHROPIC_API_KEY");

    let output = run_with_timeout(cmd);
    assert_eq!(
        output.status.code(),
        Some(0),
        "_learn_bg must exit 0; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(file.exists(), "non-payload file must not be deleted");

    let status = std::fs::read_to_string(cfg.join("learn-status.log")).unwrap();
    assert!(
        status.contains("not a learn payload"),
        "status must contain 'not a learn payload'; got: {status}"
    );
}

/// `oo hook ""` must print the same stderr as main (the unknown-agent error),
/// not the no-agent error. Regression guard for the `.filter(|s| !s.is_empty())`
/// that was added during the OsString migration (issue #213).
#[test]
fn valid_utf8_hook_empty_string_matches_main_behavior() {
    let (mut cmd, _root) = oo_sandboxed();
    cmd.arg("hook").arg("");

    let output = run_with_timeout(cmd);
    assert_eq!(
        output.status.code(),
        Some(1),
        "oo hook \"\" must exit 1; got {:?}",
        output.status.code()
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr == "oo: unknown hook agent ''; supported hook agents: claude\n",
        "stderr must match main's exact unknown-agent error; got: {stderr:?}"
    );
}

/// Table-driven: every reserved subcommand's own text argument, when given
/// non-UTF-8 bytes, must be rejected with exit 2 and the D3 rejection line.
#[test]
fn all_reserved_subcommands_reject_non_utf8_text_arg() {
    let byte = OsString::from_vec(vec![0xff]);
    let cases: &[(&str, &[&str])] = &[
        ("recall", &["recall"] as &[&str]),
        ("forget", &["forget"].as_ref()),
        ("learn --hint", &["learn", "--hint"].as_ref()),
        ("help", &["help"].as_ref()),
        ("rewrite", &["rewrite"].as_ref()),
        ("hook", &["hook"].as_ref()),
        ("init --format", &["init", "--format"].as_ref()),
        ("patterns", &["patterns"].as_ref()),
        ("version", &["version"].as_ref()),
    ];

    for &(label, prefix) in cases {
        let (mut cmd, _root) = oo_sandboxed();
        cmd.args(prefix.as_ref()).arg(&byte);
        let output = run_with_timeout(cmd);
        assert_eq!(
            output.status.code(),
            Some(2),
            "oo {label} <non-UTF-8> must exit 2; got {:?}",
            output.status.code()
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("argument is not valid UTF-8"),
            "oo {label} stderr must contain the D3 rejection message; got: {stderr}"
        );
    }
}
