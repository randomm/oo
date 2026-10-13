//! Location of oo's directories, shared by `learn`, `retention` and `store`.
//!
//! `OO_CONFIG_DIR` and `OO_DATA_DIR` must be absolute paths. An empty or
//! whitespace-only value counts as unset. Any other value that is not absolute
//! is ignored with a warning ([`warn_ignored_overrides`]) and the platform
//! default applies. Nothing is resolved against the current directory, so a
//! repository's own files cannot act as oo's configuration or data.
//! `OO_CLAUDE_DIR` and `OO_PI_EXTENSIONS_DIR` follow their own rules.

use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Environment variable naming the config directory.
pub(crate) const CONFIG_DIR_VAR: &str = "OO_CONFIG_DIR";

/// Environment variable naming the data directory.
pub(crate) const DATA_DIR_VAR: &str = "OO_DATA_DIR";

/// `OO_CONFIG_DIR` when valid, otherwise `<platform config dir>/oo`.
pub(crate) fn config_dir() -> PathBuf {
    config_dir_from(std::env::var_os(CONFIG_DIR_VAR))
}

/// Pure form of [`config_dir`] taking the `OO_CONFIG_DIR` value explicitly,
/// so callers and tests never need to mutate process-global environment.
pub(crate) fn config_dir_from(override_dir: Option<OsString>) -> PathBuf {
    accepted(override_dir.as_deref()).unwrap_or_else(platform_config_dir)
}

/// The directory that holds `oo.db`: `OO_DATA_DIR` when valid (used as-is),
/// otherwise `<platform data dir>/.oo`.
pub(crate) fn data_dir() -> PathBuf {
    data_dir_from(std::env::var_os(DATA_DIR_VAR))
}

/// Pure form of [`data_dir`] taking the `OO_DATA_DIR` value explicitly,
/// so callers and tests never need to mutate process-global environment.
pub(crate) fn data_dir_from(override_dir: Option<OsString>) -> PathBuf {
    accepted(override_dir.as_deref()).unwrap_or_else(platform_data_dir)
}

/// Prints one stderr line for each set override that is ignored. Called once at
/// process start, so every command path warns at most once per variable.
///
/// The message names the variable and the reason, not the value: the value
/// comes from the environment and may contain control characters. The write is
/// best-effort so a closed stderr cannot abort the command.
pub fn warn_ignored_overrides() {
    for name in [CONFIG_DIR_VAR, DATA_DIR_VAR] {
        let Some(value) = std::env::var_os(name) else {
            continue;
        };
        if let Err(reason) = check_override(&value) {
            let _ = writeln!(
                std::io::stderr(),
                "oo: ignoring {name}: {reason}; using the default directory"
            );
        }
    }
}

/// Classifies a raw override value without touching the environment or stderr.
///
/// `Ok(None)` means unset or blank. `Ok(Some(dir))` is an absolute path used
/// verbatim (no trimming, so a padded path is rejected). `Err(reason)` means the
/// value must be ignored.
fn check_override(value: &OsStr) -> Result<Option<PathBuf>, &'static str> {
    let text = value.to_string_lossy();
    if text.trim().is_empty() {
        return Ok(None);
    }
    if text.trim() != text.as_ref() {
        return Err("it must not have surrounding whitespace");
    }
    let path = Path::new(value);
    if path.is_absolute() {
        Ok(Some(path.to_path_buf()))
    } else {
        Err("it must be an absolute path")
    }
}

/// The override when it is valid; `None` when it is unset, blank, or ignored.
fn accepted(value: Option<&OsStr>) -> Option<PathBuf> {
    check_override(value?).ok().flatten()
}

fn platform_config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("oo")
}

fn platform_data_dir() -> PathBuf {
    dirs::data_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".oo")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_override_is_used_verbatim_without_oo_suffix() {
        let dir = std::env::temp_dir();
        assert_eq!(config_dir_from(Some(dir.clone().into_os_string())), dir);
        assert_eq!(data_dir_from(Some(dir.clone().into_os_string())), dir);
    }

    #[test]
    fn unset_override_uses_platform_dirs() {
        // The platform dirs read HOME live, so hold the guard against HOME mutators.
        let _env = crate::test_env::env_guard();
        assert_eq!(config_dir_from(None), platform_config_dir());
        assert_eq!(data_dir_from(None), platform_data_dir());
    }

    #[test]
    fn blank_override_is_treated_as_unset() {
        let _env = crate::test_env::env_guard();
        assert_eq!(check_override(OsStr::new("")), Ok(None));
        assert_eq!(check_override(OsStr::new(" \t")), Ok(None));
        assert_eq!(config_dir_from(Some("".into())), platform_config_dir());
        assert_eq!(data_dir_from(Some(" \t".into())), platform_data_dir());
    }

    #[test]
    fn absolute_override_is_accepted_verbatim() {
        let dir = std::env::temp_dir();
        assert_eq!(check_override(dir.as_os_str()), Ok(Some(dir.clone())));
    }

    #[test]
    fn relative_and_padded_overrides_are_rejected() {
        assert!(check_override(OsStr::new("rel/dir")).is_err());
        assert!(check_override(OsStr::new(".")).is_err());
        let leading = format!(" {}", std::env::temp_dir().display());
        assert!(check_override(OsStr::new(&leading)).is_err());
        let trailing = format!("{} ", std::env::temp_dir().display());
        assert!(check_override(OsStr::new(&trailing)).is_err());
    }

    #[test]
    fn relative_overrides_fall_back_to_platform_dirs() {
        let _env = crate::test_env::env_guard();
        assert_eq!(
            config_dir_from(Some("relative".into())),
            platform_config_dir()
        );
        assert_eq!(data_dir_from(Some("relative".into())), platform_data_dir());
    }
}
