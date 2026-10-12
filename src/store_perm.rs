// File modes for the SQLite store.
//
// The database holds captured command output, which can contain secrets. oo
// therefore keeps a data directory it creates at 0700 and `oo.db` plus its
// SQLite sidecars at 0600 (the umask can only remove bits from those). Every
// step here is best effort: a chmod or create failure never fails `open_at`,
// and nothing is printed.

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

/// Tighten a file that `tighten_as` accepts (see there) to 0600 or its owner bits.
#[cfg(unix)]
fn tighten(path: &Path) {
    // SAFETY: geteuid takes no arguments, has no preconditions and cannot fail.
    let euid = unsafe { libc::geteuid() };
    tighten_as(path, euid);
}

/// `tighten` for an explicit effective uid, so the owner check is testable.
///
/// Applies to a regular file owned by `euid`. Group and other bits are removed,
/// and the owner keeps read and write (mode 0600). A file whose owner lacks
/// read or write keeps its owner bits instead. Symlinks are never followed, and
/// a file owned by someone else or already without group/other bits is left
/// alone. A file the caller cannot open for reading is skipped.
///
/// The file is opened with `O_NOFOLLOW` and the mode is changed through the
/// handle (`fchmod`), so a path swapped for a symlink between the check and the
/// chmod cannot redirect the change. `O_NONBLOCK` stops a swapped-in FIFO from
/// blocking the open; the metadata check then rejects it.
#[cfg(unix)]
pub(super) fn tighten_as(path: &Path, euid: u32) {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
    let Ok(file) = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    else {
        return;
    };
    let Ok(meta) = file.metadata() else {
        return;
    };
    if !meta.file_type().is_file() || meta.uid() != euid {
        return;
    }
    let mode = meta.mode() & 0o777;
    if mode & 0o077 == 0 {
        return;
    }
    // SQLite needs owner read+write only, so the target is 0600 (a 0755 file
    // becomes 0600, not 0700). An owner without read+write keeps its own bits.
    let owner = mode & 0o700;
    let target = if owner & 0o600 == 0o600 { 0o600 } else { owner };
    let _ = file.set_permissions(fs::Permissions::from_mode(target));
}
