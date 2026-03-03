use super::*;
use crate::util::now_epoch;

fn test_meta(session: &str) -> SessionMeta {
    SessionMeta {
        source: "oo".into(),
        session: session.into(),
        command: "test cmd".into(),
        timestamp: now_epoch(),
    }
}

fn temp_store() -> SqliteStore {
    SqliteStore::open_at(&std::env::temp_dir().join(format!("oo-test-{}.db", uuid::Uuid::new_v4())))
        .unwrap()
}

#[test]
fn test_index_and_search() {
    let mut store = temp_store();
    let meta = test_meta("s1");
    store
        .index("proj", "auth bug in login flow", &meta)
        .unwrap();
    store
        .index("proj", "database migration issue", &meta)
        .unwrap();

    let results = store.search("proj", "auth", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].content.contains("auth"));
}

#[test]
fn test_search_no_results() {
    let mut store = temp_store();
    let results = store.search("proj", "nonexistent", 10).unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_delete_session() {
    let mut store = temp_store();
    store.index("proj", "a", &test_meta("s1")).unwrap();
    store.index("proj", "b", &test_meta("s2")).unwrap();
    store.index("proj", "c", &test_meta("s1")).unwrap();

    let deleted = store.delete_by_session("proj", "s1").unwrap();
    assert_eq!(deleted, 2);

    let remaining = store.search("proj", "b", 10).unwrap();
    assert_eq!(remaining.len(), 1);
}

#[test]
fn test_cleanup_stale() {
    let mut store = temp_store();
    let old_meta = SessionMeta {
        source: "oo".into(),
        session: "s1".into(),
        command: "old".into(),
        timestamp: now_epoch() - 100_000,
    };
    let fresh_meta = test_meta("s1");

    store.index("proj", "old data here", &old_meta).unwrap();
    store.index("proj", "fresh data here", &fresh_meta).unwrap();

    let deleted = store.cleanup_stale("proj", 86400).unwrap();
    assert_eq!(deleted, 1);

    // Only fresh remains
    let results = store.search("proj", "data", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].content.contains("fresh"));
}

#[test]
fn test_metadata_round_trip() {
    let mut store = temp_store();
    let meta = test_meta("sess123");
    store
        .index("proj", "test content for round trip", &meta)
        .unwrap();

    let results = store.search("proj", "round trip", 10).unwrap();
    assert_eq!(results.len(), 1);
    let found_meta = results[0].meta.as_ref().unwrap();
    assert_eq!(found_meta.source, "oo");
    assert_eq!(found_meta.session, "sess123");
    assert_eq!(found_meta.command, "test cmd");
}

#[test]
fn test_recall_short_query() {
    // Queries ≤ 2 chars fall back to LIKE search — must still return results
    let mut store = temp_store();
    let meta = test_meta("s1");
    store
        .index("proj", "ab stands for abstract", &meta)
        .unwrap();

    // 2-char query triggers FTS (length == 2 is still FTS path)
    // 1-char query triggers LIKE fallback
    let results = store.search("proj", "a", 10).unwrap();
    // The single-char LIKE search should find the entry containing "a"
    assert!(!results.is_empty(), "LIKE fallback should find results");
    assert!(results[0].content.contains("abstract"));
}

#[test]
fn test_store_and_recall_roundtrip() {
    // Index an entry and retrieve it — verifies the full index→search cycle
    let mut store = temp_store();
    let meta = test_meta("roundtrip-session");
    let content = "unique_token_for_roundtrip_test_xyz";
    let id = store.index("proj", content, &meta).unwrap();
    assert!(!id.is_empty(), "indexed ID must not be empty");

    let results = store.search("proj", "unique_token", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].content, content);
}

