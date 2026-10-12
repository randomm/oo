//! Location of oo's config directory, shared by `learn` and `retention`.

use std::ffi::OsString;
use std::path::PathBuf;

/// `OO_CONFIG_DIR` when set, otherwise `<platform config dir>/oo`.
pub(crate) fn config_dir() -> PathBuf {
    config_dir_from(std::env::var_os("OO_CONFIG_DIR"))
}

/// Pure form of [`config_dir`] taking the `OO_CONFIG_DIR` value explicitly,
/// so callers and tests never need to mutate process-global environment.
pub(crate) fn config_dir_from(override_dir: Option<OsString>) -> PathBuf {
    if let Some(dir) = override_dir {
        return PathBuf::from(dir);
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("oo")
}

/// `dir` when it is absolute, otherwise `/tmp`. An empty or relative `TMPDIR`
/// would make `tempfile` create files in the current directory.
pub(crate) fn absolute_temp_dir(dir: PathBuf) -> PathBuf {
    if dir.is_absolute() {
        dir
    } else {
        PathBuf::from("/tmp")
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
    fn override_is_used_verbatim_without_oo_suffix() {
        let dir = PathBuf::from("/some/override");
        assert_eq!(config_dir_from(Some(dir.clone().into())), dir);
    }

    #[test]
    fn unset_override_appends_oo_to_platform_dir() {
        let expected = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("oo");
        assert_eq!(config_dir_from(None), expected);
    }
}
