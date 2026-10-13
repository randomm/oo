//! Unit tests for the OS-string argv helpers (issue #213).

use crate::argv::{NON_UTF8_ARG_ERR, lossy_join, reject_non_utf8_reserved_args, utf8_or};
use std::collections::BTreeSet;
use std::ffi::OsString;

#[cfg(unix)]
use std::os::unix::ffi::OsStringExt as _;

/// Non-UTF-8 bytes are rejected with the fixed message, never converted.
#[cfg(unix)]
#[test]
fn utf8_or_rejects_invalid_bytes() {
    let arg = OsString::from_vec(vec![0xff, 0xfe, 0x41]);
    assert_eq!(utf8_or(&arg), Err(NON_UTF8_ARG_ERR));
}

/// Valid UTF-8 is returned verbatim.
#[test]
fn utf8_or_returns_text_for_valid_input() {
    let arg = OsString::from("recall query");
    assert_eq!(utf8_or(&arg), Ok("recall query".to_string()));
}

/// Empty arguments are valid (and empty).
#[test]
fn utf8_or_accepts_empty_argument() {
    assert_eq!(utf8_or(&OsString::new()), Ok(String::new()));
}

/// The lossy join converts each argument independently: invalid sequences in
/// one argument do not affect the others.
#[cfg(unix)]
#[test]
fn lossy_join_replaces_invalid_bytes_per_argument() {
    let args = vec![
        OsString::from("sh"),
        OsString::from_vec(vec![0xff]),
        OsString::from("x"),
    ];
    assert_eq!(lossy_join(&args), "sh \u{fffd} x");
}

/// A single argument joins to itself.
#[test]
fn lossy_join_single_argument() {
    let args = vec![OsString::from("echo hi")];
    assert_eq!(lossy_join(&args), "echo hi");
}

/// Valid UTF-8 arguments pass through the lossy join unchanged (all platforms).
#[test]
fn lossy_join_leaves_valid_utf8_unchanged() {
    let args = vec![
        OsString::from("sh"),
        OsString::from("-c"),
        OsString::from("echo hi"),
    ];
    assert_eq!(lossy_join(&args), "sh -c echo hi");
}

/// A non-UTF-8 first word is simply the command to run: it can never match
/// a reserved name, so the arguments pass through as bytes.
#[cfg(unix)]
#[test]
fn reject_non_utf8_non_utf8_first_word_ok() {
    let args = vec![OsString::from_vec(vec![0xff]), OsString::from("x")];
    assert_eq!(reject_non_utf8_reserved_args(&args), Ok(()));
}

/// A non-reserved first word passes through: a non-UTF-8 later argument is
/// part of the wrapped command, not an oo-owned option value.
#[cfg(unix)]
#[test]
fn reject_non_utf8_non_reserved_first_word_ok() {
    let args = vec![
        OsString::from("sh"),
        OsString::from("-c"),
        OsString::from_vec(vec![0xff]),
    ];
    assert_eq!(reject_non_utf8_reserved_args(&args), Ok(()));
}

/// A reserved first word (other than `learn`) makes every remaining argument
/// oo-owned, so a non-UTF-8 text argument is rejected.
#[cfg(unix)]
#[test]
fn reject_non_utf8_reserved_first_word_with_bad_arg() {
    let args = vec![OsString::from("recall"), OsString::from_vec(vec![0xff])];
    assert_eq!(reject_non_utf8_reserved_args(&args), Err(NON_UTF8_ARG_ERR));
}

/// `learn` is special-cased: a non-UTF-8 value after `--hint` is rejected,
/// while a non-UTF-8 wrapped-command word passes through (as does a valid
/// hint).
#[cfg(unix)]
#[test]
fn reject_non_utf8_learn_hint_rejected_wrapped_ok() {
    let bad_hint = vec![
        OsString::from("learn"),
        OsString::from("--hint"),
        OsString::from_vec(vec![0xff]),
        OsString::from("echo"),
    ];
    assert_eq!(
        reject_non_utf8_reserved_args(&bad_hint),
        Err(NON_UTF8_ARG_ERR)
    );

    let ok_wrapped = vec![
        OsString::from("learn"),
        OsString::from("--hint"),
        OsString::from("compress"),
        OsString::from_vec(vec![0xff]),
    ];
    assert_eq!(reject_non_utf8_reserved_args(&ok_wrapped), Ok(()));

    let ok_hint = vec![
        OsString::from("learn"),
        OsString::from("--hint"),
        OsString::from("compress"),
        OsString::from("echo"),
    ];
    assert_eq!(reject_non_utf8_reserved_args(&ok_hint), Ok(()));
}

/// Sync guard: `argv::RESERVED_SUBCOMMANDS` must stay identical to the
/// reserved names in the `USAGE` table in `usage.rs`; drift means a
/// subcommand was added to one list but not the other, which this test catches.
#[test]
fn reserved_subcommands_match_usage_table() {
    let argv_names = crate::argv::RESERVED_SUBCOMMANDS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let usage_names = crate::usage::reserved_names().collect::<BTreeSet<_>>();
    assert_eq!(
        argv_names, usage_names,
        "a subcommand was added to one reserved-name list but not the other: argv = {:?}, usage = {:?}",
        argv_names, usage_names
    );
}
