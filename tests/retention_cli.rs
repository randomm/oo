//! End-to-end check that `[store] retention` in `config.toml` drives the purge
//! performed by `oo` when it indexes output. Isolation is via `OO_CONFIG_DIR`
//! and `OO_DATA_DIR` on the child process only, so the test process env is untouched.

use assert_cmd::Command;
use double_o::store::{SessionMeta, SqliteStore, Store};
use std::path::Path;
use tempfile::TempDir;

const DAY: i64 = 86_400;

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Seed an entry whose timestamp is `age_secs` in the past, under the project id
/// the CLI child will compute (same cwd, so same project id).
fn seed(data_dir: &Path, marker: &str, age_secs: i64) {
    let mut store = SqliteStore::open_at(&data_dir.join("oo.db")).unwrap();
    let meta = SessionMeta {
        source: "oo".into(),
        session: "test".into(),
        command: "seed".into(),
        timestamp: now() - age_secs,
    };
    store
        .index(&double_o::session::project_id(), marker, &meta)
        .unwrap();
}

fn survives(data_dir: &Path, marker: &str) -> bool {
    let mut store = SqliteStore::open_at(&data_dir.join("oo.db")).unwrap();
    !store
        .search(&double_o::session::project_id(), marker, 10)
        .unwrap()
        .is_empty()
}

/// Run `oo` with the given config (None = no config.toml) and a large output
/// that gets indexed, which triggers the retention purge. Returns stderr.
fn run_index(config: Option<&str>, data_dir: &Path) -> String {
    let cfg_dir = TempDir::new().unwrap();
    if let Some(content) = config {
        std::fs::write(cfg_dir.path().join("config.toml"), content).unwrap();
    }
    let out = Command::cargo_bin("oo")
        .unwrap()
        .args(["seq", "1", "3000"])
        .env("OO_CONFIG_DIR", cfg_dir.path())
        .env("OO_DATA_DIR", data_dir)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn default_retention_keeps_25h_and_6d_purges_8d() {
    let data = TempDir::new().unwrap();
    seed(data.path(), "zqxcliday25", 25 * 3600);
    seed(data.path(), "zqxcliday6", 6 * DAY);
    seed(data.path(), "zqxcliday8", 8 * DAY);
    run_index(None, data.path());
    assert!(survives(data.path(), "zqxcliday25"));
    assert!(survives(data.path(), "zqxcliday6"));
    assert!(!survives(data.path(), "zqxcliday8"));
}

#[test]
fn custom_retention_purges_entry_older_than_it() {
    let data = TempDir::new().unwrap();
    seed(data.path(), "zqxclicustom25", 25 * 3600);
    run_index(Some("[store]\nretention = 86400\n"), data.path());
    assert!(!survives(data.path(), "zqxclicustom25"));
}

#[test]
fn zero_retention_never_purges() {
    let data = TempDir::new().unwrap();
    seed(data.path(), "zqxclizero30", 30 * DAY);
    run_index(Some("[store]\nretention = 0\n"), data.path());
    assert!(survives(data.path(), "zqxclizero30"));
}

#[test]
fn invalid_retention_warns_and_uses_default() {
    let data = TempDir::new().unwrap();
    seed(data.path(), "zqxcliinvalid25", 25 * 3600);
    let stderr = run_index(Some("[store]\nretention = -1\n"), data.path());
    assert!(
        stderr.contains("invalid [store] retention"),
        "stderr: {stderr}"
    );
    assert!(survives(data.path(), "zqxcliinvalid25"));
}
