//! Tests for `src/project_drain.rs`: drain and stale-cleanup semantics over
//! deterministic fakes, plus the once-per-process window warning.

use super::*;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};

const LIST_CALL_BUDGET: usize = 1_000;
const TEST_LIST_CAP: usize = 10_000;

/// Scripted result for one delete call: `Ok(deleted)` or an error message.
type Scripted = Result<bool, String>;

/// Expected drain/cleanup error Display: the single D3 payload, no cause.
fn partial(deleted: usize, failed: usize) -> String {
    format!("store error: partial delete: deleted {deleted}, failed {failed}; entries remain")
}

fn oo(id: &str, timestamp: i64) -> Entry {
    Entry {
        id: EntryId::new(id),
        oo_timestamp: Some(timestamp),
    }
}

fn other(id: &str) -> Entry {
    Entry {
        id: EntryId::new(id),
        oo_timestamp: None,
    }
}

/// Stable string id for the fake's numbered rows.
fn name(n: i64) -> String {
    format!("e{n}")
}

/// Deterministic in-memory fake for [`ProjectEntries`].
///
/// The store is a list of live row names. `list` returns live rows
/// newest-first (last inserted first) capped at `limit`. `delete` removes the
/// row and returns `Ok(true)`, or `Ok(false)` if the row is not live. Scripted
/// behaviours:
/// - `failing` ids always return `Err`.
/// - `script` results are consumed one per delete of that id; after the script
///   runs out the id deletes normally.
/// - `vanishing` ids are removed from the live set and return `Ok(false)` on
///   delete, modelling a row another writer removed after it was listed.
/// - `readd_per_pass` adds fresh live rows after every listing, so the listing
///   never empties. This exercises the pass bound.
///
/// `list` counts calls and panics after `LIST_CALL_BUDGET` calls so a
/// regression in the drain loop fails the test instead of hanging.
struct Fake {
    page: usize,
    readd_per_pass: usize,
    always_err: HashSet<String>,
    vanish: HashSet<String>,
    script: RefCell<HashMap<String, VecDeque<Scripted>>>,
    live: RefCell<Vec<String>>,
    max_id: Cell<i64>,
    list_calls: Cell<usize>,
}

impl Fake {
    fn new(ids: &[i64]) -> Self {
        Self {
            page: 10,
            readd_per_pass: 0,
            always_err: HashSet::new(),
            vanish: HashSet::new(),
            script: RefCell::new(HashMap::new()),
            live: RefCell::new(ids.iter().map(|&n| name(n)).collect()),
            max_id: Cell::new(ids.iter().copied().max().unwrap_or(0)),
            list_calls: Cell::new(0),
        }
    }

    fn with_page(mut self, page: usize) -> Self {
        self.page = page;
        self
    }

    fn failing(mut self, id: i64) -> Self {
        self.always_err.insert(name(id));
        self
    }

    fn vanishing(mut self, ids: &[i64]) -> Self {
        self.vanish.extend(ids.iter().map(|&n| name(n)));
        self
    }

    fn script(self, id: i64, results: Vec<Scripted>) -> Self {
        self.script
            .borrow_mut()
            .insert(name(id), VecDeque::from(results));
        self
    }

    fn readd_after_each_pass(mut self, count: usize) -> Self {
        self.readd_per_pass = count;
        self
    }

    fn live(&self) -> Vec<String> {
        self.live.borrow().clone()
    }

    fn list_calls(&self) -> usize {
        self.list_calls.get()
    }
}

impl ProjectEntries for Fake {
    fn list(&self, limit: usize) -> Result<Vec<Entry>, Error> {
        assert!(
            self.list_calls.get() < LIST_CALL_BUDGET,
            "fake listing called {} times; loop is not converging",
            self.list_calls.get()
        );
        self.list_calls.set(self.list_calls.get() + 1);
        let rows: Vec<Entry> = self
            .live
            .borrow()
            .iter()
            .rev()
            .take(limit.min(self.page))
            .map(|s| oo(s, 0))
            .collect();
        for _ in 0..self.readd_per_pass {
            self.max_id.set(self.max_id.get() + 1);
            self.live.borrow_mut().push(name(self.max_id.get()));
        }
        Ok(rows)
    }

