//! Retention of indexed command output, configured by `[store] retention`
//! in `config.toml` (seconds; `0` disables purging).

use std::path::Path;

use serde::Deserialize;

use crate::config::config_dir;
use crate::store::Store;

/// Default retention: 7 days, in seconds.
pub const DEFAULT_RETENTION_SECS: i64 = 604_800;

#[derive(Deserialize)]
struct ConfigFile {
    store: Option<StoreSection>,
}

// Strict only here: a typo such as `retension = 0` must warn, not be silently
// ignored. Other top-level sections (e.g. `[learn]`) stay permissive.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoreSection {
    retention: Option<u64>,
}

/// Parse `[store] retention` from config.toml content.
///
/// Unrelated sections are ignored, so a `[learn]` block with unexpected
/// content does not affect retention. A file that fails TOML parsing, or a
/// `[store]` section with an unknown key or an invalid value, is an error.
pub(crate) fn parse_retention(content: &str) -> Result<i64, String> {
    let cf: ConfigFile = toml::from_str(content).map_err(|e| format!("cannot parse: {e}"))?;
    match cf.store.and_then(|s| s.retention) {
        None => Ok(DEFAULT_RETENTION_SECS),
        Some(n) => i64::try_from(n).map_err(|_| format!("[store] retention {n} is too large")),
    }
}

/// Read `[store] retention` from `path`. A missing file is not an error.
fn read_retention(path: &Path) -> Result<i64, String> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(DEFAULT_RETENTION_SECS),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    parse_retention(&content).map_err(|e| format!("{}: {e}", path.display()))
}

/// Retention from `<config dir>/config.toml`, falling back to the default.
///
/// Never fails: a missing file is silent, an invalid value prints a warning
/// on stderr and uses the default so indexing is not blocked by config.
pub(crate) fn load_retention() -> i64 {
    load_retention_from(&config_dir().join("config.toml"))
}

pub(crate) fn load_retention_from(path: &Path) -> i64 {
    read_retention(path).unwrap_or_else(|reason| {
        eprintln!("oo: warning: {reason}; using default retention {DEFAULT_RETENTION_SECS}s");
        DEFAULT_RETENTION_SECS
    })
}

/// Lazy, per-project purge run when `project_id` indexes output, using the
/// retention from config.toml.
pub(crate) fn purge_stale(store: &mut dyn Store, project_id: &str) {
    purge_with(store, project_id, load_retention());
}

/// Purge `project_id`'s entries older than `retention` seconds. Entries of
/// other projects are untouched; `retention = 0` skips the purge entirely.
/// Best-effort: a failed purge warns on stderr but never blocks indexing.
pub(crate) fn purge_with(store: &mut dyn Store, project_id: &str, retention: i64) {
    if retention == 0 {
        return;
    }
    if let Err(e) = store.cleanup_stale(project_id, retention) {
        eprintln!("oo: warning: retention purge failed: {e}");
    }
}

#[cfg(test)]
#[path = "retention_tests.rs"]
mod retention_tests;
