//! End-to-end checks that an empty or relative `OO_CONFIG_DIR` / `OO_DATA_DIR`
//! never makes files in the current directory act as oo's configuration, status
//! or database. Each case runs the `oo` binary from a temp directory that holds
//! repository-style files, so a cwd-relative resolution would be visible.

use std::path::Path;
use tempfile::TempDir;

const STATUS_LINE: &str = "FAILED repo-fake: planted by the repository\n";

/// A temp directory laid out like a repository that ships oo config files.
fn repo_with_planted_files() -> TempDir {
    let repo = TempDir::new().unwrap();
    std::fs::write(repo.path().join("config.toml"), "[store]\nretention = 0\n").unwrap();
    std::fs::write(repo.path().join("learn-status.log"), STATUS_LINE).unwrap();
    repo
}

fn oo(cwd: &Path) -> assert_cmd::Command {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("oo");
    cmd.current_dir(cwd);
    cmd
}

#[test]
fn empty_config_dir_does_not_read_or_clear_repo_status_file() {
    let repo = repo_with_planted_files();
    let out = oo(repo.path())
        .arg("version")
        .env("OO_CONFIG_DIR", "")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(
        repo.path().join("learn-status.log").exists(),
        "repo learn-status.log must not be consumed"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("planted by the repository"), "{stderr}");
}

#[test]
fn relative_config_dir_does_not_read_repo_status_file() {
    let repo = repo_with_planted_files();
    let out = oo(repo.path())
        .arg("version")
        .env("OO_CONFIG_DIR", "relative-cfg")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(repo.path().join("learn-status.log").exists());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("planted by the repository"), "{stderr}");
}

#[test]
fn relative_config_dir_warns_once_per_process() {
    let repo = TempDir::new().unwrap();
    // Indexing output reads the config directory from several places in one run.
    let out = oo(repo.path())
        .args(["seq", "1", "3000"])
        .env("OO_CONFIG_DIR", "relative-cfg")
        .env("OO_DATA_DIR", repo.path().join("data"))
        .output()
        .unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        stderr.matches("ignoring OO_CONFIG_DIR").count(),
        1,
        "{stderr}"
    );
}

#[test]
fn empty_data_dir_does_not_create_database_in_cwd() {
    let repo = TempDir::new().unwrap();
    let cfg = TempDir::new().unwrap();
    let out = oo(repo.path())
        .args(["seq", "1", "3000"])
        .env("OO_CONFIG_DIR", cfg.path())
        .env("OO_DATA_DIR", "")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(!repo.path().join("oo.db").exists(), "oo.db created in cwd");
}

#[test]
fn relative_data_dir_does_not_create_database_in_cwd() {
    let repo = TempDir::new().unwrap();
    let cfg = TempDir::new().unwrap();
    let out = oo(repo.path())
        .args(["seq", "1", "3000"])
        .env("OO_CONFIG_DIR", cfg.path())
        .env("OO_DATA_DIR", "rel-data")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(!repo.path().join("rel-data").join("oo.db").exists());
    assert!(!repo.path().join("oo.db").exists());
}

#[test]
fn absolute_data_dir_still_holds_database() {
    let repo = TempDir::new().unwrap();
    let cfg = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let out = oo(repo.path())
        .args(["seq", "1", "3000"])
        .env("OO_CONFIG_DIR", cfg.path())
        .env("OO_DATA_DIR", data.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(data.path().join("oo.db").exists());
    assert!(!String::from_utf8_lossy(&out.stderr).contains("ignoring OO_DATA_DIR"));
}