#[test]
fn test_recall_empty_results() {
    // A query that matches nothing in an empty store returns an empty vec
    let mut store = temp_store();
    let results = store
        .search("proj", "definitely_not_present_xyz", 10)
        .unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_search_isolates_projects() {
    // Results from one project must not appear when searching another
    let mut store = temp_store();
    let meta = test_meta("s1");
    store
        .index("project_a", "isolated content alpha", &meta)
        .unwrap();

    let results = store.search("project_b", "isolated", 10).unwrap();
    assert!(
        results.is_empty(),
        "cross-project leakage: results from project_a appeared in project_b"
    );
}

#[test]
fn test_index_returns_unique_ids() {
    // Each indexed entry should get a distinct UUID
    let mut store = temp_store();
    let meta = test_meta("s1");
    let id1 = store.index("proj", "content one", &meta).unwrap();
    let id2 = store.index("proj", "content two", &meta).unwrap();
    assert_ne!(id1, id2, "each index call must return a unique ID");
}

#[test]
fn test_delete_by_session_leaves_other_session() {
    // delete_by_session must not remove entries from other sessions
    let mut store = temp_store();
    store
        .index("proj", "keep this", &test_meta("keep"))
        .unwrap();
    store
        .index("proj", "delete this", &test_meta("remove"))
        .unwrap();

    let deleted = store.delete_by_session("proj", "remove").unwrap();
    assert_eq!(deleted, 1);

    let remaining = store.search("proj", "keep", 10).unwrap();
    assert_eq!(remaining.len(), 1, "entry from kept session must survive");
}

#[test]
fn test_cleanup_stale_preserves_fresh() {
    // cleanup_stale must not delete entries younger than the threshold
    let mut store = temp_store();
    let fresh = test_meta("fresh-session");
    store.index("proj", "fresh_content_xyz", &fresh).unwrap();

    // Nothing is stale (threshold = 1 second, all entries are brand-new)
    let deleted = store.cleanup_stale("proj", 1).unwrap();
    // The fresh entry is at most a few ms old — it must survive
    // (we allow 0 deletions; > 0 would indicate a race, which is acceptable
    // on extremely slow machines, so we just check the content is still there)
    let _ = deleted;
    let results = store.search("proj", "fresh_content", 10).unwrap();
    assert!(!results.is_empty(), "fresh entry must not be cleaned up");
}

#[test]
fn test_search_with_double_quotes_in_query_does_not_panic() {
    // A query containing double-quotes must not cause FTS5 syntax errors or panics.
    // Previously, unescaped quotes would be wrapped as `""token""`, which is invalid FTS5 syntax.
    let mut store = temp_store();
    let meta = test_meta("s1");
    store
        .index("proj", "some searchable content", &meta)
        .unwrap();

    // These queries all contain double-quote characters that could break FTS5 syntax.
    let queries = [
        r#"foo"bar"#,
        r#""quoted""#,
        r#"he said "hello" world"#,
        r#""""#,
    ];
    for query in &queries {
        let result = store.search("proj", query, 10);
        assert!(
            result.is_ok(),
            "search must not return Err for query {query:?}, got: {:?}",
            result.unwrap_err()
        );
        // The result vec itself must be a valid (possibly empty) list — not garbage.
        let results = result.unwrap();
        assert!(
            results.len() <= 1,
            "at most 1 indexed entry can match, got {}",
            results.len()
        );
    }

    // Additionally verify a quote-containing query that matches content actually finds it.
    // Index content with the word "searchable" and query with embedded quotes around it.
    let result = store.search("proj", r#""searchable""#, 10).unwrap();
    // After stripping quotes the token becomes "searchable" — FTS5 should find the entry.
    assert_eq!(
        result.len(),
        1,
        "stripping embedded quotes must still allow FTS5 to find the matching entry"
    );
}

#[test]
fn test_search_with_asterisk_in_query_does_not_panic() {
    // A query containing `*` inside a token (e.g. "foo*bar") must not panic or
    // return an Err. FTS5 phrase-quoting neutralizes * so it is treated as a
    // literal character rather than a prefix-search operator.
    let mut store = temp_store();
    let meta = test_meta("s1");
    store
        .index("proj", "wildcard matching content", &meta)
        .unwrap();

    let queries = [
        "foo*bar", "prefix*", "*suffix", "a*b*c",
        // Standalone * is a 1-char query that falls back to LIKE — must also be safe.
        "*",
    ];
    for query in &queries {
        let result = store.search("proj", query, 10);
        assert!(
            result.is_ok(),
            "search must not Err for query {query:?}, got: {:?}",
            result.unwrap_err()
        );
        // Results must be a valid (possibly empty) vector.
        let _ = result.unwrap();
    }
}

#[test]
fn test_parse_meta_invalid_json_does_not_panic() {
    // Store an entry with corrupt metadata directly via raw SQL, then search.
    // The parse_meta helper silently returns None for invalid JSON — must not panic.
    let mut store = temp_store();
    let meta = test_meta("s1");
    // Insert a valid entry first so FTS is initialised
    store
        .index("proj", "searchable content corrupt meta", &meta)
        .unwrap();

    // Overwrite metadata with corrupt JSON using raw SQL
    store
        .conn
        .execute(
            "UPDATE entries SET metadata = ?1 WHERE project = ?2",
            rusqlite::params!["{invalid json{{", "proj"],
        )
        .unwrap();

    // Search must not panic; the corrupt-meta entry should be silently skipped or
    // returned without a `meta` field (None).
    let results = store.search("proj", "corrupt", 10).unwrap();
    // Either 0 or 1 result — either is acceptable; what is not acceptable is a panic.
    for r in &results {
        // If the entry is found, its meta must be None (corrupt JSON → parse failure)
        assert!(r.meta.is_none(), "corrupt metadata must parse to None");
    }
}

#[test]
fn test_search_short_query_with_metadata() {
    // 1-char queries fall back to LIKE; the returned SearchResult should carry
    // the stored metadata so callers can display session + command info.
    let mut store = temp_store();
    let meta = SessionMeta {
        source: "oo".into(),
        session: "meta-session".into(),
        command: "echo hello".into(),
        timestamp: now_epoch(),
    };
    store
        .index("proj", "abc some content with metadata", &meta)
        .unwrap();

    // 1-char query → LIKE path
    let results = store.search("proj", "a", 10).unwrap();
    assert!(!results.is_empty(), "LIKE fallback must find results");

    let found = results.iter().find(|r| r.content.contains("abc"));
    assert!(found.is_some(), "must find the indexed entry");

    let found_meta = found.unwrap().meta.as_ref();
    assert!(
        found_meta.is_some(),
        "LIKE-path result must include metadata"
    );
    assert_eq!(found_meta.unwrap().command, "echo hello");
    assert_eq!(found_meta.unwrap().session, "meta-session");
}
