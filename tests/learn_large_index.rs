//! Integration test for the `oo learn` Large-arm behaviour change (issue #151).
//!
//! Guards that when `oo learn <data-cmd>` produces >4 KB of output, the shared
//! Large arm really calls `try_index` (pre-#151 the cmd_learn Large arm printed
//! "indexed" without ever indexing). The observable effect is the "● ... use
//! `oo recall`" indexed marker on stdout; if indexing fails the fallback
//! `smart_truncate` marker ("[N lines truncated]") appears instead.
//!
//! The background LLM child is prevented from contacting any provider by
//! clearing all known API-key env vars before spawn.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn oo() -> Command {
    Command::cargo_bin("oo").unwrap()
}

/// Data-category command (`ls`) with >4 KB of output → Large arm must index
/// or fall back to smart_truncate — never silently passthrough.
#[test]
fn test_learn_large_data_output_indexes_or_falls_back() {
    let dir = TempDir::new().unwrap();
    // 200 long filenames guarantee the `ls -la` listing exceeds 4096 bytes.
    for i in 0..200 {
        std::fs::write(
            dir.path().join(format!(
                "some_long_filename_number_{i:04}_padding_padding_padding.txt"
            )),
            "x\n",
        )
        .unwrap();
    }

    // OO_DATA_DIR points the SQLite store at the temp dir so the indexed
    // branch is deterministic and the developer's real store is untouched.
    let data_dir = TempDir::new().unwrap();

    let assert = oo()
        .args(["learn", "ls", "-la", dir.path().to_str().unwrap()])
        .env("OO_DATA_DIR", data_dir.path())
        // Prevent the detached background LLM child from contacting a provider.
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env_remove("CEREBRAS_API_KEY")
        .assert()
        .success()
        // The indexed branch (the one #151 changed) prints
        // "● ls (indexed X → use `oo recall` to query)". If the store ever
        // fails to open, the fallback branch would emit the smart_truncate
        // marker instead — both prove the shared Large arm ran.
        .stdout(
            predicate::str::contains("use `oo recall")
                .or(predicate::str::contains("lines truncated")),
        );

    // The indexed marker only prints when `store.index` succeeded, so the
    // temp store must now contain the ls output — the actual, observable
    // effect of the behaviour change.
    let db = data_dir.path().join("oo.db");
    assert!(
        db.exists() && db.metadata().unwrap().len() > 0,
        "store db must exist and be non-empty after indexing"
    );
}

/// Same Large arm with the store made unusable (unreadable parent dir) so
/// `try_index` fails and the smart_truncate fallback branch is exercised.
#[test]
fn test_learn_large_data_output_falls_back_when_store_unavailable() {
    let dir = TempDir::new().unwrap();
    for i in 0..200 {
        std::fs::write(
            dir.path().join(format!(
                "some_long_filename_number_{i:04}_padding_padding_padding.txt"
            )),
            "x\n",
        )
        .unwrap();
    }

    // Point the store at a path whose parent is a regular FILE — open() must
    // fail (create_dir_all can't replace an existing file), forcing the
    // fallback branch.
    let blocker = TempDir::new().unwrap();
    let bad = blocker.path().join("blocker-file");
    std::fs::write(&bad, "x").unwrap();

    oo().args(["learn", "ls", "-la", dir.path().to_str().unwrap()])
        .env("OO_DATA_DIR", bad)
        // Prevent the detached background LLM child from contacting a provider.
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env_remove("CEREBRAS_API_KEY")
        .assert()
        .success()
        .stdout(predicate::str::contains("lines truncated"));
}