    fn delete(&self, id: &EntryId) -> Result<bool, Error> {
        let key = id.as_str();
        if let Some(res) = self
            .script
            .borrow_mut()
            .get_mut(key)
            .and_then(VecDeque::pop_front)
        {
            return res.map_err(Error::Store);
        }
        if self.always_err.contains(key) {
            return Err(Error::Store("scripted delete failure".into()));
        }
        let mut live = self.live.borrow_mut();
        let present = live.iter().any(|r| r == key);
        live.retain(|r| r != key);
        Ok(present && !self.vanish.contains(key))
    }
}

/// Fake driven by a fixed script: each `list` pops the next page (an empty
/// page once the script runs out). Each id's deletes pop scripted results
/// (`Ok(true)` once exhausted).
struct PagesFake {
    pages: RefCell<VecDeque<Vec<&'static str>>>,
    deletes: RefCell<HashMap<&'static str, VecDeque<Scripted>>>,
}

impl PagesFake {
    fn new(pages: Vec<Vec<&'static str>>, deletes: Vec<(&'static str, Vec<Scripted>)>) -> Self {
        Self {
            pages: RefCell::new(VecDeque::from(pages)),
            deletes: RefCell::new(
                deletes
                    .into_iter()
                    .map(|(id, r)| (id, VecDeque::from(r)))
                    .collect(),
            ),
        }
    }
}

impl ProjectEntries for PagesFake {
    fn list(&self, _limit: usize) -> Result<Vec<Entry>, Error> {
        let page = self.pages.borrow_mut().pop_front().unwrap_or_default();
        Ok(page.into_iter().map(|id| oo(id, 0)).collect())
    }

    fn delete(&self, id: &EntryId) -> Result<bool, Error> {
        match self
            .deletes
            .borrow_mut()
            .get_mut(id.as_str())
            .and_then(VecDeque::pop_front)
        {
            Some(res) => res.map_err(Error::Store),
            None => Ok(true),
        }
    }
}

/// Fake whose listing always fails: the error must surface as the drain's
/// `Err`, never as success with zero deletions.
struct ListingFails;

impl ProjectEntries for ListingFails {
    fn list(&self, _limit: usize) -> Result<Vec<Entry>, Error> {
        Err(Error::Store("listing failed".into()))
    }

    fn delete(&self, _id: &EntryId) -> Result<bool, Error> {
        Err(Error::Store("unreachable".into()))
    }
}

// --- drain_project -----------------------------------------------------------

#[test]
fn drain_page3_over_10_entries_all_deletable() {
    let ids: Vec<i64> = (1..=10).collect();
    let fake = Fake::new(&ids).with_page(3);
    let deleted = drain_project(&fake, 3, MAX_PASSES).unwrap();
    assert_eq!(deleted, 10);
    assert!(fake.live().is_empty());
}

#[test]
fn drain_one_always_failing_id_stops_and_counts_it_once() {
    let ids: Vec<i64> = (1..=8).collect();
    let fake = Fake::new(&ids).failing(3);
    let err = drain_project(&fake, 10, MAX_PASSES).unwrap_err();
    assert_eq!(err.to_string(), partial(7, 1));
    // Pass 1 deletes seven rows and fails e3; pass 2 lists e3 alone and stops.
    // Its failure is counted once, not once per pass.
    assert_eq!(fake.list_calls(), 2);
    assert_eq!(fake.live(), vec!["e3".to_string()]);
}

#[test]
fn drain_id_failing_then_succeeding_empties_the_project() {
    let ids: Vec<i64> = (1..=5).collect();
    let fake = Fake::new(&ids).script(5, vec![Err("first attempt".into())]);
    let deleted = drain_project(&fake, 10, MAX_PASSES).unwrap();
    assert_eq!(deleted, 5);
    assert!(fake.live().is_empty());
}

