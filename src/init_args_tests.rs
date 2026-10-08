//! Unit tests for [`crate::commands::parse_init_mode`] — the single-pass
//! flag parser for `oo init` (issue #171 lens-review fix 2).
//!
//! `parse_action` prints the parse error and calls `std::process::exit(1)`
//! on `Init` failure, so these tests call `parse_init_mode` directly: the
//! parse-time error is the contract (nothing is written before it).
//!
//! Note on stdout capture: `oo init --format bogus` prints its warn through
//! the process's stderr — these unit tests exercise the parse result (and
//! the exact error strings) without capturing output; the behavioral
//! warn-and-fall-back-to-claude case is additionally pinned by
//! `test_init_format_bogus_falls_back_to_claude` in `tests/integration.rs`.

use crate::commands::InitMode;
use crate::commands::parse_init_mode;
use crate::init::InitFormat;

fn args(t: &[&str]) -> Vec<String> {
    t.iter().map(|s| s.to_string()).collect()
}

// ---------------------------------------------------------------------------
// --agent value validation
// ---------------------------------------------------------------------------

/// `oo init --agent` with no value is an error naming the flag and the
/// supported values.
#[test]
fn agent_without_value_is_error() {
    let err = parse_init_mode(&args(&["--agent"])).expect_err("--agent without a value must error");
    assert!(
        err.contains("--agent"),
        "error must name the --agent flag: {err:?}"
    );
    assert!(
        err.contains("pi, claude-code"),
        "error must name the supported values: {err:?}"
    );
}

/// `oo init --agent --global`: the next token is itself a flag, so the
/// value is missing — error naming the flag and supported values.
#[test]
fn agent_followed_by_flag_is_error() {
    let err =
        parse_init_mode(&args(&["--agent", "--global"])).expect_err("flag-like value must error");
    assert!(
        err.contains("--agent"),
        "error must name the --agent flag: {err:?}"
    );
    assert!(
        err.contains("pi, claude-code"),
        "error must name the supported values: {err:?}"
    );
}

// ---------------------------------------------------------------------------
// --format value validation
// ---------------------------------------------------------------------------

/// `oo init --format` with no value alone: unchanged behavior — the value
/// defaults to claude (parse_init_format semantics).
#[test]
fn format_without_value_defaults_to_claude() {
    let mode = parse_init_mode(&args(&["--format"])).expect("no value must fall back to claude");
    assert_eq!(mode, InitMode::Format(InitFormat::Claude));
}

// ---------------------------------------------------------------------------
// mutual exclusion (--agent + --format)
// ---------------------------------------------------------------------------

