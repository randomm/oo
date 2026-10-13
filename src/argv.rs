//! OS-string argv helpers (issue #213).
//!
//! `oo` reads its command line with `std::env::args_os`, so no argument is
//! ever converted with a panicking API. On unix, wrapped-command arguments
//! are passed to the child verbatim as [`OsString`] bytes; text that `oo`
//! itself consumes (reserved-subcommand names and arguments, labels, history)
//! is converted lossily with `OsStr::to_string_lossy` (see #213) or rejected
//! by [`reject_non_utf8_reserved_args`] (see #213).

use std::ffi::OsString;

/// The stderr line printed when an argument that `oo` itself must interpret
/// is not valid UTF-8 (see #213).
pub const NON_UTF8_ARG_ERR: &str = "oo: argument is not valid UTF-8";

/// The reserved subcommand names that `oo` itself handles. A non-UTF-8 first
/// word that is not in this set is simply the command to run (see #213).
pub const RESERVED_SUBCOMMANDS: &[&str] = &[
    "recall", "forget", "learn", "version", "help", "init", "patterns", "rewrite", "hook",
];

/// The argument's text, when it is valid UTF-8; `Err(NON_UTF8_ARG_ERR)`
/// otherwise.
///
/// Reserved subcommand names and `oo`'s own option values are matched only
/// as valid UTF-8 (see #213): `OsStr::to_str` is `None` for non-UTF-8 content
/// on every platform, so this check runs on the OS string before any lossy
/// conversion, identically on unix and non-unix.
pub fn utf8_or(arg: &OsString) -> Result<String, &'static str> {
    arg.to_str().map(str::to_string).ok_or(NON_UTF8_ARG_ERR)
}

/// Pre-validate the arguments of a reserved subcommand before dispatch (see
/// #213). Returns `Ok(())` when every argument `oo` itself consumes is valid
/// UTF-8, or `Err(NON_UTF8_ARG_ERR)` for the first violation.
///
/// The UTF-8 policy lives here, next to [`utf8_or`], so `main` only owns the
/// exit decision:
///
/// - a non-UTF-8 first word is simply the command to run: it can never match
///   a reserved name, so the arguments pass through as bytes (the non-UTF-8
///   case returns `Ok` here and is rejected by nobody);
/// - for a reserved first word, every argument is oo-owned and must be valid
///   UTF-8 — except `learn`, where only the `--hint` value is oo-owned and
///   the wrapped command's arguments pass through as raw bytes.
pub fn reject_non_utf8_reserved_args(args: &[OsString]) -> Result<(), &'static str> {
    // A non-UTF-8 first word is simply the command to run.
    let first = match args.first().and_then(|a| utf8_or(a).ok()) {
        Some(name) if RESERVED_SUBCOMMANDS.contains(&name.as_str()) => name,
        _ => return Ok(()),
    };

    if first == "learn" {
        // Reject only non-UTF-8 values following `--hint` (the hint is
        // consumed by `oo`); the wrapped command's arguments pass through.
        let mut i = 1;
        while i < args.len() {
            if args[i] == "--hint" && i + 1 < args.len() {
                utf8_or(&args[i + 1])?;
                i += 2;
            } else {
                i += 1;
            }
        }
        return Ok(());
    }

    // Every remaining argument is oo-owned and must be valid UTF-8.
    for arg in &args[1..] {
        utf8_or(arg)?;
    }
    Ok(())
}

/// Join OS-string arguments with spaces, converting each lossily, for the
/// command line recorded in labels, history, and the learn payload (see
/// #213).
pub fn lossy_join(args: &[OsString]) -> String {
    args.iter()
        .map(|a| a.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ")
}
