//! Tests for `src/project_drain.rs`: drain and stale-cleanup semantics over
//! a deterministic fake, plus the once-per-process window warning.

use super::*;
use std::collections::{HashMap, HashSet, VecDeque};

const LIST_CALL_BUDGET: usize = 1_000;

/// Deterministic in-memory fake for [`ProjectEntries`].
///
/// The store is a set of live row ids. `list` returns the live rows
/// newest-first (descending id number) capped at `limit`. `delete` removes
/// the row and returns `Ok(true)`, or `Ok(false)` if the row is not live
/// (already gone). Scriptable behaviors:
/// - `always_err` ids always return `Err`.
/// - `delete_script` ids return scripted results in order; after the script
///   is exhausted the id deletes normally.
/// - `vanish` ids are removed from the live set and return `Ok(false)` on
///   delete, modelling a row another writer removed after it was listed.
/// - `readd_per_pass` adds fresh live rows after every listing, so the
///   listing never empties — used to exercise the pass bound.
///
/// `list` counts calls and panics after `LIST_CALL_BUDGET` calls so a
/// regression in the drain loop fails the test instead of hanging.
struct Fake {
    page: usize,
    max_id: i64,
    live: Vec<i64>,
    always_err: HashSet<i64>,
    vanish: HashSet<i64>,
    delete_script: HashMap<i64, VecDeque<Result<bool, String>>>,
    readd_per_pass: Option<usize>,
    list_calls: usize,
}

impl Fake {
    fn new(ids: &[i64]) -> Self {
        let max_id = *ids.iter().max().unwrap_or(&0);
        Self {
            page: 10,
            max_id,
            live: ids.iter().copied().collect(),
            always_err: HashSet::new(),
            vanish: HashSet::new(),
            delete_script: HashMap::new(),
            readd_per_pass: None,
            list_calls: 0,
        }
    }

    fn with_page(mut self, page: usize) -> Self {
        self.page = page;
        self
    }

    fn failing(mut self, id: i64) -> Self {
        self.always_err.insert(id);
        self
    }

    fn vanishing(mut self, ids: &[i64]) -> Self {
        self.vanish.extend(ids.iter().copied());
        self
    }

    /// Script `id`'s delete results (consumed one per call); after the
    /// script is exhausted the id deletes normally.
    fn script(mut self, id: i64, results: Vec<Result<bool, String>>) -> Self {
        self.delete_script.insert(id, VecDeque::from(results));
        self
    }

    fn readd_after_each_pass(mut self, count: usize) -> Self {
        self.readd_per_pass = Some(count);
        self
    }

    fn is_empty(&self) -> bool {
        self.live.is_empty()
    }
}

impl ProjectEntries for Fake {
    fn list(&mut self, limit: usize) -> Result<Vec<Entry>, Error> {
        assert!(
            self.list_calls < LIST_CALL_BUDGET,
            "fake listing called {} times; loop is not converging",
            self.list_calls
        );
        self.list_calls += 1;
        let limit = limit.min(self.page);
        let rows: Vec<Entry> = self
            .live
            .iter()
            .rev()
            .take(limit)
            .map(|id| Entry {
                id: format!("e{id}"),
                timestamp: 0,
                is_oo_entry: true,
            })
            .collect();
        if let Some(count) = self.readd_per_pass {
            for _ in 0..count {
                self.max_id += 1;
                self.live.push(self.max_id);
            }
        }
        Ok(rows)
    }

    fn delete(&mut self, id: &str) -> Result<bool, Error> {
        let id_num = id
            .strip_prefix('e')
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(-1);
        if let Some(res) = self
            .delete_script
            .get_mut(&id_num)
            .and_then(|v| v.pop_front())
        {
            return res.map_err(Error::Store);
        }
        if self.always_err.contains(&id_num) {
            return Err(Error::Store("scripted delete failure".into()));
        }
        let gone = self.live.contains(&id_num) && !self.vanish.contains(&id_num);
        self.live.retain(|&r| r != id_num);
        Ok(gone)
    }
}

fn entry(id: &str, timestamp: i64, is_oo: bool) -> Entry {
    Entry {
        id: id.into(),
        timestamp,
        is_oo_entry: is_oo,
    }
}

