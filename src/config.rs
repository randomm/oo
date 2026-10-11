//! Location of oo's config directory, shared by `learn` and `retention`.

use std::path::PathBuf;

/// `OO_CONFIG_DIR` when set, otherwise `<platform config dir>/oo`.
pub(crate) fn config_dir() -> PathBuf {
    if let Some(test_dir) = std::env::var_os("OO_CONFIG_DIR") {
        return PathBuf::from(test_dir);
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("oo")
}
