// Unit tests for the store's file modes. These run in-process, so they never
// change the umask (that would race across test threads); they assert modes
// that hold under any common umask (022 or 077), because the umask can only
// remove bits. Umask-specific behaviour is covered by tests/store_permissions.rs.

use super::SqliteStore;
use super::store_perm::{create_data_dir, prepare_db_files, tighten_as};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

#[cfg(unix)]
fn mode_of(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

#[cfg(unix)]
#[test]
fn test_create_data_dir_makes_leaf_private_and_creates_missing_parents() {
    let tmp = TempDir::new().unwrap();
    let parent = tmp.path().join("outer");
    set_mode(tmp.path(), 0o755);
    let dir = parent.join("oo");

    create_data_dir(&dir).unwrap();

    assert_eq!(mode_of(&dir), 0o700);
    assert!(parent.is_dir(), "missing parents must still be created");
}

#[cfg(unix)]
#[test]
fn test_create_data_dir_keeps_mode_of_existing_directory() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("existing");
    fs::create_dir(&dir).unwrap();
    set_mode(&dir, 0o755);

    create_data_dir(&dir).unwrap();

    assert_eq!(
        mode_of(&dir),
        0o755,
        "a pre-existing directory is not chmod-ed"
    );
}

#[cfg(unix)]
#[test]
fn test_open_at_creates_private_dir_and_db_file() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join(".oo");
    let db = dir.join("oo.db");

    let _store = SqliteStore::open_at(&db).unwrap();

    assert_eq!(mode_of(&dir), 0o700);
    assert_eq!(mode_of(&db), 0o600);
}

#[cfg(unix)]
#[test]
fn test_open_at_tightens_existing_world_readable_db() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("oo.db");
    fs::write(&db, b"").unwrap();
    set_mode(&db, 0o644);

    let _store = SqliteStore::open_at(&db).unwrap();

    assert_eq!(mode_of(&db), 0o600);
}

#[cfg(unix)]
#[test]
fn test_prepare_db_files_tightens_leftover_sidecars() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("oo.db");
    fs::write(&db, b"").unwrap();
    let wal = tmp.path().join("oo.db-wal");
    let shm = tmp.path().join("oo.db-shm");
    fs::write(&wal, b"").unwrap();
    fs::write(&shm, b"").unwrap();
    set_mode(&db, 0o644);
    set_mode(&wal, 0o644);
    set_mode(&shm, 0o644);

    prepare_db_files(&db);

    assert_eq!(mode_of(&db), 0o600);
    assert_eq!(mode_of(&wal), 0o600);
    assert_eq!(mode_of(&shm), 0o600);
}

#[cfg(unix)]
#[test]
fn test_prepare_db_files_leaves_already_private_file_alone() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("oo.db");
    fs::write(&db, b"").unwrap();
    set_mode(&db, 0o400);

    prepare_db_files(&db);

    assert_eq!(mode_of(&db), 0o400, "owner-only modes are not rewritten");
}

#[cfg(unix)]
#[test]
fn test_prepare_db_files_does_not_truncate_existing_db() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("oo.db");
    fs::write(&db, b"keep me").unwrap();

    prepare_db_files(&db);

    assert_eq!(fs::read(&db).unwrap(), b"keep me");
}

#[cfg(unix)]
#[test]
fn test_tighten_as_leaves_file_owned_by_another_user_alone() {
    use std::os::unix::fs::MetadataExt;
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("oo.db");
    fs::write(&db, b"").unwrap();
    set_mode(&db, 0o644);
    let owner = fs::metadata(&db).unwrap().uid();

    tighten_as(&db, owner.wrapping_add(1));

    assert_eq!(
        mode_of(&db),
        0o644,
        "a file owned by another uid is not chmod-ed"
    );
    tighten_as(&db, owner);
    assert_eq!(
        mode_of(&db),
        0o600,
        "the owning uid does get the tightening"
    );
}

#[cfg(unix)]
#[test]
fn test_wal_sidecars_are_private_while_connection_is_open() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("oo.db");
    let store = SqliteStore::open_at(&db).unwrap();
    store
        .conn
        .pragma_update(None, "journal_mode", "WAL")
        .unwrap();
    store
        .conn
        .execute_batch("CREATE TABLE probe (x INTEGER); INSERT INTO probe VALUES (1);")
        .unwrap();

    // -wal and -shm exist only while the WAL connection is open; SQLite gives
    // them the main file's mode, which open_at has already set to 0600.
    for sidecar in ["oo.db-wal", "oo.db-shm"] {
        let path = tmp.path().join(sidecar);
        assert!(path.exists(), "{sidecar} should exist in WAL mode");
        assert_eq!(mode_of(&path), 0o600, "{sidecar} must be private");
    }
}

#[cfg(unix)]
#[test]
fn test_prepare_db_files_does_not_follow_symlink() {
    use std::os::unix::fs::symlink;
    let tmp = TempDir::new().unwrap();
    let target = tmp.path().join("elsewhere.txt");
    fs::write(&target, b"x").unwrap();
    set_mode(&target, 0o644);
    let db = tmp.path().join("oo.db");
    symlink(&target, &db).unwrap();

    prepare_db_files(&db);

    assert_eq!(
        mode_of(&target),
        0o644,
        "a symlink target is never chmod-ed"
    );
}

#[cfg(unix)]
#[test]
fn test_journal_created_during_write_is_private() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("oo.db");
    let store = SqliteStore::open_at(&db).unwrap();

    // A rollback journal exists only inside an open write transaction. SQLite
    // gives it the main file's mode, so it must be 0600 here.
    store
        .conn
        .execute_batch(
            "BEGIN IMMEDIATE; CREATE TABLE probe (x INTEGER); INSERT INTO probe VALUES (1);",
        )
        .unwrap();
    let journal = tmp.path().join("oo.db-journal");
    assert!(journal.exists(), "journal should exist mid-transaction");
    assert_eq!(mode_of(&journal), 0o600);
    store.conn.execute_batch("COMMIT;").unwrap();
}

#[test]
fn test_prepare_db_files_never_panics_when_paths_cannot_be_changed() {
    // The directory does not exist, so creating and chmod-ing must fail
    // silently rather than abort the open.
    prepare_db_files(Path::new("/nonexistent-oo-perm-test/dir/oo.db"));
}
