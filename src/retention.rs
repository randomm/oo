//! Retention of indexed command output, configured by `[store] retention`
//! in `config.toml` (seconds; `0` disables automatic purging).

use std::path::Path;

use serde::Deserialize;

/// Default retention: 7 days, in seconds.
pub const DEFAULT_RETENTION_SECS: i64 = 604_800;

#[derive(Deserialize)]
struct ConfigFile {
    store: Option<StoreSection>,
}

#[derive(Deserialize)]
struct StoreSection {
    retention: Option<i64>,
}

/// Parse `[store] retention` from config.toml content.
///
/// Unrelated sections (e.g. `[learn]`) are ignored, so a broken `[learn]`
/// block does not affect retention and vice versa.
pub(crate) fn parse_retention(content: &str) -> Result<i64, String> {
    let cf: ConfigFile = toml::from_str(content).map_err(|e| e.to_string())?;
    match cf.store.and_then(|s| s.retention) {
        None => Ok(DEFAULT_RETENTION_SECS),
        Some(n) if n < 0 => Err(format!("retention must be >= 0, got {n}")),
        Some(n) => Ok(n),
    }
}

/// Retention from `<config dir>/config.toml`, falling back to the default.
///
/// Never fails: a missing file is silent, an invalid value prints a warning
/// on stderr and uses the default so indexing is not blocked by config.
pub(crate) fn load_retention() -> i64 {
    load_retention_from(&crate::learn::config_dir().join("config.toml"))
}

pub(crate) fn load_retention_from(path: &Path) -> i64 {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return DEFAULT_RETENTION_SECS,
        Err(e) => {
            eprintln!(
                "oo: warning: {}: {e}; using default retention {DEFAULT_RETENTION_SECS}s",
                path.display()
            );
            return DEFAULT_RETENTION_SECS;
        }
    };
    match parse_retention(&content) {
        Ok(n) => n,
        Err(reason) => {
            eprintln!(
                "oo: warning: {}: invalid [store] retention ({reason}); using default {DEFAULT_RETENTION_SECS}s",
                path.display()
            );
            DEFAULT_RETENTION_SECS
        }
    }
}

#[cfg(test)]
#[path = "retention_tests.rs"]
mod retention_tests;
