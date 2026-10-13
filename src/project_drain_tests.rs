//! Tests for `src/project_drain.rs`: drain and stale-cleanup semantics over one
//! configurable fake, plus the once-per-gate window warning.

use super::*;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};

/// The fake panics after this many listings, so a non-converging drain fails
/// the test instead of hanging.
const LIST_CALL_BUDGET: usize = 1_000;

/// Scripted result of one delete: `Ok(deleted)` or an error message.
type Scripted = Result<bool, String>;

/// The expected drain error Display for a stop with the given counts.
fn partial(deleted: usize, failed: usize) -> String {
    format!("store error: partial delete: deleted {deleted}, failed {failed}; entries remain")
}

fn oo(id: &str, timestamp: i64) -> Entry {
    Entry {
        id: id.to_string(),
        timestamp,
        is_oo_entry: true,
    }
}

fn other(id: &str) -> Entry {
    Entry {
        id: id.to_string(),
        timestamp: 0,
        is_oo_entry: false,
    }
}

fn numbered(range: std::ops::RangeInclusive<i64>) -> Vec<Entry> {
    range.map(|n| oo(&format!("e{n}"), 0)).collect()
}

/// Configurable fake for [`ProjectEntries`].
///
/// - Listings return the live rows newest first, capped at `limit`. `pages`
///   replaces them with scripted pages, one per listing (empty once exhausted),
///   for cases where the listing must not follow the live set.
/// - `readd_per_pass` adds fresh rows after every listing, so the project never
///   empties. This exercises the pass bound.
/// - A delete returns the next scripted result for its id if one is left. Else
///   a `failing` id returns `Err`, a `vanishing` id is removed and returns
///   `Ok(false)`, and any other id is removed and returns `Ok(true)`.
struct Fake {
    live: RefCell<Vec<Entry>>,
    /// Ids deleted or vanished; listings skip them, so a delete is O(1).
    removed: RefCell<HashSet<String>>,
    pages: RefCell<Option<VecDeque<Vec<Entry>>>>,
    list_err: Option<String>,
    always_err: HashSet<String>,
    vanish: HashSet<String>,
    script: RefCell<HashMap<String, VecDeque<Scripted>>>,
    readd_per_pass: usize,
    next_readd: Cell<usize>,
    /// Calls to `list` (the metadata-carrying listing).
    entry_listings: Cell<usize>,
    /// Calls to `list` and `list_ids` together.
    listings: Cell<usize>,
    deleted: RefCell<Vec<String>>,
}

impl Fake {
    fn with_entries(entries: Vec<Entry>) -> Self {
        Self {
            live: RefCell::new(entries),
            removed: RefCell::new(HashSet::new()),
            pages: RefCell::new(None),
            list_err: None,
            always_err: HashSet::new(),
            vanish: HashSet::new(),
            script: RefCell::new(HashMap::new()),
            readd_per_pass: 0,
            next_readd: Cell::new(0),
            entry_listings: Cell::new(0),
            listings: Cell::new(0),
            deleted: RefCell::new(Vec::new()),
        }
    }

    fn with_pages(pages: Vec<Vec<&str>>) -> Self {
        let fake = Self::with_entries(Vec::new());
        let pages = pages
            .into_iter()
            .map(|page| page.into_iter().map(|id| oo(id, 0)).collect())
            .collect();
        *fake.pages.borrow_mut() = Some(pages);
        fake
    }

    fn failing(mut self, id: &str) -> Self {
        self.always_err.insert(id.to_string());
        self
    }

    fn vanishing(mut self, ids: &[&str]) -> Self {
        self.vanish.extend(ids.iter().map(|id| id.to_string()));
        self
    }

    fn script(self, id: &str, results: Vec<Scripted>) -> Self {
        self.script
            .borrow_mut()
            .insert(id.to_string(), VecDeque::from(results));
        self
    }

    fn readd_after_each_pass(mut self, count: usize) -> Self {
        self.readd_per_pass = count;
        self
    }

