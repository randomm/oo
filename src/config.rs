//! Location of oo's directories, shared by `learn`, `retention` and `store`.
//!
//! `OO_CONFIG_DIR` and `OO_DATA_DIR` are oo's own base-directory overrides. Both
//! are checked by [`validate_override`], which accepts only an absolute path, so
//! a value that is relative (or blank) never resolves against the current
//! directory, where a repository's own files would then act as configuration.
//! `OO_CLAUDE_DIR` and `OO_PI_EXTENSIONS_DIR` follow their own rules.

use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Environment variable naming the config directory.
pub(crate) const CONFIG_DIR_VAR: &str = "OO_CONFIG_DIR";

/// Environment variable naming the data directory.
pub(crate) const DATA_DIR_VAR: &str = "OO_DATA_DIR";

static WARNED_CONFIG: AtomicBool = AtomicBool::new(false);
static WARNED_DATA: AtomicBool = AtomicBool::new(false);

/// A directory override that [`validate_override`] has checked to be absolute.
/// The field is private, so code outside this module cannot build one from an
/// unchecked path.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct AbsDir(PathBuf);

impl AbsDir {
    pub(crate) fn into_path(self) -> PathBuf {
        self.0
    }
}

/// `OO_CONFIG_DIR` when set to an absolute path, otherwise `<platform config dir>/oo`.
pub(crate) fn config_dir() -> PathBuf {
    let value = std::env::var_os(CONFIG_DIR_VAR);
    warn_if_ignored(CONFIG_DIR_VAR, value.as_deref(), &WARNED_CONFIG);
    config_dir_from(value)
}

/// Pure form of [`config_dir`] taking the `OO_CONFIG_DIR` value explicitly.
/// It neither prints nor touches global state, so tests can call it freely.
pub(crate) fn config_dir_from(override_dir: Option<OsString>) -> PathBuf {
    resolve_override(override_dir.as_deref()).unwrap_or_else(|| {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("oo")
    })
}

/// `OO_DATA_DIR` when set to an absolute path, otherwise `None` so the platform
/// data directory applies.
pub(crate) fn data_dir_override() -> Option<PathBuf> {
    let value = std::env::var_os(DATA_DIR_VAR);
    warn_if_ignored(DATA_DIR_VAR, value.as_deref(), &WARNED_DATA);
    resolve_override(value.as_deref())
}

/// Classify a raw override value without touching the environment or stderr.
///
/// `Ok(None)` means unset or blank; `Ok(Some(dir))` is an absolute path used
/// verbatim (no trimming); `Err(reason)` means the value must be ignored.
pub(crate) fn validate_override(value: &OsStr) -> Result<Option<AbsDir>, &'static str> {
    if value.to_string_lossy().trim().is_empty() {
        return Ok(None);
    }
    let path = Path::new(value);
    if path.is_absolute() {
        Ok(Some(AbsDir(path.to_path_buf())))
    } else {
        Err("it must be an absolute path")
    }
}

fn resolve_override(value: Option<&OsStr>) -> Option<PathBuf> {
    validate_override(value?)
        .ok()
        .flatten()
        .map(AbsDir::into_path)
}

/// Warn on stderr once per process when a set value is ignored. The message
/// names the variable and the reason, not the value: the value can come from a
/// per-repository environment and may contain control characters. The write is
/// best-effort so a closed stderr cannot abort the command.
fn warn_if_ignored(name: &'static str, value: Option<&OsStr>, warned: &AtomicBool) {
    let Some(value) = value else { return };
    if let Err(reason) = validate_override(value) {
        if !warned.swap(true, Ordering::Relaxed) {
            let _ = writeln!(
                std::io::stderr(),
                "oo: ignoring {name}: {reason}; using the default directory"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn platform_default() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("oo")
    }

    #[test]
    fn absolute_override_is_used_verbatim_without_oo_suffix() {
        let dir = std::env::temp_dir();
        assert_eq!(config_dir_from(Some(dir.clone().into_os_string())), dir);
    }

    #[test]
    fn unset_override_appends_oo_to_platform_dir() {
        assert_eq!(config_dir_from(None), platform_default());
    }

    #[test]
    fn blank_override_is_treated_as_unset() {
        assert_eq!(validate_override(OsStr::new("")), Ok(None));
        assert_eq!(validate_override(OsStr::new(" \t")), Ok(None));
    }

    #[test]
    fn absolute_override_is_accepted_verbatim() {
        let dir = std::env::temp_dir();
        assert_eq!(
            validate_override(dir.as_os_str()),
            Ok(Some(AbsDir(dir.clone())))
        );
    }

    #[test]
    fn relative_and_padded_overrides_are_rejected() {
        assert!(validate_override(OsStr::new("rel/dir")).is_err());
        assert!(validate_override(OsStr::new(".")).is_err());
        let padded = format!(" {}", std::env::temp_dir().display());
        assert!(validate_override(OsStr::new(&padded)).is_err());
    }

    #[test]
    fn relative_config_override_falls_back_to_platform_dir() {
        assert_eq!(config_dir_from(Some("relative".into())), platform_default());
    }
}
