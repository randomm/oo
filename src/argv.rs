//! OS-string argv helpers (issue #213).
//!
//! `oo` reads its command line with `std::env::args_os`, so no argument is
//! ever converted with a panicking API. On unix, wrapped-command arguments
//! are passed to the child verbatim as [`OsString`] bytes; text that `oo`
//! itself consumes (reserved-subcommand names and arguments, labels, history)
//! is converted with [`OsStr::to_string_lossy`] (D4) or rejected with a
//! single helper (D3).

use std::ffi::OsString;

#[cfg(unix)]
#[allow(unused_imports)]
use std::os::unix::ffi::OsStringExt as _;

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
/// as valid UTF-8 (D3): the check runs on the OS string before any lossy
/// conversion, on all platforms. On unix the check is byte-exact (the OS
/// string is a byte vector); on non-unix it is approximate (the lossy
/// encoding is the platform representation).
pub fn utf8_or(arg: &OsString) -> Result<String, &'static str> {
    #[cfg(unix)]
    {
        // On unix the OS string is a byte vector, so the check is byte-exact.
        let bytes = arg.as_encoded_bytes();
        std::str::from_utf8(bytes)
            .map(|s| s.to_string())
            .map_err(|_| NON_UTF8_ARG_ERR)
    }
    #[cfg(not(unix))]
    {
        // On non-unix the lossy encoding is the platform representation;
        // the check is approximate (valid UTF-8 passes, invalid bytes fail).
        let lossy = arg.to_string_lossy().into_owned();
        std::str::from_utf8(lossy.as_bytes())
            .map(|s| s.to_string())
            .map_err(|_| NON_UTF8_ARG_ERR)
    }
}

/// Join OS-string arguments with spaces, converting each lossily, for the
/// command line recorded in labels, history, and the learn payload (D4).
pub fn lossy_join(args: &[OsString]) -> String {
    args.iter()
        .map(|a| a.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ")
}