    fn listing_fails(mut self, msg: &str) -> Self {
        self.list_err = Some(msg.to_string());
        self
    }

    fn live_ids(&self) -> Vec<String> {
        let removed = self.removed.borrow();
        self.live
            .borrow()
            .iter()
            .filter(|e| !removed.contains(&e.id))
            .map(|e| e.id.clone())
            .collect()
    }

    fn deleted(&self) -> Vec<String> {
        self.deleted.borrow().clone()
    }

    fn listing(&self, limit: usize) -> Result<Vec<Entry>, Error> {
        self.listings.set(self.listings.get() + 1);
        assert!(
            self.listings.get() <= LIST_CALL_BUDGET,
            "fake listing called {} times; loop is not converging",
            self.listings.get()
        );
        if let Some(msg) = &self.list_err {
            return Err(Error::Store(msg.clone()));
        }
        let rows = match self.pages.borrow_mut().as_mut() {
            Some(pages) => pages.pop_front().unwrap_or_default(),
            None => {
                let removed = self.removed.borrow();
                self.live
                    .borrow()
                    .iter()
                    .filter(|e| !removed.contains(&e.id))
                    .take(limit)
                    .cloned()
                    .collect()
            }
        };
        for _ in 0..self.readd_per_pass {
            let n = self.next_readd.get() + 1;
            self.next_readd.set(n);
            self.live.borrow_mut().insert(0, oo(&format!("r{n}"), 0));
        }
        Ok(rows)
    }
}

impl ProjectEntries for Fake {
    fn list(&self, limit: usize) -> Result<Vec<Entry>, Error> {
        self.entry_listings.set(self.entry_listings.get() + 1);
        self.listing(limit)
    }

    fn list_ids(&self, limit: usize) -> Result<Vec<EntryId>, Error> {
        Ok(self.listing(limit)?.into_iter().map(|e| e.id).collect())
    }

    fn delete(&self, id: &str) -> Result<bool, Error> {
        if let Some(res) = self
            .script
            .borrow_mut()
            .get_mut(id)
            .and_then(VecDeque::pop_front)
        {
            if let Ok(true) = res {
                self.deleted.borrow_mut().push(id.to_string());
            }
            return res.map_err(Error::Store);
        }
        if self.always_err.contains(id) {
            return Err(Error::Store("scripted delete failure".into()));
        }
        self.removed.borrow_mut().insert(id.to_string());
        if self.vanish.contains(id) {
            return Ok(false);
        }
        self.deleted.borrow_mut().push(id.to_string());
        Ok(true)
    }
}

// --- drain_project -----------------------------------------------------------

#[test]
fn drain_page3_over_10_entries_all_deletable() {
    let fake = Fake::with_entries(numbered(1..=10));
    let deleted = drain_project(&fake, 3, MAX_PASSES).unwrap();
    assert_eq!(deleted, 10);
    assert!(fake.live_ids().is_empty());
}

#[test]
fn drain_one_always_failing_id_stops_and_counts_it_once() {
    let fake = Fake::with_entries(numbered(1..=8)).failing("e3");
    let err = drain_project(&fake, 10, MAX_PASSES).unwrap_err();
    assert_eq!(err.to_string(), partial(7, 1));
    // Pass 1 deletes seven rows and fails e3; pass 2 lists e3 alone and stops.
    // Its failure is counted once, not once per listing.
    assert_eq!(fake.live_ids(), vec!["e3".to_string()]);
}

#[test]
fn drain_id_failing_then_succeeding_empties_the_project() {
    let fake = Fake::with_entries(numbered(1..=5)).script("e5", vec![Err("first attempt".into())]);
    let deleted = drain_project(&fake, 10, MAX_PASSES).unwrap();
    assert_eq!(deleted, 5);
    assert!(fake.live_ids().is_empty());
}

