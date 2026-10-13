//! Drain and stale-cleanup loops for backends without bulk delete or offset.
//!
//! Vipune has no bulk delete, no delete-by-project and no pagination offset,
//! so `oo forget` (drain every entry of a project) and retention cleanup
//! (delete old `oo` entries) run list/delete loops over a [`ProjectEntries`]
//! backend. The loops are generic over that trait so a deterministic fake can
//! test them; the Vipune adapter (`store_vipune.rs`) is the only production
//! implementor. The adapter maps rows; the staleness decision lives here.
//!
//! Compiled with the `vipune-store` feature (its only production caller) and
//! for tests, so the fake-based tests run in a default `cargo test`.

use std::collections::HashSet;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::Error;

/// Identifier of one stored entry, as issued by the backend.
pub(crate) type EntryId = String;

/// One stored entry as far as stale cleanup needs to see it.
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    /// Backend identifier of the entry.
    pub id: EntryId,
    /// Unix seconds from `SessionMeta.timestamp`. Meaningful only when
    /// `is_oo_entry` is true.
    pub timestamp: i64,
    /// Metadata parses as `SessionMeta` with `source == "oo"`. Entries with
    /// missing or unparseable metadata are `false` and cleanup never deletes
    /// them. Draining ignores this flag.
    pub is_oo_entry: bool,
}

/// Backend surface used by [`drain_project`] and [`cleanup_stale_in`].
pub(crate) trait ProjectEntries {
    /// List up to `limit` entries of the project, newest first. This must be
    /// a pure read: deletes are the only mutations.
    fn list(&self, limit: usize) -> Result<Vec<Entry>, Error>;

    /// List up to `limit` entry ids of the project. Drain needs only ids, so
    /// the adapter can skip the metadata work that [`list`](Self::list) does.
    fn list_ids(&self, limit: usize) -> Result<Vec<EntryId>, Error>;

    /// Delete one entry. `Ok(true)` means it was deleted, `Ok(false)` that no
    /// row matched the id any more.
    fn delete(&self, id: &str) -> Result<bool, Error>;
}

/// Largest page one Vipune `list` call returns, so the stale-cleanup window
/// is at most this many entries. Verified against vipune 0.1.2:
/// `memory/store.rs:13` (`MAX_SEARCH_LIMIT = 10_000`) and `memory/store.rs:24`
/// (`validate_limit` rejects larger); `sqlite/mod.rs:256-265` (`list` is
/// `ORDER BY created_at DESC LIMIT ?2`, with no offset and no tiebreak, so
/// rows with equal `created_at` have no defined order among themselves).
pub(crate) const LIST_CAP: usize = 10_000;

/// Upper bound on list/delete passes in [`drain_project`], guarding against a
/// concurrent writer that keeps refilling the project. The worst case is this
/// many full listings plus a delete per row, about 10 million row reads, before
/// the drain gives up.
pub(crate) const MAX_PASSES: usize = 1_000;

/// Outcome of [`cleanup_stale_in`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CleanupOutcome {
    /// Entries deleted.
    pub deleted: usize,
    /// Deletes that failed. The caller surfaces a partial-delete error when
    /// this is non-zero.
    pub failed: usize,
    /// The listing returned exactly `page` entries, so older entries were not
    /// inspected.
    pub window_capped: bool,
}

/// Cutoff for stale cleanup: an entry is stale when `timestamp < cutoff`.
///
/// Saturates instead of overflowing. A negative `max_age_secs` is accepted by
/// the `Store` trait; `now - max_age_secs` then exceeds `i64::MAX` only for
/// values near `i64::MIN`, and saturating keeps every timestamp stale, the same
/// answer `SqliteStore` gives (`now - timestamp > max_age_secs`).
pub(crate) fn stale_cutoff(now: i64, max_age_secs: i64) -> i64 {
    now.saturating_sub(max_age_secs)
}

/// The partial-delete error. The message is a contract documented in
/// `docs/security-model.md` (Data Retention) and names no per-entry cause.
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

fn warn_delete_failed(id: &str, e: &Error) {
    eprintln!("oo: warning: failed to delete entry {id}: {}", detail(e));
}

/// Delete every entry of the project the backend can enumerate.
///
/// Each pass lists up to `page` ids and deletes them one by one. Then:
/// - at least one `Ok(true)`: continue with a fresh listing (R1);
/// - no `Ok(true)` and at least one `Err`: stop, since re-listing cannot drain
///   a row whose delete keeps failing (R2);
/// - only `Ok(false)` (rows already gone): continue once; a second such pass
///   in a row stops (R3);
/// - `max_passes` passes without an empty listing: stop (R4).
///
/// An empty listing is the only success. Every stop returns the partial-delete
/// error, with `failed` equal to the size of the set of ids whose latest delete
/// failed. A listing failure returns an error too.
pub(crate) fn drain_project(
    store: &dyn ProjectEntries,
    page: usize,
    max_passes: usize,
) -> Result<usize, Error> {
    let mut deleted = 0usize;
    // Keyed by id so a row that fails on several passes is counted once.
    let mut failed: HashSet<EntryId> = HashSet::new();
    let mut previous_pass_gone = false;

    for _ in 0..max_passes {
        let ids = match store.list_ids(page) {
            Ok(ids) => ids,
            Err(e) => {
                // The listing cause goes to stderr; the error keeps the D3 text.
                eprintln!("oo: warning: failed to list entries: {}", detail(&e));
                return Err(partial_delete_error(deleted, failed.len()));
            }
        };
        if ids.is_empty() {
            return Ok(deleted);
        }
        let mut deletions = 0usize;
        let mut pass_errors = 0usize;
        for id in ids {
            match store.delete(&id) {
                Ok(true) => {
                    deleted += 1;
                    deletions += 1;
                    failed.remove(&id);
                }
                Ok(false) => {
                    failed.remove(&id);
                }
                Err(e) => {
                    pass_errors += 1;
                    if failed.insert(id.clone()) {
                        warn_delete_failed(&id, &e);
                    }
                }
            }
        }
        if deletions == 0 && (pass_errors > 0 || previous_pass_gone) {
            return Err(partial_delete_error(deleted, failed.len()));
        }
        previous_pass_gone = deletions == 0;
    }
    Err(partial_delete_error(deleted, failed.len()))
}

/// Delete the `oo`-sourced entries older than `cutoff` (`timestamp < cutoff`;
/// an entry exactly at the cutoff is kept).
///
/// Lists one window of up to `page` entries, the newest ones the backend can
/// return, so `window_capped` is `true` when the window was full and older
/// entries were not inspected. A failed delete does not abort the loop; it is
/// counted in [`CleanupOutcome::failed`]. A listing failure is an `Err`.
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
        if !entry.is_oo_entry || entry.timestamp >= cutoff {
            continue;
        }
        match store.delete(&entry.id) {
            Ok(true) => out.deleted += 1,
            Ok(false) => {}
            Err(e) => {
                out.failed += 1;
                warn_delete_failed(&entry.id, &e);
            }
        }
    }
    Ok(out)
}

/// Print the window-capped warning once per gate.
///
/// Writes the line only when `gate` was not set yet. Production passes one
/// process-wide gate, so every indexing run and retention purge in the process
/// shares a single warning.
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