fn partial_delete_msg(deleted: usize, failed: usize) -> String {
    format!("store error: partial delete: deleted {deleted}, failed {failed}; entries remain")
}

// --- drain_project -----------------------------------------------------------

#[test]
fn drain_page3_over_10_entries_all_deletable() {
    let ids: Vec<i64> = (1..=10).collect();
    let mut fake = Fake::new(&ids).with_page(3);
    let deleted = drain_project(&mut fake, 3, MAX_PASSES).unwrap();
    assert_eq!(deleted, 10);
    assert!(fake.is_empty());
}

#[test]
fn drain_one_always_failing_id_stops_and_counts_it_once() {
    let ids: Vec<i64> = (1..=8).collect();
    let mut fake = Fake::new(&ids).failing(3);
    let err = drain_project(&mut fake, 10, MAX_PASSES).unwrap_err();
    assert_eq!(err.to_string(), partial_delete_msg(7, 1));
    // The failing id was listed on two passes (R1 after 7 deletions, then
    // R2 on the surviving id alone) but its delete error is counted once
    // in the failed set — no double counting.
    assert_eq!(fake.list_calls, 2);
    assert_eq!(fake.live, vec![3]);
}

#[test]
fn drain_id_failing_then_succeeding_empties_the_project() {
    // First delete of id 5 fails, the retry (next pass) succeeds.
    let ids: Vec<i64> = (1..=5).collect();
    let mut fake = Fake::new(&ids).script(5, vec![Err("first attempt".into())]);
    let deleted = drain_project(&mut fake, 10, MAX_PASSES).unwrap();
    assert_eq!(deleted, 5);
    assert!(fake.is_empty());
}

#[test]
fn drain_all_gone_ids_stops_with_partial_delete_error() {
    // All listed ids are already gone (Ok(false)) on the first two passes.
    // The drain retries once (R3), the second pass is also all gone,
    // and the drain stops with a partial-delete error (never Ok).
    let mut fake = GoneListingFake::new(vec![1, 2, 3]);
    let err = drain_project(&mut fake, 10, MAX_PASSES).unwrap_err();
    assert_eq!(err.to_string(), partial_delete_msg(0, 0));
}

/// Fake that lists the same set of ids on the first two passes, then
/// returns an empty listing. All deletes return Ok(false) (the rows were
/// already gone). This models the scenario where the drain lists stale ids
/// that a concurrent writer has already removed.
struct GoneListingFake {
    ids: Vec<i64>,
    list_count: usize,
}

impl GoneListingFake {
    fn new(ids: Vec<i64>) -> Self {
        Self { ids, list_count: 0 }
    }
}

impl ProjectEntries for GoneListingFake {
    fn list(&mut self, _limit: usize) -> Result<Vec<Entry>, Error> {
        assert!(self.list_count < LIST_CALL_BUDGET, "loop is not converging");
        self.list_count += 1;
        // First two passes: list the ids. Third pass: empty.
        if self.list_count <= 2 {
            Ok(self
                .ids
                .iter()
                .map(|id| Entry {
                    id: format!("e{id}"),
                    timestamp: 0,
                    is_oo_entry: true,
                })
                .collect())
        } else {
            Ok(vec![])
        }
    }

    fn delete(&mut self, _id: &str) -> Result<bool, Error> {
        // All deletes return Ok(false): the rows are already gone.
        Ok(false)
    }
}

#[test]
fn drain_stale_failed_id_from_earlier_pass_is_reported() {
    // Pass 1: id 5 fails and ids 1..=4 delete (R1, so a fresh list follows;
    // the failed set keeps id 5). Pass 2 lists the surviving id 5 only, no
    // delete succeeds, and the non-empty failed set makes it stop. The
    // stop message reports the set size — 1, not a hardcoded 0.
    let ids: Vec<i64> = (1..=5).collect();
    let mut fake = Fake::new(&ids).failing(5);
    let err = drain_project(&mut fake, 10, MAX_PASSES).unwrap_err();
    assert_eq!(err.to_string(), partial_delete_msg(4, 1));
    assert_eq!(fake.list_calls, 2);
    assert_eq!(fake.live, vec![5]);
}

