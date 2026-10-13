//! Drain and stale-cleanup logic for backends without bulk delete or offset.
//!
//! Vipune has no bulk delete, no delete-by-project and no pagination offset,
//! and `list` is capped at [`LIST_CAP`] rows per call. Both `oo forget`
//! (drain every entry of a project) and retention cleanup (delete entries
//! older than a cutoff) therefore run a list/delete loop. The loop logic is
//! backend-agnostic and lives here, so it is testable with a deterministic
//! fake without building or running the optional Vipune backend.

// Only the `vipune-store` adapter calls these items in production; the default
// build compiles them for the tests alone, so dead_code would fire there.
#![cfg_attr(not(feature = "vipune-store"), allow(dead_code))]

use std::collections::HashSet;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::Error;

/// The type of a stored entry's identifier.
pub(crate) type EntryId = String;

/// One stored entry as far as draining and stale cleanup need to see it.
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    /// Backend identifier of the entry.
    pub id: EntryId,
    /// Unix seconds, taken from the entry's `SessionMeta.timestamp` (parsed
    /// by the adapter from its metadata JSON).
    pub timestamp: i64,
    /// Whether the entry's metadata parses as a `SessionMeta` with
    /// `source == "oo"`. Entries with missing or unparseable metadata are
    /// `false`; stale cleanup never touches them. Draining ignores this flag
    /// — `oo forget` removes every entry of the project.
    pub is_oo_entry: bool,
}

/// Backend surface used by [`drain_project`] and [`cleanup_stale_in`].
pub(crate) trait ProjectEntries {
    /// List up to `limit` entries of the project (newest first).
    fn list(&mut self, limit: usize) -> Result<Vec<Entry>, Error>;

    /// Delete one entry by id.
    ///
    /// `Ok(true)` means the entry was deleted, `Ok(false)` that no row
    /// matched the id.
    fn delete(&mut self, id: &str) -> Result<bool, Error>;
}

/// Vipune's per-call row cap for `list`.
pub(crate) const LIST_CAP: usize = 10_000;

/// Bound on the list/delete passes of [`drain_project`]. Guards against a
/// concurrent writer that keeps adding rows so the loop never ends.
pub(crate) const MAX_PASSES: usize = 1_000;

/// Outcome of [`cleanup_stale_in`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CleanupOutcome {
    /// Entries deleted.
    pub deleted: usize,
    /// Deletes that failed; the caller surfaces a partial-delete error when
    /// this is non-zero.
    pub failed: usize,
    /// The listing returned exactly `page` entries, so entries beyond the
    /// window were not inspected.
    pub window_capped: bool,
}

fn partial_delete_error(deleted: usize, failed: usize) -> Error {
    Error::Store(format!(
        "partial delete: deleted {deleted}, failed {failed}; entries remain"
    ))
}

/// Delete every entry of the project the backend can enumerate.
///
/// Each pass lists up to `page` entries and deletes them one by one. Pass
/// outcomes:
/// - at least one `Ok(true)`: continue with a fresh listing;
/// - no `Ok(true)` and at least one `Err`: stop at once, since re-listing
///   cannot drain a row whose delete keeps failing;
/// - only `Ok(false)` (rows already gone): continue once; a second such pass
///   in a row stops.
///
/// An empty listing is the only success. A listing failure, every stop above,
/// and reaching `max_passes` return an error so a partial drain is never
/// reported as success.
pub(crate) fn drain_project(
    store: &mut dyn ProjectEntries,
    page: usize,
    max_passes: usize,
) -> Result<usize, Error> {
    let mut deleted = 0usize;
    let mut failed: HashSet<EntryId> = HashSet::new();
    let mut empty_streak = 0usize;

    for _ in 0..max_passes {
        let entries = store.list(page).map_err(|e| {
            Error::Store(format!("list failed after deleting {deleted} entries: {e}"))
        })?;
        if entries.is_empty() {
            return Ok(deleted);
        }
        let mut deletions = 0usize;
        let mut pass_errors = 0usize;
        for entry in entries {
            match store.delete(&entry.id) {
                Ok(true) => {
                    deleted += 1;
                    deletions += 1;
                    failed.remove(&entry.id);
                }
                Ok(false) => {
                    failed.remove(&entry.id);
                }
                Err(e) => {
                    pass_errors += 1;
                    // Key by id so a row that errors on several passes is
                    // counted once.
                    if failed.insert(entry.id) {
                        eprintln!("oo: warning: failed to delete entry: {e}");
                    }
                }
            }
        }
        if deletions > 0 {
            empty_streak = 0;
        } else if pass_errors > 0 {
            // R2: a failed delete means re-listing cannot drain the entry.
            return Err(partial_delete_error(deleted, failed.len()));
        } else {
            // R3: only already-gone entries; allow one retry, then stop.
            empty_streak += 1;
            if empty_streak >= 2 {
                return Err(partial_delete_error(deleted, failed.len()));
            }
        }
    }
    Err(partial_delete_error(deleted, failed.len()))
}

/// Delete the project's `oo`-sourced entries older than `cutoff` (entries
/// with `timestamp < cutoff`; an entry exactly at the cutoff is kept).
///
/// Lists a single window of up to `page` entries (the newest ones, which is
/// all the backend can return), so `window_capped` is `true` when the
/// window was full and older entries were not examined. A failed delete does
/// not abort the loop; it is counted in [`CleanupOutcome::failed`]. A
/// listing failure is returned as an `Err`, never swallowed.
pub(crate) fn cleanup_stale_in(
    store: &mut dyn ProjectEntries,
    cutoff: i64,
    page: usize,
) -> Result<CleanupOutcome, Error> {
    let entries = store.list(page)?;
    let window_capped = entries.len() == page;
    let mut deleted = 0usize;
    let mut failed = 0usize;
    for entry in entries {
        if !entry.is_oo_entry || entry.timestamp >= cutoff {
            continue;
        }
        match store.delete(&entry.id) {
            Ok(true) => deleted += 1,
            Ok(false) => {}
            Err(e) => {
                failed += 1;
                eprintln!("oo: warning: failed to delete entry: {e}");
            }
        }
    }
    Ok(CleanupOutcome {
        deleted,
        failed,
        window_capped,
    })
}

/// Print the window-capped warning once per process.
///
/// Writes the warning line to `out` only when `gate` has not been set yet;
/// the gate makes the first caller win so the warning is not repeated by
/// every indexing run in the same process.
pub(crate) fn warn_window_capped_once(out: &mut dyn Write, gate: &AtomicBool) {
    if gate.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = writeln!(
        out,
        "oo: warning: project has 10000 or more entries; stale cleanup only inspected the newest 10000"
    );
}

#[cfg(test)]
#[path = "project_drain_tests.rs"]
mod tests;
