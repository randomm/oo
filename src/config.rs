//! Location of oo's directories, shared by `learn`, `retention` and `store`.
//!
//! `OO_CONFIG_DIR` and `OO_DATA_DIR` are the only two places where a
//! user-supplied path is trusted as a base directory. Every reader goes through
//! [`resolve_override`], so a value that is not absolute (or is blank) never
//! resolves against the current directory, where a repository's own files
//! would then act as user configuration.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Environment variable naming the config directory.
pub(crate) const CONFIG_DIR_VAR: &str = "OO_CONFIG_DIR";

/// Environment variable naming the data directory.
pub(crate) const DATA_DIR_VAR: &str = "OO_DATA_DIR";

/// `OO_CONFIG_DIR` when set to an absolute path, otherwise `<platform config dir>/oo`.
pub(crate) fn config_dir() -> PathBuf {
    config_dir_from(std::env::var_os(CONFIG_DIR_VAR))
}

/// Pure form of [`config_dir`] taking the `OO_CONFIG_DIR` value explicitly,
/// so callers and tests never need to mutate process-global environment.
pub(crate) fn config_dir_from(override_dir: Option<OsString>) -> PathBuf {
    resolve_override(CONFIG_DIR_VAR, override_dir).unwrap_or_else(|| {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("oo")
    })
}

/// Read a directory override from the environment. See [`resolve_override`].
pub(crate) fn env_override(name: &'static str) -> Option<PathBuf> {
    resolve_override(name, std::env::var_os(name))
}

/// The single resolution point for `OO_CONFIG_DIR` / `OO_DATA_DIR`.
///
/// Returns `Some(absolute path)` for an absolute value. Unset, empty and
/// whitespace-only values return `None` silently (the platform default
/// applies). Any other value (relative, or padded with spaces such as
/// `" /abs"`) is ignored with a one-time warning on stderr, and `None` is
/// returned, so the platform default applies rather than the current directory.
pub(crate) fn resolve_override(name: &'static str, value: Option<OsString>) -> Option<PathBuf> {
    let value = value?;
    match validate_override(&value) {
        Ok(path) => path,
        Err(reason) => {
            if mark_warned(name) {
                eprintln!(
                    "oo: ignoring {name}={}: {reason}; using the default directory",
                    value.to_string_lossy()
                );
            }
            None
        }
    }
}

/// Classify a raw override value without touching the environment or stderr.
///
/// `Ok(None)` means unset or blank; `Ok(Some(path))` is an absolute path used
/// verbatim (no trimming); `Err(reason)` means the value must be ignored.
pub(crate) fn validate_override(value: &OsStr) -> Result<Option<PathBuf>, &'static str> {
    if value.to_string_lossy().trim().is_empty() {
        return Ok(None);
    }
    let path = Path::new(value);
    if path.is_absolute() {
        Ok(Some(path.to_path_buf()))
    } else {
        Err("it must be an absolute path")
    }
}

/// Record that a warning was emitted for `name`. Returns `true` only the first
/// time per process, so several readers of one variable warn once.
fn mark_warned(name: &'static str) -> bool {
    static WARNED: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    let mut warned = WARNED.lock().unwrap_or_else(|e| e.into_inner());
    if warned.contains(&name) {
        false
    } else {
        warned.push(name);
        true
    }
}

#[cfg(test)]
#[path = "env_dirs_tests.rs"]
mod tests;