#[test]
fn drain_rows_vanishing_concurrently_succeeds() {
    // Ids 2 and 3 are listed but another writer removes them before their
    // delete, so those deletes return Ok(false). Id 1 deletes normally, and
    // the next listing is empty, so the drain succeeds with one deletion.
    let mut fake = Fake::new(&[1, 2, 3]).vanishing(&[2, 3]);
    let deleted = drain_project(&mut fake, 10, MAX_PASSES).unwrap();
    assert_eq!(deleted, 1);
    assert!(fake.is_empty());
}

#[test]
fn drain_all_rows_vanishing_concurrently_succeeds_with_zero() {
    // Every listed row vanishes before its delete: one all-Ok(false) pass
    // (the single allowed retry), then an empty listing, so Ok(0).
    let mut fake = Fake::new(&[1, 2]).vanishing(&[1, 2]);
    let deleted = drain_project(&mut fake, 10, MAX_PASSES).unwrap();
    assert_eq!(deleted, 0);
    assert!(fake.is_empty());
}

#[test]
fn drain_failed_id_absent_from_later_listings_gets_one_gone_retry() {
    // Pass 1 deletes a and fails b (R1). Pass 2 lists only c, which is
    // already gone: no delete succeeds and none fails, so it is one R3 retry,
    // not an immediate stop. Pass 3 is also all gone, so the drain stops with
    // b still in the failed set (size 1, not 0).
    let mut fake = PagesFake {
        pages: VecDeque::from(vec![vec!["a", "b"], vec!["c"], vec!["c"]]),
        deletes: HashMap::from([
            ("b", VecDeque::from(vec![Err("boom".to_string())])),
            ("c", VecDeque::from(vec![Ok(false), Ok(false)])),
        ]),
    };
    let err = drain_project(&mut fake, 10, MAX_PASSES).unwrap_err();
    assert_eq!(err.to_string(), partial_delete_msg(1, 1));
    assert_eq!(fake.pages.len(), 0);
}

/// Fake driven by a fixed script: each `list` pops the next page (an empty
/// page once the script runs out) and each id's deletes pop scripted results
/// (`Ok(true)` once exhausted).
struct PagesFake {
    pages: VecDeque<Vec<&'static str>>,
    deletes: HashMap<&'static str, VecDeque<Result<bool, String>>>,
}

impl ProjectEntries for PagesFake {
    fn list(&mut self, _limit: usize) -> Result<Vec<Entry>, Error> {
        let page = self.pages.pop_front().unwrap_or_default();
        Ok(page.into_iter().map(|id| entry(id, 0, true)).collect())
    }

    fn delete(&mut self, id: &str) -> Result<bool, Error> {
        match self.deletes.get_mut(id).and_then(|q| q.pop_front()) {
            Some(res) => res.map_err(Error::Store),
            None => Ok(true),
        }
    }
}

#[test]
fn drain_readding_writer_hits_the_pass_bound_and_stops() {
    // Every pass deletes three rows but a writer re-adds three after each
    // listing, so the listing never empties.
    let ids: Vec<i64> = (1..=3).collect();
    let mut fake = Fake::new(&ids).readd_after_each_pass(3);
    let err = drain_project(&mut fake, 10, 5).unwrap_err();
    // 5 passes x 3 deletions.
    assert!(
        err.to_string()
            .starts_with("store error: partial delete: deleted 15")
    );
    assert!(err.to_string().ends_with("; entries remain"));
    assert_eq!(fake.list_calls, 5);
}

#[test]
fn drain_empty_project_succeeds_with_zero() {
    let ids: Vec<i64> = Vec::new();
    let mut fake = Fake::new(&ids);
    let deleted = drain_project(&mut fake, 10, MAX_PASSES).unwrap();
    assert_eq!(deleted, 0);
}

#[test]
fn drain_listing_failure_is_an_error_never_success() {
    let mut fake = ListingFailingFake::new();
    let err = drain_project(&mut fake, 10, MAX_PASSES).unwrap_err();
    assert_eq!(
        err.to_string(),
        "store error: list failed after deleting 0 entries: store error: listing failed"
    );
}

