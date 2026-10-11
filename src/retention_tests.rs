//! Tests for `src/retention.rs`: config parsing and the purge logic.
use super::*;
use crate::store::{SessionMeta, SqliteStore};
use crate::util::now_epoch;

#[test]
fn parse_missing_store_section_uses_default() {
    assert_eq!(parse_retention("").unwrap(), DEFAULT_RETENTION_SECS);
    assert_eq!(
        parse_retention("[learn]\nprovider = \"anthropic\"\n").unwrap(),
        DEFAULT_RETENTION_SECS
    );
    assert_eq!(
        parse_retention("[store]\n").unwrap(),
        DEFAULT_RETENTION_SECS
    );
}

#[test]
fn parse_custom_value() {
    assert_eq!(
        parse_retention("[store]\nretention = 3600\n").unwrap(),
        3600
    );
}

#[test]
fn parse_zero_disables_purge() {
    assert_eq!(parse_retention("[store]\nretention = 0\n").unwrap(), 0);
}

#[test]
fn parse_negative_is_rejected() {
    assert!(parse_retention("[store]\nretention = -1\n").is_err());
}

#[test]
fn parse_value_beyond_i64_is_rejected() {
    assert!(parse_retention("[store]\nretention = 18446744073709551615\n").is_err());
}

#[test]
fn parse_unknown_key_in_store_is_rejected() {
    assert!(parse_retention("[store]\nretension = 0\n").is_err());
}

#[test]
fn unknown_key_in_store_warns_and_falls_back_to_default() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[store]\nretension = 0\n").unwrap();
    assert!(read_retention(&path).is_err());
    assert_eq!(load_retention_from(&path), DEFAULT_RETENTION_SECS);
}

#[test]
fn unknown_top_level_section_is_still_ignored() {
    assert_eq!(
        parse_retention("[other]\nretension = 0\n[store]\nretention = 60\n").unwrap(),
        60
    );
}

#[test]
fn parse_non_integer_is_rejected() {
    assert!(parse_retention("[store]\nretention = \"7d\"\n").is_err());
    assert!(parse_retention("[store]\nretention = 1.5\n").is_err());
}

#[test]
fn parse_malformed_toml_is_rejected() {
    assert!(parse_retention("[store\nretention = 1").is_err());
}

#[test]
fn load_missing_file_uses_default() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        load_retention_from(&dir.path().join("config.toml")),
        DEFAULT_RETENTION_SECS
    );
}

#[test]
fn load_unreadable_path_falls_back_to_default() {
    // A directory named config.toml fails read_to_string (not NotFound).
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::create_dir(&path).unwrap();
    assert_eq!(load_retention_from(&path), DEFAULT_RETENTION_SECS);
}

#[test]
fn load_invalid_value_falls_back_to_default() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[store]\nretention = -5\n").unwrap();
    assert_eq!(load_retention_from(&path), DEFAULT_RETENTION_SECS);
}

#[test]
fn load_resolves_config_toml_inside_config_dir() {
    // Pure path resolution: no process env is read or written.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("config.toml"), "[store]\nretention = 42\n").unwrap();
    let path = crate::config::config_dir_from(Some(dir.path().into())).join("config.toml");
    assert_eq!(load_retention_from(&path), 42);
}

/// Fixed project id: purge tests never run `git` to derive one.
const PROJECT: &str = "retention-test-project";
const DAY: i64 = 86_400;

/// Open a store in a fresh temp dir (no env vars, no thread-local override).
fn temp_store() -> (tempfile::TempDir, SqliteStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = SqliteStore::open_at(&dir.path().join("oo.db")).unwrap();
    (dir, store)
}

/// Seed an entry whose timestamp is `age_secs` in the past.
fn seed_aged(store: &mut SqliteStore, project: &str, marker: &str, age_secs: i64) {
    let meta = SessionMeta {
        source: "oo".into(),
        session: "test".into(),
        command: "seed".into(),
        timestamp: now_epoch() - age_secs,
    };
    store.index(project, marker, &meta).unwrap();
}

fn survives(store: &mut SqliteStore, project: &str, marker: &str) -> bool {
    !store.search(project, marker, 10).unwrap().is_empty()
}

#[test]
fn purge_default_keeps_entries_aged_25h_to_6d() {
    let (_dir, mut store) = temp_store();
    seed_aged(&mut store, PROJECT, "zqxmarkerdefault25h", 25 * 3600);
    seed_aged(&mut store, PROJECT, "zqxmarkerdefault6d", 6 * DAY);
    purge_with(&mut store, PROJECT, DEFAULT_RETENTION_SECS);
    assert!(survives(&mut store, PROJECT, "zqxmarkerdefault25h"));
    assert!(survives(&mut store, PROJECT, "zqxmarkerdefault6d"));
}

#[test]
fn purge_default_purges_entry_aged_eight_days() {
    let (_dir, mut store) = temp_store();
    seed_aged(&mut store, PROJECT, "zqxmarkerdefaultpurge", 8 * DAY);
    purge_with(&mut store, PROJECT, DEFAULT_RETENTION_SECS);
    assert!(!survives(&mut store, PROJECT, "zqxmarkerdefaultpurge"));
}

#[test]
fn purge_custom_retention_purges_entry_older_than_it() {
    let (_dir, mut store) = temp_store();
    seed_aged(&mut store, PROJECT, "zqxmarkercustompurge", 2 * DAY);
    purge_with(&mut store, PROJECT, DAY);
    assert!(!survives(&mut store, PROJECT, "zqxmarkercustompurge"));
}

#[test]
fn purge_zero_retention_never_purges() {
    let (_dir, mut store) = temp_store();
    seed_aged(&mut store, PROJECT, "zqxmarkerzeroforever", 30 * DAY);
    purge_with(&mut store, PROJECT, 0);
    assert!(survives(&mut store, PROJECT, "zqxmarkerzeroforever"));
}

#[test]
fn purge_leaves_other_projects_untouched() {
    let (_dir, mut store) = temp_store();
    seed_aged(&mut store, "other-project", "zqxmarkerotherproj", 30 * DAY);
    purge_with(&mut store, PROJECT, DAY);
    assert!(survives(&mut store, "other-project", "zqxmarkerotherproj"));
}
