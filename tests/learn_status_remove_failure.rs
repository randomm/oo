//! Pins the branch of `check_and_clear_learn_status` where the status file is
//! present but cannot be removed: the status text is still printed, the failure
//! is reported on stderr, the file stays in place and the process exits cleanly.
//!
//! Unix only: the test makes the config directory read-only via permission bits.
//! The `oo` binary is spawned (not the function called in-process) because the
//! function writes with `eprintln!`, and stderr is only capturable from a child.

#![cfg(unix)]

use predicates::prelude::*;
use std::path::Path;
use tempfile::TempDir;

/// Restores a directory's mode on drop, so a failing assert cannot strand the TempDir.
struct RestoreMode(std::path::PathBuf);

impl Drop for RestoreMode {
    fn drop(&mut self) {
        set_mode(&self.0, 0o700);
    }
}

fn set_mode(dir: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
fn status_file_that_cannot_be_removed_is_reported_and_kept() {
    let root = TempDir::new().unwrap();
    let cfg = root.path().join("cfg");
    let data = root.path().join("data");
    std::fs::create_dir(&cfg).unwrap();
    std::fs::create_dir(&data).unwrap();
    let status = cfg.join("learn-status.log");
    std::fs::write(&status, "FAILED echo: boom\n").unwrap();

    // Read-only config dir: the status file can be read but not unlinked.
    set_mode(&cfg, 0o500);
    let _restore = RestoreMode(cfg.clone());
    let probe = cfg.join(".write-probe");
    if std::fs::write(&probe, b"").is_ok() {
        // Root ignores mode bits, so the directory stays writable and the lock
        // did not take effect. Skip, as the other permission tests do.
        let _ = std::fs::remove_file(&probe);
        eprintln!(
            "skipped: {} stays writable (running as root?)",
            cfg.display()
        );
        return;
    }

    // Per-child environment only: nothing here touches the real HOME or config.
    assert_cmd::cargo::cargo_bin_cmd!("oo")
        .args(["echo", "hi"])
        .env("OO_CONFIG_DIR", &cfg)
        .env("OO_DATA_DIR", &data)
        .env("HOME", root.path())
        .env("XDG_CONFIG_HOME", root.path().join("xdg-config"))
        .env("XDG_DATA_HOME", root.path().join("xdg-data"))
        .env("XDG_CACHE_HOME", root.path().join("xdg-cache"))
        .env_remove("ANTHROPIC_API_KEY")
        .assert()
        .success()
        .stdout(predicate::str::contains("hi"))
        .stderr(predicate::str::contains("oo: learn failed for echo"))
        .stderr(predicate::str::contains("could not remove learn status"))
        .stderr(predicate::str::contains("panicked").not());

    assert!(status.exists(), "the status file must stay in place");
    assert_eq!(
        std::fs::read_to_string(&status).unwrap(),
        "FAILED echo: boom\n"
    );
}
