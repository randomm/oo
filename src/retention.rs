//! Retention of indexed command output, configured by `[store] retention`
//! in `config.toml` (seconds; `0` disables purging).

use std::path::Path;

use serde::Deserialize;

use crate::store::Store;

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
    let cf: ConfigFile =
        toml::from_str(content).map_err(|e| format!("cannot parse config.toml: {e}"))?;
    match cf.store.and_then(|s| s.retention) {
        None => Ok(DEFAULT_RETENTION_SECS),
        Some(n) if n < 0 => Err(format!("invalid [store] retention: must be >= 0, got {n}")),
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
                "oo: warning: {}: {reason}; using default {DEFAULT_RETENTION_SECS}s",
                path.display()
            );
            DEFAULT_RETENTION_SECS
        }
    }
}

/// Lazy, per-project purge run when `project_id` indexes output. Entries of
/// other projects are untouched; `retention = 0` skips the purge entirely.
/// Best-effort: a failed purge never blocks indexing.
pub(crate) fn purge_stale(store: &mut dyn Store, project_id: &str) {
    let retention = load_retention();
    if retention > 0 {
        let _ = store.cleanup_stale(project_id, retention);
    }
}

#[cfg(test)]
#[path = "retention_tests.rs"]
mod retention_tests;
