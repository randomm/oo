//! Tests for `src/retention.rs`: config parsing and the `try_index` purge path.
use super::*;
use crate::store::SessionMeta;
use crate::util::now_epoch;

// OO_CONFIG_DIR is process-global; every test that mutates it takes this lock.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn env_guard() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Run `f` with OO_CONFIG_DIR pointing at a temp dir holding `config` (if any).
fn with_config<F: FnOnce()>(config: Option<&str>, f: F) {
    let _lock = env_guard();
    let cfg_dir = tempfile::tempdir().unwrap();
    if let Some(content) = config {
        std::fs::write(cfg_dir.path().join("config.toml"), content).unwrap();
    }
    unsafe { std::env::set_var("OO_CONFIG_DIR", cfg_dir.path()) };
    f();
    unsafe { std::env::remove_var("OO_CONFIG_DIR") };
}

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
fn parse_default_is_seven_days() {
    assert_eq!(DEFAULT_RETENTION_SECS, 7 * 24 * 60 * 60);
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
fn load_invalid_value_falls_back_to_default() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[store]\nretention = -5\n").unwrap();
    assert_eq!(load_retention_from(&path), DEFAULT_RETENTION_SECS);
}

#[test]
fn load_reads_config_dir_override() {
    with_config(Some("[store]\nretention = 42\n"), || {
        assert_eq!(load_retention(), 42);
    });
    with_config(None, || {
        assert_eq!(load_retention(), DEFAULT_RETENTION_SECS);
    });
}

/// Seed an oo entry whose timestamp is `age_secs` in the past.
fn seed_aged(marker: &str, age_secs: i64) {
    let mut store = crate::store::open().unwrap();
    let meta = SessionMeta {
        source: "oo".into(),
        session: "test".into(),
        command: "seed".into(),
        timestamp: now_epoch() - age_secs,
    };
    store
        .index(&crate::session::project_id(), marker, &meta)
        .unwrap();
}

fn survives(marker: &str) -> bool {
    let mut store = crate::store::open().unwrap();
    !store
        .search(&crate::session::project_id(), marker, 10)
        .unwrap()
        .is_empty()
}

const DAY: i64 = 86_400;

#[test]
fn try_index_default_keeps_entry_aged_two_days() {
    let dir = tempfile::tempdir().unwrap();
    let _data = crate::store::set_test_data_dir(dir.path());
    with_config(None, || {
        seed_aged("zqxmarkerdefaultkeep", 2 * DAY);
        assert!(crate::commands::try_index(
            "cmd",
            "zqxmarkerdefaultkeep body"
        ));
        assert!(survives("zqxmarkerdefaultkeep"));
    });
}

#[test]
fn try_index_default_purges_entry_aged_eight_days() {
    let dir = tempfile::tempdir().unwrap();
    let _data = crate::store::set_test_data_dir(dir.path());
    with_config(None, || {
        seed_aged("zqxmarkerdefaultpurge", 8 * DAY);
        assert!(crate::commands::try_index("cmd", "unrelated body"));
        assert!(!survives("zqxmarkerdefaultpurge"));
    });
}

#[test]
fn try_index_custom_retention_purges_entry_older_than_it() {
    let dir = tempfile::tempdir().unwrap();
    let _data = crate::store::set_test_data_dir(dir.path());
    with_config(Some("[store]\nretention = 86400\n"), || {
        seed_aged("zqxmarkercustompurge", 2 * DAY);
        assert!(crate::commands::try_index("cmd", "unrelated body"));
        assert!(!survives("zqxmarkercustompurge"));
    });
}

#[test]
fn try_index_zero_retention_never_purges() {
    let dir = tempfile::tempdir().unwrap();
    let _data = crate::store::set_test_data_dir(dir.path());
    with_config(Some("[store]\nretention = 0\n"), || {
        seed_aged("zqxmarkerzeroforever", 30 * DAY);
        assert!(crate::commands::try_index("cmd", "unrelated body"));
        assert!(survives("zqxmarkerzeroforever"));
    });
}

#[test]
fn try_index_invalid_config_still_indexes() {
    let dir = tempfile::tempdir().unwrap();
    let _data = crate::store::set_test_data_dir(dir.path());
    with_config(Some("[store]\nretention = -1\n"), || {
        seed_aged("zqxmarkerinvalidkeep", 2 * DAY);
        assert!(crate::commands::try_index(
            "cmd",
            "zqxmarkerinvalidnew body"
        ));
        assert!(survives("zqxmarkerinvalidkeep"));
        assert!(survives("zqxmarkerinvalidnew"));
    });
}
