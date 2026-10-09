//! CLI behaviour for `--help`/`-h` on reserved subcommands, `oo help <sub>`,
//! and strict `oo init` parsing (issue #178).
//!
//! Every test runs in a throwaway git repo with all state directories pointed
//! at temp dirs, so nothing touches the real repo, HOME, or the data store.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

/// A hermetic `oo` invocation rooted in `repo` (a temp dir with `.git`).
fn oo_in(repo: &TempDir, data: &TempDir, home: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("oo").unwrap();
    cmd.current_dir(repo.path())
        .env("OO_DATA_DIR", data.path())
        .env("HOME", home.path())
        .env("OO_CLAUDE_DIR", home.path().join(".claude"))
        .env("OO_PI_EXTENSIONS_DIR", home.path().join("pi-ext"));
    cmd
}

/// Temp repo with a `.git` dir, a data dir, and a home dir.
fn fixture() -> (TempDir, TempDir, TempDir) {
    let repo = TempDir::new().unwrap();
    std::fs::create_dir(repo.path().join(".git")).unwrap();
    (repo, TempDir::new().unwrap(), TempDir::new().unwrap())
}

/// Count entries in a directory tree (files and dirs), excluding `.git`.
fn entries(dir: &std::path::Path) -> usize {
    let Ok(read) = std::fs::read_dir(dir) else {
        return 0;
    };
    read.flatten()
        .filter(|e| e.file_name() != ".git")
        .map(|e| {
            1 + if e.path().is_dir() {
                entries(&e.path())
            } else {
                0
            }
        })
        .sum()
}

const RESERVED: &[&str] = &[
    "recall", "forget", "learn", "init", "patterns", "rewrite", "hook", "version",
];

#[test]
fn reserved_help_flags_print_usage_with_no_side_effects() {
    for sub in RESERVED {
        for flag in ["--help", "-h"] {
            let (repo, data, home) = fixture();
            std::fs::write(data.path().join("sentinel"), "keep").unwrap();
            oo_in(&repo, &data, &home)
                .args([sub, flag])
                .assert()
                .success()
                .stdout(predicate::str::contains(format!("oo {sub}")));
            assert_eq!(entries(repo.path()), 0, "`oo {sub} {flag}` wrote files");
            assert_eq!(entries(home.path()), 0, "`oo {sub} {flag}` touched HOME");
            assert!(
                data.path().join("sentinel").exists(),
                "`oo {sub} {flag}` touched the data dir"
            );
        }
    }
}

#[test]
fn forget_help_keeps_indexed_data() {
    let (repo, data, home) = fixture();
    std::fs::write(data.path().join("sentinel"), "keep").unwrap();
    oo_in(&repo, &data, &home)
        .args(["forget", "--help"])
        .assert()
        .success();
    assert!(data.path().join("sentinel").exists());
}

#[test]
fn help_subcommand_prints_local_usage_for_reserved_names() {
    let (repo, data, home) = fixture();
    oo_in(&repo, &data, &home)
        .args(["help", "init"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "oo init [--format claude|generic]",
        ))
        .stdout(predicate::str::contains("--agent pi|claude-code"))
        .stdout(predicate::str::contains("OO_PI_EXTENSIONS_DIR"))
        .stdout(predicate::str::contains("OO_CLAUDE_DIR"));
    assert_eq!(entries(repo.path()), 0);
}

#[test]
fn init_rejects_unknown_options_and_writes_nothing() {
    for case in [
        vec!["init", "--bogus"],
        vec!["init", "--format"],
        vec!["init", "--format", "--global"],
        vec!["init", "stray"],
    ] {
        let (repo, data, home) = fixture();
        oo_in(&repo, &data, &home)
            .args(&case)
            .assert()
            .code(1)
            .stderr(predicate::str::contains("oo: "));
        assert_eq!(entries(repo.path()), 0, "`oo {case:?}` wrote files");
    }
}

#[test]
fn init_unknown_option_error_names_option_and_help() {
    let (repo, data, home) = fixture();
    oo_in(&repo, &data, &home)
        .args(["init", "--bogus"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "unknown option '--bogus' (try: oo init --help)",
        ));
}

#[test]
fn plain_init_still_writes_hooks_json() {
    let (repo, data, home) = fixture();
    oo_in(&repo, &data, &home).arg("init").assert().success();
    let written = std::fs::read_to_string(repo.path().join(".claude/hooks.json")).unwrap();
    assert_eq!(written, double_o::init::HOOKS_JSON);
}

#[test]
fn non_reserved_command_with_help_flag_still_executes() {
    let (repo, data, home) = fixture();
    // `oo echo --help` must run `echo`, not print oo usage.
    oo_in(&repo, &data, &home)
        .args(["echo", "--help"])
        .assert()
        .stdout(predicate::str::contains("Usage: oo").not());
}
