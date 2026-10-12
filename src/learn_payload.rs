//! The temp-file payload that `spawn_background` hands to the `_learn_bg` child.
//!
//! The payload can hold command output (possibly secret), so the child deletes
//! it as soon as it has read it. The parent passes the directory it created the
//! file in, so the child's ownership check does not depend on its environment.

use std::path::{Path, PathBuf};

/// Directory for the background payload file.
pub fn payload_dir() -> PathBuf {
    absolute_temp_dir(std::env::temp_dir())
}

/// `dir` when it is absolute, otherwise `/tmp`. An empty or relative `TMPDIR`
/// would make `tempfile` create files in the current directory.
fn absolute_temp_dir(dir: PathBuf) -> PathBuf {
    if dir.is_absolute() {
        dir
    } else {
        PathBuf::from("/tmp")
    }
}

/// Only a file that `spawn_background` created directly in `dir`, with the exact
/// name `tempfile` gives it (`.tmp` plus six alphanumerics), may be deleted:
/// `_learn_bg` takes its path from argv, so any other path is left alone.
pub fn is_owned_payload(path: &Path, dir: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    path.parent() == Some(dir)
        && name
            .strip_prefix(".tmp")
            .is_some_and(|rand| rand.len() == 6 && rand.bytes().all(|b| b.is_ascii_alphanumeric()))
}

/// Delete the payload. A file that is already gone is not an error.
pub fn remove_payload(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_temp_dir_falls_back_to_tmp_when_empty_or_relative() {
        assert_eq!(absolute_temp_dir(PathBuf::new()), PathBuf::from("/tmp"));
        assert_eq!(
            absolute_temp_dir(PathBuf::from("relative/dir")),
            PathBuf::from("/tmp")
        );
    }

    #[test]
    fn absolute_temp_dir_keeps_absolute_dir() {
        let dir = PathBuf::from("/some/abs/tmp");
        assert_eq!(absolute_temp_dir(dir.clone()), dir);
    }

    #[test]
    fn is_owned_payload_accepts_tmp_file_in_payload_dir() {
        let dir = PathBuf::from("/some/tmp");
        assert!(is_owned_payload(&dir.join(".tmpAbC123"), &dir));
    }

    #[test]
    fn is_owned_payload_rejects_other_names_and_dirs() {
        let dir = PathBuf::from("/some/tmp");
        assert!(!is_owned_payload(&dir.join("payload.json"), &dir));
        assert!(!is_owned_payload(&dir.join(".tmpBadJson"), &dir));
        assert!(!is_owned_payload(
            &PathBuf::from("/elsewhere/.tmpAbC123"),
            &dir
        ));
        assert!(!is_owned_payload(
            &dir.join("nested").join(".tmpAbC123"),
            &dir
        ));
    }
}