/// Fake whose listing always fails: a listing error must surface as the
/// drain's `Err`, never as success with zero deletions.
struct ListingFailingFake;

impl ListingFailingFake {
    fn new() -> Self {
        Self
    }
}

impl ProjectEntries for ListingFailingFake {
    fn list(&mut self, _limit: usize) -> Result<Vec<Entry>, Error> {
        Err(Error::Store("listing failed".into()))
    }

    fn delete(&mut self, _id: &str) -> Result<bool, Error> {
        Err(Error::Store("unreachable".into()))
    }
}

// --- cleanup_stale_in --------------------------------------------------------

#[test]
fn cleanup_stale_removes_only_stale_oo_entries() {
    let cutoff = 100;
    let mut fake = CleanupFake::new(vec![
        entry("old-oo", 99, true),      // deleted
        entry("at-cutoff", 100, true),  // kept (exactly at cutoff)
        entry("fresh", 101, true),      // kept
        entry("old-other", 50, false),  // kept (not oo)
        entry("old-nometa", 10, false), // kept (unparseable metadata)
    ]);
    let out = cleanup_stale_in(&mut fake, cutoff, 10).unwrap();
    assert_eq!(
        out,
        CleanupOutcome {
            deleted: 1,
            failed: 0,
            window_capped: false
        }
    );
    assert_eq!(fake.deleted, vec!["old-oo"]);
}

#[test]
fn cleanup_stale_delete_error_continues_and_counts() {
    let mut fake = CleanupFake::new(vec![
        entry("a", 1, true),
        entry("b", 2, true),
        entry("c", 3, true),
    ]);
    fake.fail_delete = Some("b".into());
    let out = cleanup_stale_in(&mut fake, 100, 10).unwrap();
    assert_eq!(
        out,
        CleanupOutcome {
            deleted: 2,
            failed: 1,
            window_capped: false
        }
    );
    assert_eq!(fake.deleted, vec!["a", "c"]);
}

#[test]
fn cleanup_stale_window_capped_at_full_page() {
    // 10,000 rows, all stale oo entries — the listing returns exactly `page`.
    let n = LIST_CAP as i64;
    let entries: Vec<Entry> = (1..=n).map(|i| entry(&format!("r{i}"), i, true)).collect();
    let mut fake = CleanupFake::new(entries);
    let out = cleanup_stale_in(&mut fake, n + 100, LIST_CAP).unwrap();
    assert_eq!(
        out,
        CleanupOutcome {
            deleted: LIST_CAP,
            failed: 0,
            window_capped: true
        }
    );

    // One fewer row: below the cap, not capped.
    let entries: Vec<Entry> = (1..n).map(|i| entry(&format!("r{i}"), i, true)).collect();
    let mut fake = CleanupFake::new(entries);
    let out = cleanup_stale_in(&mut fake, n + 100, LIST_CAP).unwrap();
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
    let mut fake = CleanupFake::new(vec![entry("a", 1, true)]);
    fake.list_err = Some("db offline".into());
    let err = cleanup_stale_in(&mut fake, 100, 10).unwrap_err();
    assert_eq!(err.to_string(), "store error: db offline");
}

/// Cleanup-oriented fake: fixed listing (returned as-is, up to a `page`
/// cap), with a settable delete failure and a recorded delete list.
struct CleanupFake {
    entries: Vec<Entry>,
    list_err: Option<String>,
    fail_delete: Option<String>,
    deleted: Vec<String>,
}

impl CleanupFake {
    fn new(entries: Vec<Entry>) -> Self {
        Self {
            entries,
            list_err: None,
            fail_delete: None,
            deleted: Vec::new(),
        }
    }
}

impl ProjectEntries for CleanupFake {
    fn list(&mut self, _limit: usize) -> Result<Vec<Entry>, Error> {
        if let Some(msg) = self.list_err.take() {
            return Err(Error::Store(msg));
        }
        Ok(std::mem::take(&mut self.entries))
    }

    fn delete(&mut self, id: &str) -> Result<bool, Error> {
        if self.fail_delete.as_deref() == Some(id) {
            return Err(Error::Store("scripted delete failure".into()));
        }
        self.deleted.push(id.to_string());
        Ok(true)
    }
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
