//! `oo` must not hang or fail when the patterns directory contains
//! non-regular `*.toml` entries: a FIFO, a directory, a dangling symlink,
//! a symlink to `/dev/zero`, or a Unix socket. The loader only reads
//! regular files (or symlinks to them); `oo patterns` additionally reports
//! metadata errors such as a symlink loop.

#![cfg(unix)]

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::fs::symlink;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

const DEADLINE: Duration = Duration::from_secs(10);
const VALID_TOML: &str = "command_match = \"^parity-cmd\"\n";

/// Spawns `oo` with an isolated config/data dir, waits at most `DEADLINE`,
/// and fails the test if the child was still running.
fn run_oo(
    config: &TempDir,
    data: &TempDir,
    cwd: &TempDir,
    args: &[&str],
) -> (ExitStatus, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_oo"))
        .args(args)
        .current_dir(cwd.path())
        .env("OO_CONFIG_DIR", config.path())
        .env("OO_DATA_DIR", data.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let deadline = Instant::now() + DEADLINE;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    // Regression: the stray entry blocked `oo`. Kill and reap.
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!(
                        "oo {args:?} did not exit within {DEADLINE:?} — the non-regular entry was not skipped"
                    );
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(_) => panic!("failed to wait on oo {args:?}"),
        }
    };

    let stdout = read_pipe(&mut child.stdout);
    let stderr = read_pipe(&mut child.stderr);
    (status, stdout, stderr)
}

/// Reads a piped child's stdout or stderr to EOF after the child has been
/// reaped (so the pipe is already closed and no writer remains to block us).
fn read_pipe<R>(pipe: &mut Option<R>) -> String
where
    R: std::io::Read,
{
    let mut buf = String::new();
    if let Some(mut p) = pipe.take() {
        let _ = std::io::Read::read_to_string(&mut p, &mut buf);
    }
    buf
}

/// Creates one `*.toml` entry of each non-regular kind in `patterns`.
/// The FIFO is only stat'ed by the loader, never opened, so this cannot hang.
fn create_stray(patterns: &Path, name: &str) {
    let path = patterns.join(name);
    match name {
        "fifo.toml" => {
            // SAFETY: a valid null-terminated path and zero mode; the call is `mkfifo(2)`.
            let rc = unsafe {
                libc::mkfifo(
                    path.to_str().unwrap().as_ptr().cast::<libc::c_char>(),
                    0o600,
                )
            };
            assert_eq!(rc, 0, "mkfifo: {}", std::io::Error::last_os_error());
        }
        "dir.toml" => std::fs::create_dir(&path).unwrap(),
        "dangling.toml" => symlink(patterns.join("missing-target.toml"), &path).unwrap(),
        "zero.toml" => symlink("/dev/zero", &path).unwrap(),
        "socket.toml" => {
            // The unix(7) socket path is limited to 104-108 bytes depending
            // on the OS; `tempfile` paths under a deep `TMPDIR` can exceed
            // that. Fail loudly instead of a cryptic `bind` error.
            if path.as_os_str().len() > 104 {
                panic!("socket path too long for a unix socket: {path:?}");
            }
            let listener = UnixListener::bind(&path)
                .unwrap_or_else(|e| panic!("bind({}): {e}", path.display()));
            drop(listener);
        }
        _ => panic!("unhandled stray kind {name}"),
    }
}

/// Creates all five stray kinds in `patterns` plus a valid pattern file.
fn create_all_strays(patterns: &Path) {
    for name in [
        "fifo.toml",
        "dir.toml",
        "dangling.toml",
        "zero.toml",
        "socket.toml",
    ] {
        create_stray(patterns, name);
    }
    std::fs::write(patterns.join("parity.toml"), VALID_TOML).unwrap();
}

/// Runs the three acceptance commands for one patterns directory and asserts
/// their outcomes.
fn assert_nonregular_is_skipped(config: &TempDir, data: &TempDir, cwd: &TempDir, patterns: &Path) {
    let (status, stdout, stderr) = run_oo(config, data, cwd, &["echo", "hi"]);
    assert_eq!(status.code(), Some(0), "oo echo hi failed: {stderr}");
    assert_eq!(stdout, "hi\n");

    let (status, stdout, stderr) = run_oo(config, data, cwd, &["patterns"]);
    assert_eq!(status.code(), Some(0), "oo patterns failed: {stderr}");
    let user_header = format!("User ({}):\n", patterns.display());
    let tail = stdout
        .split(&user_header)
        .nth(1)
        .expect("User header present");
    assert!(
        !tail.contains("no learned patterns yet"),
        "valid pattern should be listed, got: {stdout}"
    );

    let (status, stdout, stderr) = run_oo(config, data, cwd, &["rewrite", "pytest -q"]);
    assert_eq!(status.code(), Some(0), "oo rewrite failed: {stderr}");
    assert_eq!(stdout.trim(), "oo pytest -q", "stdout: {stdout}");
}