#[test]
fn drain_all_listed_rows_already_gone_stops_with_partial_error() {
    // The same three rows are listed on every pass, and each delete reports
    // that the row is already gone. One retry is allowed; the second such pass
    // stops. Failed count is the final failed-set size (0), never special-cased.
    let gone = || vec![Ok(false), Ok(false)];
    let fake = PagesFake::new(
        vec![
            vec!["a", "b", "c"],
            vec!["a", "b", "c"],
            vec!["a", "b", "c"],
        ],
        vec![("a", gone()), ("b", gone()), ("c", gone())],
    );
    let err = drain_project(&fake, 10, MAX_PASSES).unwrap_err();
    assert_eq!(err.to_string(), partial(0, 0));
}

#[test]
fn drain_stale_failed_id_from_earlier_pass_is_reported() {
    // Pass 1 deletes 1..=4 and fails 5. Pass 2 lists 5 alone; nothing can be
    // deleted, so the drain stops. The error reports the one pending failure.
    let ids: Vec<i64> = (1..=5).collect();
    let fake = Fake::new(&ids).failing(5);
    let err = drain_project(&fake, 10, MAX_PASSES).unwrap_err();
    assert_eq!(err.to_string(), partial(4, 1));
    assert_eq!(fake.list_calls(), 2);
    assert_eq!(fake.live(), vec!["e5".to_string()]);
}

#[test]
fn drain_rows_vanishing_concurrently_succeeds() {
    // Ids 2 and 3 are removed by another writer before their delete, so those
    // deletes return Ok(false). Id 1 deletes normally; the next listing is
    // empty, so the drain succeeds with one deletion.
    let fake = Fake::new(&[1, 2, 3]).vanishing(&[2, 3]);
    let deleted = drain_project(&fake, 10, MAX_PASSES).unwrap();
    assert_eq!(deleted, 1);
    assert!(fake.live().is_empty());
}

#[test]
fn drain_all_rows_vanishing_concurrently_succeeds_with_zero() {
    // Every listed row vanishes before its delete: one all-gone pass (the
    // allowed retry), then an empty listing, so Ok(0).
    let fake = Fake::new(&[1, 2]).vanishing(&[1, 2]);
    let deleted = drain_project(&fake, 10, MAX_PASSES).unwrap();
    assert_eq!(deleted, 0);
    assert!(fake.live().is_empty());
}

#[test]
fn drain_failed_id_absent_from_later_listings_gets_one_gone_retry() {
    // Pass 1 deletes a and fails b. Pass 2 lists only c, which is already
    // gone: that is one retry, not an immediate stop. Pass 3 is also all gone,
    // so the drain stops with b still pending.
    let fake = PagesFake::new(
        vec![vec!["a", "b"], vec!["c"], vec!["c"]],
        vec![
            ("b", vec![Err("boom".to_string())]),
            ("c", vec![Ok(false), Ok(false)]),
        ],
    );
    let err = drain_project(&fake, 10, MAX_PASSES).unwrap_err();
    assert_eq!(err.to_string(), partial(1, 1));
    assert_eq!(fake.pages.borrow().len(), 0);
}

#[test]
fn drain_readding_writer_hits_the_pass_bound_and_stops() {
    // Every pass deletes three rows, but a writer re-adds three after each
    // listing, so the listing never empties.
    let ids: Vec<i64> = (1..=3).collect();
    let fake = Fake::new(&ids).readd_after_each_pass(3);
    let err = drain_project(&fake, 10, 5).unwrap_err();
    // 5 passes x 3 deletions; no delete failed, so there is no cause clause.
    assert_eq!(err.to_string(), partial(15, 0));
    assert_eq!(fake.list_calls(), 5);
}

#[test]
fn drain_empty_project_succeeds_with_zero() {
    let fake = Fake::new(&[]);
    let deleted = drain_project(&fake, 10, MAX_PASSES).unwrap();
    assert_eq!(deleted, 0);
}

#[test]
fn drain_listing_failure_is_an_error_never_success() {
    let err = drain_project(&ListingFails, 10, MAX_PASSES).unwrap_err();
    assert_eq!(
        err.to_string(),
        "store error: list failed after deleting 0 entries: listing failed"
    );
}

// --- cleanup_stale_in --------------------------------------------------------

