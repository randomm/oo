//! Unit tests for `is_pattern_file` and `load_user_patterns` on a directory
//! mixing regular files with non-regular `*.toml` entries.

use super::load_user_patterns;
use crate::pattern::is_pattern_file;
use std::os::unix::fs::symlink;
use std::path::Path;
use tempfile::TempDir;

const VALID_TOML: &str = "command_match = \"^valid\"\n";

/// Creates the named non-regular entries in `dir`: a FIFO, a directory, a
/// dangling symlink, all named `*.toml`. The FIFO is only stat'ed, never
/// opened, so the tests cannot hang.
fn create_strays(dir: &Path) {
    let fifo = dir.join("stray-fifo.toml");
    // SAFETY: a valid null-terminated path and zero mode; the call is `mkfifo(2)`.
    let rc = unsafe {
        libc::mkfifo(
            fifo.to_str().unwrap().as_ptr().cast::<libc::c_char>(),
            0o600,
        )
    };
    assert_eq!(
        rc,
        0,
        "mkfifo({}): {}",
        fifo.display(),
        std::io::Error::last_os_error()
    );

    let sub = dir.join("stray-dir.toml");
    std::fs::create_dir(&sub).unwrap();

    let link = dir.join("stray-dangling.toml");
    symlink(dir.join("missing-target.toml"), &link).unwrap();
}

#[test]
fn is_pattern_file_classifies_regular_and_nonregular_entries() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    let regular = path.join("regular.toml");
    std::fs::write(&regular, VALID_TOML).unwrap();
    assert!(is_pattern_file(&regular).unwrap(), "regular file");

    let target = path.join("target.txt");
    std::fs::write(&target, "not toml").unwrap();
    let to_regular = path.join("symlink-to-regular.toml");
    symlink(&target, &to_regular).unwrap();
    assert!(is_pattern_file(&to_regular).unwrap(), "symlink to regular");

    let fifo = path.join("fifo.toml");
    // SAFETY: a valid null-terminated path and zero mode; the call is `mkfifo(2)`.
    let rc = unsafe {
        libc::mkfifo(
            fifo.to_str().unwrap().as_ptr().cast::<libc::c_char>(),
            0o600,
        )
    };
    assert_eq!(rc, 0, "mkfifo: {}", std::io::Error::last_os_error());
    assert!(!is_pattern_file(&fifo).unwrap(), "fifo");

    let subdir = path.join("subdir.toml");
    std::fs::create_dir(&subdir).unwrap();
    assert!(!is_pattern_file(&subdir).unwrap(), "directory");

    let dangling = path.join("dangling.toml");
    symlink(path.join("missing.toml"), &dangling).unwrap();
    assert!(!is_pattern_file(&dangling).unwrap(), "dangling symlink");

    let loop_link = path.join("loop.toml");
    symlink(&loop_link, &loop_link).unwrap();
    // ELOOP must surface as an error, not Ok(false): `oo patterns` reports it.
    assert!(
        is_pattern_file(&loop_link).is_err(),
        "symlink loop must be an Err, not a skip"
    );
}

#[test]
fn load_user_patterns_skips_nonregular_entries() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    std::fs::write(path.join("valid.toml"), VALID_TOML).unwrap();
    create_strays(&path);

    let patterns = load_user_patterns(dir.path());
    assert_eq!(patterns.len(), 1, "expected exactly one pattern");
    assert_eq!(patterns[0].command_match.as_str(), "^valid");
}

#[test]
fn load_user_patterns_ignores_invalid_toml_content() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("good.toml"), VALID_TOML).unwrap();
    std::fs::write(dir.path().join("bad.toml"), "not toml =").unwrap();

    let patterns = load_user_patterns(dir.path());
    assert_eq!(patterns.len(), 1);
}
