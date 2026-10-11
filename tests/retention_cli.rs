//! End-to-end checks that `[store] retention` in `config.toml` drives the purge
//! and warning behaviour of the `oo` binary when it indexes output. Isolation is
//! via `OO_CONFIG_DIR` and `OO_DATA_DIR` on the child process only. Parsing and
//! purge logic are covered by unit tests in `src/retention_tests.rs`.

use double_o::store::{SessionMeta, SqliteStore, Store};
use std::path::Path;
use tempfile::TempDir;

/// Seed an entry whose timestamp is `age_secs` in the past, under the project id
/// the CLI child will compute (same cwd, so same project id).
fn seed(data_dir: &Path, marker: &str, age_secs: i64) {
    let mut store = SqliteStore::open_at(&data_dir.join("oo.db")).unwrap();
    let meta = SessionMeta {
        source: "oo".into(),
        session: "test".into(),
        command: "seed".into(),
        timestamp: double_o::util::now_epoch() - age_secs,
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
/// that gets indexed, which triggers the retention purge. Asserts the child
/// exits successfully and returns stderr.
fn run_index(config: Option<&str>, data_dir: &Path) -> String {
    let cfg_dir = TempDir::new().unwrap();
    if let Some(content) = config {
        std::fs::write(cfg_dir.path().join("config.toml"), content).unwrap();
    }
    let out = assert_cmd::cargo::cargo_bin_cmd!("oo")
        .args(["seq", "1", "3000"])
        .env("OO_CONFIG_DIR", cfg_dir.path())
        .env("OO_DATA_DIR", data_dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "oo exited with {:?}", out.status);
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn custom_retention_purges_entry_older_than_it() {
    let data = TempDir::new().unwrap();
    seed(data.path(), "zqxclicustom25", 25 * 3600);
    run_index(Some("[store]\nretention = 86400\n"), data.path());
    assert!(!survives(data.path(), "zqxclicustom25"));
}

#[test]
fn invalid_retention_warns_and_uses_default() {
    let data = TempDir::new().unwrap();
    seed(data.path(), "zqxcliinvalid25", 25 * 3600);
    let stderr = run_index(Some("[store]\nretention = -1\n"), data.path());
    assert!(stderr.contains("oo: warning:"), "stderr: {stderr}");
    assert!(stderr.contains("config.toml"), "stderr: {stderr}");
    assert!(survives(data.path(), "zqxcliinvalid25"));
}