#[test]
fn drain_all_listed_rows_already_gone_twice_stops_with_zero_failed() {
    // The same rows are listed on every pass and each delete reports the row
    // already gone. One retry is allowed; the second such pass stops, and the
    // failed count is the failed-set size (0), not a special case.
    let gone = || vec![Ok(false), Ok(false)];
    let fake = Fake::with_pages(vec![vec!["a", "b", "c"]; 3])
        .script("a", gone())
        .script("b", gone())
        .script("c", gone());
    let err = drain_project(&fake, 10, MAX_PASSES).unwrap_err();
    assert_eq!(err.to_string(), partial(0, 0));
}

#[test]
fn drain_failed_id_absent_from_later_listings_stops_on_second_gone_pass() {
    // Pass 1 deletes a and fails b. Pass 2 lists only c, already gone: one
    // retry, not an immediate stop. Pass 3 is also all gone, so the drain
    // stops with b still pending.
    let fake = Fake::with_pages(vec![vec!["a", "b"], vec!["c"], vec!["c"]])
        .script("b", vec![Err("boom".to_string())])
        .script("c", vec![Ok(false), Ok(false)]);
    let err = drain_project(&fake, 10, MAX_PASSES).unwrap_err();
    assert_eq!(err.to_string(), partial(1, 1));
}

#[test]
fn drain_rows_vanishing_concurrently_succeeds() {
    // Ids 2 and 3 are removed by another writer before their delete. Id 1
    // deletes normally, and the next listing is empty.
    let fake = Fake::with_entries(numbered(1..=3)).vanishing(&["e2", "e3"]);
    let deleted = drain_project(&fake, 10, MAX_PASSES).unwrap();
    assert_eq!(deleted, 1);
    assert!(fake.live_ids().is_empty());
}

#[test]
fn drain_all_rows_vanishing_concurrently_succeeds_with_zero() {
    let fake = Fake::with_entries(numbered(1..=2)).vanishing(&["e1", "e2"]);
    let deleted = drain_project(&fake, 10, MAX_PASSES).unwrap();
    assert_eq!(deleted, 0);
    assert!(fake.live_ids().is_empty());
}

#[test]
fn drain_readding_writer_hits_the_pass_bound_and_stops() {
    // Every pass deletes three rows, but a writer re-adds three after each
    // listing, so the project never empties.
    let fake = Fake::with_entries(numbered(1..=3)).readd_after_each_pass(3);
    let err = drain_project(&fake, 10, 5).unwrap_err();
    // Five passes of three deletions; no delete failed.
    assert_eq!(err.to_string(), partial(15, 0));
    assert_eq!(fake.listings.get(), 5);
}

#[test]
fn drain_empty_project_succeeds_with_zero() {
    let fake = Fake::with_entries(Vec::new());
    assert_eq!(drain_project(&fake, 10, MAX_PASSES).unwrap(), 0);
}

#[test]
fn drain_uses_id_listing_not_entry_listing() {
    // Drain ignores timestamps and source, so it must not take the listing
    // that carries metadata.
    let fake = Fake::with_entries(numbered(1..=3));
    drain_project(&fake, 10, MAX_PASSES).unwrap();
    assert_eq!(fake.entry_listings.get(), 0);
}

#[test]
fn drain_listing_failure_is_an_error_never_success() {
    let fake = Fake::with_entries(numbered(1..=2)).listing_fails("listing failed");
    let err = drain_project(&fake, 10, MAX_PASSES).unwrap_err();
    assert_eq!(
        err.to_string(),
        "store error: partial delete: deleted 0, failed 0; entries may remain; listing failed: listing failed"
    );
}

// --- cleanup_stale_in --------------------------------------------------------

#[test]
fn cleanup_stale_removes_only_stale_oo_entries() {
    let fake = Fake::with_entries(vec![
        oo("old-oo", 99),     // deleted
        oo("at-cutoff", 100), // kept: exactly at the cutoff
        oo("fresh", 101),     // kept
        other("old-other"),   // kept: not an oo entry
    ]);
    let out = cleanup_stale_in(&fake, 100, 10).unwrap();
    assert_eq!(
        out,
        CleanupOutcome {
            deleted: 1,
            failed: 0,
            window_capped: false
        }
    );
    assert_eq!(fake.deleted(), vec!["old-oo"]);
}

