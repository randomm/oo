//! Drain and stale-cleanup loops for backends without bulk delete or offset.
//!
//! Vipune has no bulk delete, no delete-by-project and no pagination offset,
//! so `oo forget` (drain every entry of a project) and retention cleanup
//! (delete old `oo` entries) run list/delete loops over a [`ProjectEntries`]
//! backend. The loops are generic over that trait so a deterministic fake can
//! test them; the Vipune adapter is the only production implementor.
//!
//! The staleness rule is oo policy, not a backend property: only entries whose
//! metadata parses as `SessionMeta` with `source == "oo"` carry a timestamp
//! ([`Entry::oo_timestamp`]) and are ever deleted by [`cleanup_stale_in`].
//! Draining ignores the timestamp.
//!
//! Compiled only with the `vipune-store` feature (its sole caller) and for
//! tests.

use std::collections::HashSet;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::Error;

/// Identifier of one stored entry, as issued by [`ProjectEntries::list`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct EntryId(String);

impl EntryId {
    pub(crate) fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// One stored entry as far as draining and stale cleanup need to see it.
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    /// Backend identifier of the entry.
    pub id: EntryId,
    /// `Some(unix seconds)` when the entry is `oo`-sourced with parseable
    /// metadata, `None` otherwise. Stale cleanup only deletes entries that
    /// carry a timestamp older than the cutoff.
    pub oo_timestamp: Option<i64>,
}

/// Backend surface used by [`drain_project`] and [`cleanup_stale_in`].
pub(crate) trait ProjectEntries {
    /// List up to `limit` entries of the project, newest first.
    ///
    /// The stale-cleanup window assumes this ordering. The Vipune adapter
    /// relies on Vipune's list order, which is not verified against vipune
    /// 0.1.2. This method must be a pure read: deletes are the only mutations.
    fn list(&self, limit: usize) -> Result<Vec<Entry>, Error>;

    /// Delete one entry by id.
    ///
    /// `Ok(true)` means the entry was deleted, `Ok(false)` that no row
    /// matched the id.
    fn delete(&self, id: &EntryId) -> Result<bool, Error>;
}

/// Upper bound on list/delete passes in [`drain_project`]. Guards against a
/// concurrent writer that keeps refilling the project. Worst case is this many
/// full listings plus one delete per row (at Vipune's 10,000-row cap, about
/// 10 million row materialisations) before the drain gives up; a plain drain
/// finishes in one or two passes.
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

/// The error for a drain or cleanup that could not delete every entry. The
/// message is a contract (documented in `docs/security-model.md`, Data
/// Retention) and carries no per-entry cause.
pub(crate) fn partial_delete_error(deleted: usize, failed: usize) -> Error {
    Error::Store(format!(
        "partial delete: deleted {deleted}, failed {failed}; entries remain"
    ))
}

/// The message of a backend error without the `store error: ` display prefix,
/// so wrapping it does not repeat the prefix.
fn detail(e: &Error) -> String {
    match e {
        Error::Store(msg) => msg.clone(),
        other => other.to_string(),
    }
}

/// Delete every entry of the project the backend can enumerate.
///
/// Each pass lists up to `page` entries and deletes them one by one, then
/// stops or continues:
/// - at least one `Ok(true)`: continue with a fresh listing;
/// - no `Ok(true)` and at least one `Err`: stop, since re-listing cannot drain
///   a row whose delete keeps failing;
/// - only `Ok(false)` (rows already gone): retry once; a second such pass in a
///   row stops.
///
/// An empty listing is the only success. A listing failure, every stop above,
/// and reaching `max_passes` return an error, so a partial drain is never
/// reported as success.
pub(crate) fn drain_project(
    store: &dyn ProjectEntries,
    page: usize,
    max_passes: usize,
) -> Result<usize, Error> {
    let mut deleted = 0usize;
    // Keyed by id so a row that fails on several passes is counted once.
    let mut failed: HashSet<EntryId> = HashSet::new();
    // The previous pass deleted nothing and saw only already-gone rows.
    let mut gone_pass = false;

    for _ in 0..max_passes {
        let entries = store.list(page).map_err(|e| {
            Error::Store(format!(
                "list failed after deleting {deleted} entries: {}",
                detail(&e)
            ))
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
                    if !failed.contains(&entry.id) {
                        eprintln!(
                            "oo: warning: failed to delete entry {}: {}",
                            entry.id.as_str(),
                            detail(&e)
                        );
                    }
                    failed.insert(entry.id);
                }
            }
        }
        if deletions == 0 && (pass_errors > 0 || gone_pass) {
            return Err(partial_delete_error(deleted, failed.len()));
        }
        gone_pass = deletions == 0;
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
    store: &dyn ProjectEntries,
    cutoff: i64,
    page: usize,
) -> Result<CleanupOutcome, Error> {
    let entries = store.list(page)?;
    let mut out = CleanupOutcome {
        deleted: 0,
        failed: 0,
        window_capped: entries.len() == page,
    };
    for entry in entries {
        let Some(timestamp) = entry.oo_timestamp else {
            continue;
        };
        if timestamp >= cutoff {
            continue;
        }
        match store.delete(&entry.id) {
            Ok(true) => out.deleted += 1,
            Ok(false) => {}
            Err(e) => {
                out.failed += 1;
                eprintln!(
                    "oo: warning: failed to delete entry {}: {}",
                    entry.id.as_str(),
                    detail(&e)
                );
            }
        }
    }
    Ok(out)
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
