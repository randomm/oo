//! Pattern listing functionality for `oo patterns` commands.

use std::path::Path;

use crate::{init, learn, pattern};

/// Print a single pattern entry with flags.
///
/// Helper function to reduce duplication in pattern listing code.
/// Prints the command_match with optional success/failure flags.
fn print_pattern_entry(cmd_match: &str, has_success: bool, has_failure: bool) {
    if has_success || has_failure {
        let mut flags = Vec::new();
        if has_success {
            flags.push("success");
        }
        if has_failure {
            flags.push("failure");
        }
        println!("  {cmd_match}  [{}]", flags.join("] ["));
    } else {
        println!("  {cmd_match}");
    }
}

/// Print pattern entries from a single directory, returning true if any were found.
///
/// Each line is printed with a two-space indent so callers can add section headers.
pub fn list_patterns_in(dir: &Path) -> bool {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return false,
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

        let cmd_match = parsed
            .as_ref()
            .and_then(|v| v.get("command_match")?.as_str().map(str::to_string));
        let has_success = parsed.as_ref().and_then(|v| v.get("success")).is_some();
        let has_failure = parsed.as_ref().and_then(|v| v.get("failure")).is_some();

        if parsed.is_none() {
            continue;
        }
        found = true;
        let cmd_match = cmd_match.unwrap_or_else(|| "(unknown)".into());

        print_pattern_entry(&cmd_match, has_success, has_failure);
    }
    found
}

/// List learned pattern files from a single directory (legacy test helper).
pub fn cmd_patterns_in(dir: &Path) -> i32 {
    if !list_patterns_in(dir) {
        println!("no learned patterns yet");
    }
    0
}

/// List built-in patterns.
fn list_builtins_in() -> bool {
    let patterns = pattern::builtins();
    if patterns.is_empty() {
        return false;
    }

    for pat in patterns {
        let cmd_match = pat.command_match.as_str();
        let has_success = pat.success.is_some();
        let has_failure = pat.failure.is_some();

        print_pattern_entry(cmd_match, has_success, has_failure);
    }
    true
}

/// List patterns from both project-local and user config directories.
pub fn cmd_patterns() -> i32 {
    let project_dir = std::env::current_dir()
        .map(|cwd| init::project_patterns_dir(&cwd))
        .ok();
    let user_dir = learn::patterns_dir();

    // Show built-in patterns first
    println!("Built-in ({} patterns):", pattern::builtins().len());
    list_builtins_in();
    println!();

    if let Some(ref pdir) = project_dir {
        if pdir.exists() {
            println!("Project ({}):", pdir.display());
            list_patterns_in(pdir);
            println!();
        }
    }

    println!("User ({}):", user_dir.display());
    list_patterns_in(&user_dir);

    0
}