#[test]
fn cleanup_stale_delete_error_continues_and_counts_it() {
    let fake = Fake::with_entries(vec![oo("a", 1), oo("b", 2), oo("c", 3)]).failing("b");
    let out = cleanup_stale_in(&fake, 100, 10).unwrap();
    assert_eq!(
        out,
        CleanupOutcome {
            deleted: 2,
            failed: 1,
            window_capped: false
        }
    );
    assert_eq!(fake.deleted(), vec!["a", "c"]);
}

#[test]
fn cleanup_stale_window_capped_at_page_boundary() {
    let n = LIST_CAP as i64;
    let entries =
        |count: i64| -> Vec<Entry> { (1..=count).map(|i| oo(&format!("e{i}"), i)).collect() };

    // Exactly 10,000 rows: the window is full, so it is capped.
    let fake = Fake::with_entries(entries(n));
    let out = cleanup_stale_in(&fake, n + 100, LIST_CAP).unwrap();
    assert_eq!(
        out,
        CleanupOutcome {
            deleted: LIST_CAP,
            failed: 0,
            window_capped: true
        }
    );

    // More rows than the window: only the first 10,000 are listed, still capped.
    let fake = Fake::with_entries(entries(n + 1));
    let out = cleanup_stale_in(&fake, n + 100, LIST_CAP).unwrap();
    assert_eq!(out.deleted, LIST_CAP);
    assert!(out.window_capped);

    // One row below the page: not capped.
    let fake = Fake::with_entries(entries(n - 1));
    let out = cleanup_stale_in(&fake, n + 100, LIST_CAP).unwrap();
    assert_eq!(
        out,
        CleanupOutcome {
            deleted: LIST_CAP - 1,
            failed: 0,
            window_capped: false
        }
    );
}

#[test]
fn cleanup_stale_listing_failure_propagates() {
    let fake = Fake::with_entries(vec![oo("a", 1)]).listing_fails("db offline");
    let err = cleanup_stale_in(&fake, 100, 10).unwrap_err();
    assert_eq!(err.to_string(), "store error: db offline");
}

// --- stale_cutoff ------------------------------------------------------------

#[test]
fn stale_cutoff_subtracts_the_age() {
    assert_eq!(stale_cutoff(1_000, 100), 900);
}

#[test]
fn stale_cutoff_age_beyond_now_marks_nothing_stale() {
    // A huge positive age: the cutoff is far in the past, so no real timestamp
    // is older than it. No overflow.
    assert_eq!(stale_cutoff(1_000, i64::MAX), 1_000 - i64::MAX);
    assert!(stale_cutoff(1_000, i64::MAX) < 0);
}

#[test]
fn stale_cutoff_negative_age_saturates_instead_of_wrapping() {
    // now - i64::MIN overflows; the saturated cutoff keeps every timestamp
    // stale, as SqliteStore's `now - timestamp > max_age_secs` does.
    assert_eq!(stale_cutoff(1_000, i64::MIN), i64::MAX);
    assert_eq!(stale_cutoff(1_000, -1), 1_001);
}

// --- warn_window_capped_once -------------------------------------------------

const CAP_WARNING: &str = "oo: warning: project has 10000 or more entries; stale cleanup only inspected the newest 10000\n";

#[test]
fn window_warning_once_per_gate() {
    let mut out: Vec<u8> = Vec::new();
    let gate = AtomicBool::new(false);

    warn_window_capped_once(&mut out, &gate);
    assert_eq!(String::from_utf8(out.clone()).unwrap(), CAP_WARNING);

    warn_window_capped_once(&mut out, &gate);
    assert_eq!(String::from_utf8(out.clone()).unwrap(), CAP_WARNING);

    // A fresh gate writes again.
    let mut again: Vec<u8> = Vec::new();
    let fresh = AtomicBool::new(false);
    warn_window_capped_once(&mut again, &fresh);
    assert_eq!(String::from_utf8(again).unwrap(), CAP_WARNING);
}
