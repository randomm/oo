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
use std::os::unix::fs::PermissionsExt;
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

// ---------------------------------------------------------------------------
// Savings suffix regression locks (issue #150) — cmd_learn path
//
// `oo learn` shares the same `render_classification` helper as `cmd_run`.
// These tests verify the Large and Bounded arm wording is byte-for-byte
// unchanged through the cmd_learn path (no `[saved …]` suffix on either),
// and that the quiet-success suffix does appear there.
// ---------------------------------------------------------------------------

/// Large arm via cmd_learn: the `● ls (indexed … → use `oo recall` to query)`
/// string is unchanged — no `[saved …]` suffix.
#[test]
fn test_learn_large_arm_wording_unchanged() {
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
    let data_dir = TempDir::new().unwrap();

    oo().args(["learn", "ls", "-la", dir.path().to_str().unwrap()])
        .env("OO_DATA_DIR", data_dir.path())
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env_remove("CEREBRAS_API_KEY")
        .assert()
        .success()
        // The Large arm's exact wording is unchanged: indexed + use `oo recall`.
        .stdout(predicate::str::contains("indexed"))
        .stdout(predicate::str::contains("use `oo recall` to query"))
        // The savings suffix must NOT appear on the Large arm.
        .stdout(predicate::str::contains("[saved ").not());
}

/// Bounded arm via cmd_learn: the `● {label} (output truncated: …)` string is
/// unchanged — no `[saved …]` suffix.
#[test]
fn test_learn_bounded_arm_wording_unchanged() {
    let dir = TempDir::new().unwrap();
    let big_file = dir.path().join("big.txt");
    let content = std::iter::repeat("x\n").take(5000).collect::<String>();
    std::fs::write(&big_file, &content).unwrap();
    let data_dir = TempDir::new().unwrap();

    oo().args(["learn", "cat", big_file.to_str().unwrap()])
        .env("OO_DATA_DIR", data_dir.path())
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env_remove("CEREBRAS_API_KEY")
        .assert()
        .success()
        // The Bounded arm's exact wording is unchanged: output truncated + recall.
        .stdout(predicate::str::contains("output truncated:"))
        .stdout(predicate::str::contains("use `oo recall` to query"))
        // The savings suffix must NOT appear on the Bounded arm.
        .stdout(predicate::str::contains("[saved ").not());
}

/// Quiet Success via cmd_learn: a Status-category command with >4 KB output
/// and no matching builtin pattern produces `✓ {label} [saved …]`.
#[test]
fn test_learn_quiet_success_shows_savings() {
    let dir = TempDir::new().unwrap();
    let bin_dir = dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let go_script = bin_dir.join("go");
    std::fs::write(
        &go_script,
        "#!/bin/sh\nfor i in $(seq 1 500); do echo 'building component $i ...'; done\nexit 0\n",
    )
    .unwrap();
    std::fs::set_permissions(&go_script, std::fs::Permissions::from_mode(0o755)).unwrap();

    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let data_dir = TempDir::new().unwrap();

    oo().args(["learn", "go", "build"])
        .env("PATH", path)
        .env("OO_DATA_DIR", data_dir.path())
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env_remove("CEREBRAS_API_KEY")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("\u{2713}"))
        // The savings suffix must appear on the indicator line.
        .stdout(predicate::str::contains("[saved "))
        .stdout(predicate::function(|out: &str| {
            let first_line = out.lines().next().unwrap_or("");
            first_line.starts_with('\u{2713}') && first_line.contains("[saved ")
        }));
}