#[test]
fn user_stray_toml_entries_are_skipped() {
    let config = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let patterns = config.path().join("patterns");
    std::fs::create_dir_all(&patterns).unwrap();
    create_all_strays(&patterns);

    assert_nonregular_is_skipped(&config, &data, &cwd, &patterns);
}

#[test]
fn project_stray_toml_entries_are_skipped() {
    let config = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    // A directory with a `.git` marker is treated as a git root, so
    // `oo` reads patterns from `<cwd>/.oo/patterns`.
    std::fs::create_dir_all(cwd.path().join(".git")).unwrap();
    let patterns = cwd.path().join(".oo").join("patterns");
    std::fs::create_dir_all(&patterns).unwrap();
    create_all_strays(&patterns);

    let (status, stdout, stderr) = run_oo(&config, &data, &cwd, &["echo", "hi"]);
    assert_eq!(status.code(), Some(0), "oo echo hi failed: {stderr}");
    assert_eq!(stdout, "hi\n");

    let (status, _, stderr) = run_oo(&config, &data, &cwd, &["patterns"]);
    assert_eq!(status.code(), Some(0), "oo patterns failed: {stderr}");
}

#[test]
fn symlink_loop_is_reported_by_patterns_but_skipped_by_loader() {
    let config = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let patterns = config.path().join("patterns");
    std::fs::create_dir_all(&patterns).unwrap();
    let loop_link = patterns.join("loop.toml");
    symlink(&loop_link, &loop_link).unwrap();

    let (status, stdout, stderr) = run_oo(&config, &data, &cwd, &["echo", "hi"]);
    assert_eq!(status.code(), Some(0), "oo echo hi failed: {stderr}");
    assert_eq!(stdout, "hi\n");
    assert!(stderr.is_empty(), "loader must stay silent: {stderr}");

    let (status, _, stderr) = run_oo(&config, &data, &cwd, &["patterns"]);
    assert_eq!(
        status.code(),
        Some(1),
        "oo patterns must report the loop: {stderr}"
    );
    assert!(
        stderr.starts_with(&format!("oo: {}: ", loop_link.display())),
        "stderr: {stderr}"
    );
}

#[test]
fn valid_pattern_beside_stray_fifo_is_still_applied() {
    let config = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let patterns = config.path().join("patterns");
    std::fs::create_dir_all(&patterns).unwrap();

    // A user pattern with a failure strategy that greps a line: on a failed
    // command the display shows exactly the filtered line (with a `✗` status
    // line), so the pattern's effect is visible in the output.
    std::fs::write(
        patterns.join("false.toml"),
        "command_match = \"^false\"\n\n[failure]\nstrategy = \"grep\"\ngrep = \"filtered-line\"\n",
    )
    .unwrap();
    // A stray FIFO next to it.
    let fifo = patterns.join("fifo.toml");
    // SAFETY: a valid null-terminated path and zero mode; the call is `mkfifo(2)`.
    let rc = unsafe {
        libc::mkfifo(
            fifo.to_str().unwrap().as_ptr().cast::<libc::c_char>(),
            0o600,
        )
    };
    assert_eq!(rc, 0, "mkfifo: {}", std::io::Error::last_os_error());

    let (status, stdout, stderr) = run_oo(
        &config,
        &data,
        &cwd,
        &["sh", "-c", "echo filtered-line; false"],
    );
    assert_eq!(
        status.code(),
        Some(1),
        "sh -c ... failed unexpectedly: {stderr} {stdout}"
    );
    assert!(
        stdout.contains("filtered-line"),
        "pattern failure strategy should filter the output, stdout: {stdout}"
    );
    assert!(
        stdout.contains('✗'),
        "pattern-matched failure run should show the ✗ line, stdout: {stdout}"
    );
}