/// Cleanup-oriented fake: a fixed listing returned once, with a settable
/// listing failure, a settable delete failure, and a recorded delete list.
struct CleanupFake {
    entries: RefCell<Vec<Entry>>,
    list_err: RefCell<Option<String>>,
    fail_delete: Option<String>,
    deleted: RefCell<Vec<String>>,
}

impl CleanupFake {
    fn new(entries: Vec<Entry>) -> Self {
        Self {
            entries: RefCell::new(entries),
            list_err: RefCell::new(None),
            fail_delete: None,
            deleted: RefCell::new(Vec::new()),
        }
    }
}

impl ProjectEntries for CleanupFake {
    fn list(&self, _limit: usize) -> Result<Vec<Entry>, Error> {
        if let Some(msg) = self.list_err.borrow_mut().take() {
            return Err(Error::Store(msg));
        }
        Ok(std::mem::take(&mut *self.entries.borrow_mut()))
    }

    fn delete(&self, id: &EntryId) -> Result<bool, Error> {
        if self.fail_delete.as_deref() == Some(id.as_str()) {
            return Err(Error::Store("scripted delete failure".into()));
        }
        self.deleted.borrow_mut().push(id.as_str().to_string());
        Ok(true)
    }
}

#[test]
fn cleanup_stale_removes_only_stale_oo_entries() {
    let cutoff = 100;
    let fake = CleanupFake::new(vec![
        oo("old-oo", 99),     // deleted
        oo("at-cutoff", 100), // kept (exactly at cutoff)
        oo("fresh", 101),     // kept
        other("old-other"),   // kept (not oo)
    ]);
    let out = cleanup_stale_in(&fake, cutoff, 10).unwrap();
    assert_eq!(
        out,
        CleanupOutcome {
            deleted: 1,
            failed: 0,
            window_capped: false
        }
    );
    assert_eq!(*fake.deleted.borrow(), vec!["old-oo"]);
}

#[test]
fn cleanup_stale_delete_error_continues_and_counts_it() {
    let mut fake = CleanupFake::new(vec![oo("a", 1), oo("b", 2), oo("c", 3)]);
    fake.fail_delete = Some("b".into());
    let out = cleanup_stale_in(&fake, 100, 10).unwrap();
    assert_eq!(
        out,
        CleanupOutcome {
            deleted: 2,
            failed: 1,
            window_capped: false
        }
    );
    assert_eq!(*fake.deleted.borrow(), vec!["a", "c"]);
}

#[test]
fn cleanup_stale_window_capped_at_full_page() {
    // 10,000 rows, all stale oo entries: the listing returns exactly `page`.
    let n = TEST_LIST_CAP as i64;
    let entries: Vec<Entry> = (1..=n).map(|i| oo(&name(i), i)).collect();
    let fake = CleanupFake::new(entries);
    let out = cleanup_stale_in(&fake, n + 100, TEST_LIST_CAP).unwrap();
    assert_eq!(
        out,
        CleanupOutcome {
            deleted: TEST_LIST_CAP,
            failed: 0,
            window_capped: true
        }
    );

    // One fewer row: below the cap, not capped.
    let entries: Vec<Entry> = (1..n).map(|i| oo(&name(i), i)).collect();
    let fake = CleanupFake::new(entries);
    let out = cleanup_stale_in(&fake, n + 100, TEST_LIST_CAP).unwrap();
    assert_eq!(
        out,
        CleanupOutcome {
            deleted: TEST_LIST_CAP - 1,
            failed: 0,
            window_capped: false
        }
    );
}

#[test]
fn cleanup_stale_listing_failure_propagates() {
    let fake = CleanupFake::new(vec![oo("a", 1)]);
    *fake.list_err.borrow_mut() = Some("db offline".into());
    let err = cleanup_stale_in(&fake, 100, 10).unwrap_err();
    assert_eq!(err.to_string(), "store error: db offline");
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
    assert_eq!(String::from_utf8(out).unwrap(), CAP_WARNING);

    // A fresh gate writes again.
    let mut out2: Vec<u8> = Vec::new();
    let fresh = AtomicBool::new(false);
    warn_window_capped_once(&mut out2, &fresh);
    assert_eq!(String::from_utf8(out2).unwrap(), CAP_WARNING);
}
