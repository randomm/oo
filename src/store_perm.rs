// File modes for the SQLite store.
//
// The database holds captured command output, which can contain secrets. oo
// therefore keeps a data directory it creates at 0700 and `oo.db` plus its
// SQLite sidecars at 0600, whatever the process umask. Every step here is best
// effort: a chmod or create failure never fails `open_at`, and nothing is
// printed.

use std::fs;
#[cfg(unix)]
use std::fs::OpenOptions;
use std::io;
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;

/// Create `dir` if it is missing, returning any error other than "already exists".
///
/// Only the final component is created with mode 0700; parents come from
/// `create_dir_all` under the process umask. A directory that already exists
/// keeps its mode, so a user-chosen `OO_DATA_DIR` is never changed.
pub(super) fn create_data_dir(dir: &Path) -> io::Result<()> {
    if dir.as_os_str().is_empty() || dir.is_dir() {
        return Ok(());
    }
    if let Some(parent) = dir.parent() {
        fs::create_dir_all(parent)?;
    }
    match make_private_dir(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists && dir.is_dir() => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(unix)]
fn make_private_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new().mode(0o700).create(dir)
}

#[cfg(not(unix))]
fn make_private_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir(dir)
}

/// Make `db` and its sidecars private before SQLite opens them.
///
/// `oo.db` is created with mode 0600 first, because SQLite gives new `-wal`,
/// `-shm` and `-journal` files the mode of the main file. Existing files that
/// oo's user owns are then tightened, which covers leftovers from older builds.
#[cfg(unix)]
pub(super) fn prepare_db_files(db: &Path) {
    use std::os::unix::fs::OpenOptionsExt;
    let _ = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(db);
    tighten(db);
    for suffix in SIDECAR_SUFFIXES {
        tighten(&sidecar(db, suffix));
    }
}

#[cfg(not(unix))]
pub(super) fn prepare_db_files(_db: &Path) {}

#[cfg(unix)]
const SIDECAR_SUFFIXES: [&str; 3] = ["-wal", "-shm", "-journal"];

#[cfg(unix)]
fn sidecar(db: &Path, suffix: &str) -> PathBuf {
    let mut name = db.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Drop group and other permission bits from a regular file the current user owns.
#[cfg(unix)]
fn tighten(path: &Path) {
    // SAFETY: geteuid takes no arguments, has no preconditions and cannot fail.
    let euid = unsafe { libc::geteuid() };
    tighten_as(path, euid);
}

/// `tighten` for an explicit effective uid, so the owner check is testable.
///
/// Symlinks are never followed: a link to a file elsewhere is left untouched.
/// Files owned by someone else are left alone, and a file that is already
/// private (no group or other bits) is never rewritten.
#[cfg(unix)]
pub(super) fn tighten_as(path: &Path, euid: u32) {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let Ok(meta) = fs::symlink_metadata(path) else {
        return;
    };
    if !meta.file_type().is_file() || meta.uid() != euid {
        return;
    }
    let mode = meta.mode() & 0o777;
    if mode & 0o077 == 0 {
        return;
    }
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode & 0o600));
}
