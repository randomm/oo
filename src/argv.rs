//! OS-string argv helpers (issue #213).
//!
//! `oo` reads its command line with `std::env::args_os`, so no argument is
//! ever converted with a panicking API. On unix, wrapped-command arguments
//! are passed to the child verbatim as [`OsString`] bytes; text that `oo`
//! itself consumes (reserved-subcommand names and arguments, labels, history)
//! is converted with [`OsStr::to_string_lossy`] (D4) or rejected with a
//! single helper (D3).

use std::ffi::OsString;

/// The stderr line printed when an argument that `oo` itself must interpret
/// is not valid UTF-8 (D3).
pub const NON_UTF8_ARG_ERR: &str = "oo: argument is not valid UTF-8";

/// The reserved subcommand names that `oo` itself handles. A non-UTF-8 first
/// word that is not in this set is simply the command to run (D3).
pub const RESERVED_SUBCOMMANDS: &[&str] = &[
    "recall", "forget", "learn", "version", "help", "init", "patterns", "rewrite", "hook",
];

/// The argument's bytes converted lossily (invalid sequences become U+FFFD).
///
/// This is the form `oo` records for the command line: label, classification,
/// history, and the learn payload all use it, so a non-UTF-8 wrapped argument
/// never panics (D4).
pub fn lossy_display(arg: &OsString) -> String {
    arg.to_string_lossy().into_owned()
}

/// The argument's text, when it is valid UTF-8; `Err(NON_UTF8_ARG_ERR)`
/// otherwise.
///
/// Reserved subcommand names and `oo`'s own option values are matched only
/// as valid UTF-8 (D3): `OsStr::to_str` is `None` for non-UTF-8 content on
/// every platform, so this check runs on the OS string before any lossy
/// conversion, identically on unix and non-unix.
pub fn utf8_or(arg: &OsString) -> Result<String, &'static str> {
    arg.to_str().map(str::to_string).ok_or(NON_UTF8_ARG_ERR)
}

/// Join OS-string arguments with spaces, converting each lossily, for the
/// command line recorded in labels, history, and the learn payload (D4).
pub fn lossy_join(args: &[OsString]) -> String {
    args.iter()
        .map(|a| a.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ")
}
