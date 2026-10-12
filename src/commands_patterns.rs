//! Pattern listing functionality for `oo patterns` commands.

use std::io::{self, Write};
use std::path::Path;

use crate::{init, learn, pattern};

/// Line printed under the `User (<dir>):` header when no user pattern is listable.
const EMPTY_USER_PATTERNS_LINE: &str = "  no learned patterns yet";

/// Write a single pattern entry with flags.
///
/// Helper function to reduce duplication in pattern listing code.
/// Writes the command_match with optional success/failure flags.
fn write_pattern_entry<W: Write>(
    out: &mut W,
    cmd_match: &str,
    has_success: bool,
    has_failure: bool,
) -> io::Result<()> {
    if has_success || has_failure {
        let mut flags = Vec::new();
        if has_success {
            flags.push("success");
        }
        if has_failure {
            flags.push("failure");
        }
        writeln!(out, "  {cmd_match}  [{}]", flags.join("] ["))
    } else {
        writeln!(out, "  {cmd_match}")
    }
}

/// Write pattern entries from a single directory, returning true if any were found.
///
/// Each line is written with a two-space indent so callers can add section headers.
/// A missing directory or one with no parseable `.toml` files writes nothing.
fn write_patterns_in<W: Write>(out: &mut W, dir: &Path) -> io::Result<bool> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(false),
    };

    let mut found = false;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let parsed = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| toml::from_str::<toml::Value>(&s).ok());

        let Some(parsed) = parsed else {
            continue;
        };
        found = true;

        let cmd_match = parsed
            .get("command_match")
            .and_then(|v| v.as_str())
            .unwrap_or("(unknown)");
        let has_success = parsed.get("success").is_some();
        let has_failure = parsed.get("failure").is_some();

        write_pattern_entry(out, cmd_match, has_success, has_failure)?;
    }
    Ok(found)
}

/// Print pattern entries from a single directory, returning true if any were found.
///
/// Each line is printed with a two-space indent so callers can add section headers.
/// A write failure is printed to stderr and returns false, so callers cannot tell it
/// apart from an empty directory.
pub fn list_patterns_in(dir: &Path) -> bool {
    match write_patterns_in(&mut io::stdout().lock(), dir) {
        Ok(found) => found,
        Err(e) => {
            eprintln!("oo: {e}");
            false
        }
    }
}

/// List learned pattern files from a single directory (legacy test helper).
///
/// Always returns 0. On a write error, `list_patterns_in` prints `oo: <error>` to
/// stderr and this function then prints the empty-state line to stdout.
pub fn cmd_patterns_in(dir: &Path) -> i32 {
    if !list_patterns_in(dir) {
        println!("no learned patterns yet");
    }
    0
}

/// Write the `User (<dir>):` section, with the empty-state line when nothing is listed.
fn write_user_section<W: Write>(out: &mut W, dir: &Path) -> io::Result<()> {
    writeln!(out, "User ({}):", dir.display())?;
    if !write_patterns_in(out, dir)? {
        writeln!(out, "{EMPTY_USER_PATTERNS_LINE}")?;
    }
    Ok(())
}

/// Write the built-in patterns.
fn write_builtins<W: Write>(out: &mut W) -> io::Result<()> {
    for pat in pattern::builtins() {
        let cmd_match = pat.command_match.as_str();
        let has_success = pat.success.is_some();
        let has_failure = pat.failure.is_some();

        write_pattern_entry(out, cmd_match, has_success, has_failure)?;
    }
    Ok(())
}

/// Write the full `oo patterns` listing.
fn write_all_patterns<W: Write>(out: &mut W) -> io::Result<()> {
    let project_dir = std::env::current_dir()
        .map(|cwd| init::project_patterns_dir(&cwd))
        .ok();
    let user_dir = learn::patterns_dir();

    // Show built-in patterns first
    writeln!(out, "Built-in ({} patterns):", pattern::builtins().len())?;
    write_builtins(out)?;
    writeln!(out)?;

    if let Some(ref pdir) = project_dir {
        if pdir.exists() {
            writeln!(out, "Project ({}):", pdir.display())?;
            write_patterns_in(out, pdir)?;
            writeln!(out)?;
        }
    }

    write_user_section(out, &user_dir)
}

/// List patterns from both project-local and user config directories.
pub fn cmd_patterns() -> i32 {
    match write_all_patterns(&mut io::stdout().lock()) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("oo: {e}");
            1
        }
    }
}

#[cfg(test)]
#[path = "patterns_empty_tests.rs"]
mod tests;