/// `--agent` plus `--format` is a mutual-exclusion error regardless of flag
/// order or the format's value; nothing is written (parse-time error).
#[test]
fn agent_and_format_mutually_exclusive_any_order_any_value() {
    for flag_order in [
        &["--agent", "pi", "--format", "bogus"][..],
        &["--format", "bogus", "--agent", "pi"][..],
        &["--agent", "pi", "--format", "claude"][..],
        &["--format", "generic", "--agent", "pi"][..],
        &["--agent", "pi", "--format"][..],
    ] {
        let err = parse_init_mode(&args(flag_order))
            .expect_err("combining --agent and --format must error: {flag_order:?}");
        assert!(
            err.contains("--agent and --format"),
            "error must name the conflicting flags ({flag_order:?}): {err:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// --global
// ---------------------------------------------------------------------------

/// `oo init --global` alone (no `--agent`) is an error; nothing is written.
#[test]
fn global_without_agent_is_error() {
    let err =
        parse_init_mode(&args(&["--global"])).expect_err("--global without --agent must error");
    assert!(
        err.contains("--global"),
        "error must name the --global flag: {err:?}"
    );
}

/// `oo init --global --format generic` (no `--agent`) is the same error.
#[test]
fn global_with_format_without_agent_is_error() {
    let err = parse_init_mode(&args(&["--global", "--format", "generic"]))
        .expect_err("--global with --format but no --agent must error");
    assert!(
        err.contains("--global"),
        "error must name the --global flag: {err:?}"
    );
}

/// `oo init --agent pi --global` in either flag order installs the pi
/// extension at the user level.
#[test]
fn agent_pi_global_works_both_orders() {
    for flag_order in [
        &["--agent", "pi", "--global"][..],
        &["--global", "--agent", "pi"][..],
    ] {
        let mode = parse_init_mode(&args(flag_order))
            .expect("--agent pi --global must parse: {flag_order:?}");
        assert_eq!(mode, InitMode::Pi { global: true }, "order {flag_order:?}");
    }
}

// ---------------------------------------------------------------------------
// unknown flags: today's behavior preserved
// ---------------------------------------------------------------------------

/// Unknown extra flags are ignored (today's behavior).
#[test]
fn unknown_flags_are_ignored() {
    let mode = parse_init_mode(&args(&["--agent", "pi", "--bogus"]))
        .expect("unknown flags must be ignored");
    assert_eq!(mode, InitMode::Pi { global: false });
}

/// A trailing unknown flag after `--format` keeps the pure-format path
/// (parse_init_format re-scans the original args).
#[test]
fn unknown_flags_with_format_preserve_format_path() {
    let mode = parse_init_mode(&args(&["--format", "generic", "--bogus"])).expect("must parse");
    assert_eq!(mode, InitMode::Format(InitFormat::Generic));
}

// ---------------------------------------------------------------------------
// backward-compat: plain init and pure --format path
// ---------------------------------------------------------------------------

/// Plain `oo init` — unchanged: claude format.
#[test]
fn plain_init_is_claude() {
    let mode = parse_init_mode(&args(&[])).expect("plain init must parse");
    assert_eq!(mode, InitMode::Format(InitFormat::Claude));
}

/// `--format generic` — unchanged.
#[test]
fn format_generic_unchanged() {
    let mode = parse_init_mode(&args(&["--format", "generic"])).expect("must parse");
    assert_eq!(mode, InitMode::Format(InitFormat::Generic));
}

/// `--format claude` — unchanged.
#[test]
fn format_claude_unchanged() {
    let mode = parse_init_mode(&args(&["--format", "claude"])).expect("must parse");
    assert_eq!(mode, InitMode::Format(InitFormat::Claude));
}

/// `--format bogus` alone — unchanged: warn (stderr) and fall back to claude.
#[test]
fn format_bogus_alone_falls_back_to_claude() {
    let mode = parse_init_mode(&args(&["--format", "bogus"])).expect("bogus format must parse");
    assert_eq!(
        mode,
        InitMode::Format(InitFormat::Claude),
        "unknown --format value must fall back to claude"
    );
}

/// `--agent pi` — unchanged: selects the pi installer, project scope.
#[test]
fn agent_pi_selects_pi_installer() {
    let mode = parse_init_mode(&args(&["--agent", "pi"])).expect("must parse");
    assert_eq!(mode, InitMode::Pi { global: false });
}

/// `--agent claude-code` — supported by name, not yet implemented.
#[test]
fn agent_claude_code_not_yet_supported() {
    let err = parse_init_mode(&args(&["--agent", "claude-code"]))
        .expect_err("claude-code must not be implemented yet");
    assert!(err.contains("not yet supported"), "message: {err:?}");
}

/// Unknown agent values error naming the supported values.
#[test]
fn agent_unknown_value_errors_naming_supported() {
    let err = parse_init_mode(&args(&["--agent", "cursor"])).expect_err("unknown agent must error");
    assert!(
        err.contains("unknown --agent value 'cursor'"),
        "message: {err:?}"
    );
    assert!(
        err.contains("pi, claude-code"),
        "error must name the supported values: {err:?}"
    );
}