/// Spawns `oo` with an isolated config/data dir, feeds `stdin` to the
/// child, waits at most `DEADLINE`, and fails the test if the child was
/// still running.
fn run_oo_with_stdin(
    config: &TempDir,
    data: &TempDir,
    cwd: &TempDir,
    args: &[&str],
    stdin: &str,
) -> (ExitStatus, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_oo"))
        .args(args)
        .current_dir(cwd.path())
        .env("OO_CONFIG_DIR", config.path())
        .env("OO_DATA_DIR", data.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .expect("stdin is piped")
        .write_all(stdin.as_bytes())
        .unwrap();
    // Drop the parent's stdin handle so the child sees EOF and can finish.
    drop(child.stdin.take());

    let deadline = Instant::now() + DEADLINE;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    // Regression: the stray entry blocked `oo`. Kill and reap.
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!(
                        "oo {args:?} did not exit within {DEADLINE:?} — the non-regular entry was not skipped"
                    );
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(_) => panic!("failed to wait on oo {args:?}"),
        }
    };

    let stdout = read_pipe(&mut child.stdout);
    let stderr = read_pipe(&mut child.stderr);
    (status, stdout, stderr)
}

/// `oo hook claude` is the motivating trigger in issue #214: a fresh process
/// per Bash tool call, so a stray FIFO in the user patterns dir would stall
/// the agent. The hook shares the `PATTERNS` LazyLock with `oo rewrite`
/// (src/hook.rs), but this exercises the hook's own entry point with a
/// spawned binary so a regression that changes hook-path pattern loading is
/// caught.
#[test]
fn hook_claude_with_stray_fifo_still_rewrites() {
    let config = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let patterns = config.path().join("patterns");
    std::fs::create_dir_all(&patterns).unwrap();

    // A user pattern for `pytest` next to a stray FIFO — the pattern must
    // still apply while the FIFO is skipped without being opened.
    std::fs::write(patterns.join("pytest.toml"), "command_match = \"pytest\"\n").unwrap();
    let fifo = patterns.join("fifo.toml");
    // SAFETY: a valid null-terminated path and zero mode; the call is `mkfifo(2)`.
    let rc = unsafe {
        libc::mkfifo(
            fifo.to_str().unwrap().as_ptr().cast::<libc::c_char>(),
            0o600,
        )
    };
    assert_eq!(rc, 0, "mkfifo: {}", std::io::Error::last_os_error());

    // PreToolUse JSON for a Bash tool call (same shape as `hook_tests.rs`).
    let stdin = "{\"tool_name\":\"Bash\",\"tool_input\":{\"command\":\"pytest -q\"}}";
    let (status, stdout, stderr) =
        run_oo_with_stdin(&config, &data, &cwd, &["hook", "claude"], stdin);
    assert_eq!(status.code(), Some(0), "hook claude failed: {stderr}");

    let v: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("hook output must be valid JSON: {stdout}");
    assert_eq!(
        v["hookSpecificOutput"]["hookEventName"], "PreToolUse",
        "hookSpecificOutput: {stdout}"
    );
    assert_eq!(
        v["hookSpecificOutput"]["updatedInput"]["command"].as_str(),
        Some("oo pytest -q"),
        "user pattern must rewrite through the hook: {stdout}"
    );
}

/// Restores a file's mode on drop so the temp dir can be cleaned up.
struct RestoreMode(PathBuf);

impl Drop for RestoreMode {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.0, PermissionsExt::from_mode(0o600));
    }
}

#[test]
fn unreadable_regular_toml_is_skipped_silently_but_reported_by_patterns() {
    let config = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let patterns = config.path().join("patterns");
    std::fs::create_dir_all(&patterns).unwrap();
    let entry = patterns.join("locked.toml");
    std::fs::write(&entry, VALID_TOML).unwrap();
    std::fs::set_permissions(&entry, PermissionsExt::from_mode(0o000)).unwrap();
    let _restore = RestoreMode(entry.clone());
    if std::fs::read_to_string(&entry).is_ok() {
        // Running as root: mode bits do not restrict reads, so there is nothing to test.
        return;
    }

    let (status, stdout, stderr) = run_oo(&config, &data, &cwd, &["echo", "hi"]);
    assert_eq!(status.code(), Some(0), "oo echo hi failed: {stderr}");
    assert_eq!(stdout, "hi\n");
    assert!(stderr.is_empty(), "loader must stay silent: {stderr}");

    let (status, _, stderr) = run_oo(&config, &data, &cwd, &["patterns"]);
    assert_eq!(
        status.code(),
        Some(1),
        "oo patterns must report the read error"
    );
    assert!(
        stderr.starts_with(&format!("oo: {}: ", entry.display())),
        "stderr: {stderr}"
    );
}
