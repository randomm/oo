//! Permissions of the SQLite store as created by the `oo` binary.
//!
//! Each run sets the umask explicitly through `sh -c 'umask …'` so the result
//! does not depend on the host shell. Data and home directories are fresh temp
//! dirs, so nothing touches the real `~/.oo`.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

fn mode_of(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn set_mode(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

/// Run `oo <args>` in a shell with the given umask, against `data_dir`.
fn oo_under_umask(umask: &str, data_dir: &Path, home: &Path, args: &[&str]) -> Output {
    Command::new("sh")
        .arg("-c")
        .arg(format!("umask {umask}; exec \"$0\" \"$@\""))
        .arg(env!("CARGO_BIN_EXE_oo"))
        .args(args)
        .env("OO_DATA_DIR", data_dir)
        .env("HOME", home)
        .env_remove("OO_CONFIG_DIR")
        .output()
        .expect("failed to run sh")
}

/// Output large enough to be indexed (above the 4 KiB display threshold).
const BIG_OUTPUT_CMD: &str =
    "i=0; while [ $i -lt 2000 ]; do echo \"line $i padding padding padding\"; i=$((i+1)); done";

#[test]
fn fresh_data_dir_is_private_under_umask_022() {
    let tmp = TempDir::new().unwrap();
    let data = tmp.path().join("data");
    let home = tmp.path().join("home");
    fs::create_dir(&home).unwrap();

    oo_under_umask("022", &data, &home, &["recall", "nothing"]);

    assert_eq!(mode_of(&data), 0o700);
    assert_eq!(mode_of(&data.join("oo.db")), 0o600);
}

#[test]
fn fresh_data_dir_is_private_under_umask_077() {
    let tmp = TempDir::new().unwrap();
    let data = tmp.path().join("data");
    let home = tmp.path().join("home");
    fs::create_dir(&home).unwrap();

    oo_under_umask("077", &data, &home, &["recall", "nothing"]);

    assert_eq!(mode_of(&data), 0o700);
    assert_eq!(mode_of(&data.join("oo.db")), 0o600);
}

#[test]
fn indexed_output_keeps_database_private_under_umask_022() {
    let tmp = TempDir::new().unwrap();
    let data = tmp.path().join("data");
    let home = tmp.path().join("home");
    fs::create_dir(&home).unwrap();

    oo_under_umask("022", &data, &home, &["sh", "-c", BIG_OUTPUT_CMD]);

    assert_eq!(mode_of(&data), 0o700);
    assert_eq!(mode_of(&data.join("oo.db")), 0o600);
    // The store does not enable WAL, so SQLite creates no -wal/-shm files; the
    // rollback journal exists only mid-transaction and its mode is asserted
    // in-process (store_perm_tests::test_journal_created_during_write_is_private).
    let conn = rusqlite::Connection::open(data.join("oo.db")).unwrap();
    let journal_mode: String = conn
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .unwrap();
    assert_eq!(journal_mode, "delete");
    drop(conn);
    for sidecar in ["oo.db-wal", "oo.db-shm"] {
        assert!(!data.join(sidecar).exists(), "{sidecar} must not exist");
    }
}

#[test]
fn existing_world_readable_database_is_tightened() {
    let tmp = TempDir::new().unwrap();
    let data = tmp.path().join("data");
    let home = tmp.path().join("home");
    fs::create_dir_all(&data).unwrap();
    fs::create_dir(&home).unwrap();
    let db = data.join("oo.db");
    fs::write(&db, b"").unwrap();
    set_mode(&db, 0o644);

    oo_under_umask("022", &data, &home, &["recall", "nothing"]);

    assert_eq!(mode_of(&db), 0o600);
}

#[test]
fn existing_data_dir_keeps_its_mode() {
    let tmp = TempDir::new().unwrap();
    let data = tmp.path().join("data");
    let home = tmp.path().join("home");
    fs::create_dir(&data).unwrap();
    fs::create_dir(&home).unwrap();
    set_mode(&data, 0o755);

    oo_under_umask("022", &data, &home, &["recall", "nothing"]);

    assert_eq!(
        mode_of(&data),
        0o755,
        "a directory oo did not create is left alone"
    );
    assert_eq!(mode_of(&data.join("oo.db")), 0o600);
}
