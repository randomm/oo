//! The temp-file payload that `spawn_background` hands to the `_learn_bg` child.
//!
//! The payload can hold command output (possibly secret), so the child deletes
//! it as soon as it has read it. The parent passes the directory it created the
//! file in, so the child's name check does not depend on its environment.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Name prefix of a payload file (the `tempfile` default, made explicit so the
/// producer and the consumer share one shape).
pub const PAYLOAD_PREFIX: &str = ".tmp";

/// Number of random alphanumeric characters after [`PAYLOAD_PREFIX`].
pub const PAYLOAD_RAND_LEN: usize = 6;

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

/// Create a payload holding `data` directly in `dir` with the shape
/// [`is_payload_shaped`] accepts. The file is removed when the returned
/// `TempPath` drops unless it is kept.
pub fn create_payload(dir: &Path, data: &str) -> std::io::Result<tempfile::TempPath> {
    let mut tmp = tempfile::Builder::new()
        .prefix(PAYLOAD_PREFIX)
        .rand_bytes(PAYLOAD_RAND_LEN)
        .tempfile_in(dir)?;
    tmp.write_all(data.as_bytes())?;
    // Closes the handle but keeps the file until the TempPath drops.
    Ok(tmp.into_temp_path())
}

/// Only a file with the exact tempfile name shape directly inside `dir` (the
/// directory given on argv) is read or removed: `_learn_bg` takes its path from
/// argv, so any other path is left alone. This checks name and directory only;
/// argv is untrusted, so it does not prove this parent created the file.
pub fn is_payload_shaped(path: &Path, dir: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    path.parent() == Some(dir)
        && name.strip_prefix(PAYLOAD_PREFIX).is_some_and(|rand| {
            rand.len() == PAYLOAD_RAND_LEN && rand.bytes().all(|b| b.is_ascii_alphanumeric())
        })
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
    fn payload_from_producer_is_accepted_by_consumer() {
        let dir = tempfile::TempDir::new().unwrap();
        let payload = create_payload(dir.path(), "{}").unwrap();
        assert!(is_payload_shaped(&payload, dir.path()));
    }

    #[test]
    fn is_payload_shaped_accepts_tmp_file_in_payload_dir() {
        let dir = PathBuf::from("/some/tmp");
        assert!(is_payload_shaped(&dir.join(".tmpAbC123"), &dir));
    }

    #[test]
    fn is_payload_shaped_rejects_other_names_and_dirs() {
        let dir = PathBuf::from("/some/tmp");
        assert!(!is_payload_shaped(&dir.join("payload.json"), &dir));
        assert!(!is_payload_shaped(&dir.join(".tmpBadJson"), &dir));
        assert!(!is_payload_shaped(
            &PathBuf::from("/elsewhere/.tmpAbC123"),
            &dir
        ));
        assert!(!is_payload_shaped(
            &dir.join("nested").join(".tmpAbC123"),
            &dir
        ));
    }
}
