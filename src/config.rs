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

#[cfg(test)]
mod tests {
    use super::*;

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
