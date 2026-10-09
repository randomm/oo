//! CLI behaviour for `--help`/`-h` on reserved subcommands, `oo help <sub>`,
//! and strict `oo init` parsing (issue #178).
//!
//! Every test runs in a throwaway git repo with all state directories pointed
//! at temp dirs, so nothing touches the real repo, HOME, or the data store.

use assert_cmd::Command;
use double_o::usage::reserved_names;
use predicates::prelude::*;
use tempfile::TempDir;

/// A hermetic `oo` invocation rooted in `repo` (a temp dir with `.git`).
fn oo_in(repo: &TempDir, data: &TempDir, home: &TempDir) -> Command {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("oo");
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
    std::fs::read_dir(dir)
        .expect("read test dir")
        .map(|e| e.expect("read test dir entry"))
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

#[test]
fn reserved_help_flags_print_usage_with_no_side_effects() {
    for sub in reserved_names() {
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
        vec!["init", "--format", "bogus"],
    ] {
        let (repo, data, home) = fixture();
        oo_in(&repo, &data, &home)
            .args(&case)
            .assert()
            .code(1)
            .stderr(predicate::str::starts_with("oo"));
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
    // `oo sh -c '...' sh --help` must run the shell, not print oo usage. Going
    // through `sh` keeps the check portable: `echo --help` is intercepted by
    // GNU echo (Linux CI) and prints its own usage, unlike BSD echo (macOS).
    oo_in(&repo, &data, &home)
        .args(["sh", "-c", "printf '%s\\n' \"$1\"", "sh", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::diff("--help\n"))
        .stdout(predicate::str::contains("Usage: oo").not());
}

#[test]
fn no_args_usage_has_exact_init_line() {
    let (repo, data, home) = fixture();
    oo_in(&repo, &data, &home)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "  init [--format claude|generic | --agent pi|claude-code [--global]]   Set up hooks for agent frameworks (see: oo init --help)\n",
        ));
}

#[test]
fn unsupported_format_error_is_exact_and_writes_nothing() {
    let (repo, data, home) = fixture();
    oo_in(&repo, &data, &home)
        .args(["init", "--format", "x"])
        .assert()
        .code(1)
        .stdout("")
        .stderr("oo init: unsupported format 'x' (supported: claude, generic)\n");
    assert_eq!(entries(repo.path()), 0, "unsupported format wrote files");
}

#[test]
fn top_level_help_has_exact_init_summary() {
    // The clap doc comment is the source of `oo --help`. Clap may re-wrap the
    // rendered text, so assert the exact source line and the unwrapped tokens.
    const LINE: &str = "init (set up agent hooks: --format claude|generic, --agent pi|claude-code [--global]; see oo init --help),";
    let main_src = include_str!("../src/main.rs");
    assert!(
        main_src.contains(LINE),
        "doc comment source lacks the init line"
    );
    let (repo, data, home) = fixture();
    let out = oo_in(&repo, &data, &home)
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let rendered: String = String::from_utf8(out)
        .unwrap()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for token in [
        "init (set up agent hooks:",
        "--format claude|generic,",
        "--agent pi|claude-code [--global];",
        "see oo init --help),",
    ] {
        assert!(rendered.contains(token), "missing {token:?} in --help");
    }
}
