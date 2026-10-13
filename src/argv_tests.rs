//! Unit tests for the OS-string argv helpers (issue #213).

use crate::argv::{NON_UTF8_ARG_ERR, lossy_join, utf8_or};
use std::ffi::OsString;

#[cfg(unix)]
use std::os::unix::ffi::OsStringExt as _;

/// Non-UTF-8 bytes are rejected with the fixed message, never converted.
#[cfg(unix)]
#[test]
fn utf8_or_rejects_invalid_bytes() {
    let arg = OsString::from_vec(vec![0xff, 0xfe]);
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
