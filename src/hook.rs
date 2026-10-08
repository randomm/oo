//! `oo hook claude` — stdin-JSON PreToolUse processor for Claude Code (issue #172).
//!
//! Claude Code pipes a PreToolUse event as JSON on stdin. For a `Bash` tool
//! call whose command has an `oo` rewrite, the hook prints a
//! `hookSpecificOutput` object on stdout (rewriting the command in place via
//! `updatedInput`); for everything else it prints nothing and exits 0.
//!
//! **Fail-open invariant:** the hook must never block or fail closed. Any
//! parse error, empty/invalid input, missing field, or unexpected value
//! means *no output and exit 0* — the command passes through unchanged. The
//! hook never executes the command, never touches the store, and does no
//! work beyond what `oo rewrite` already does (in-process pattern load).

use crate::rewrite::{self, REWRITE_PATTERNS};
use crate::{error::Error, init::find_root};
use std::io::Read;
use std::path::PathBuf;

/// The installed hook command (the exact `command` string in settings.json).
pub const HOOK_COMMAND: &str = "oo hook claude";

/// Resolve the Claude Code `settings.json` path for the given scope.
///
/// - `global`: `$OO_CLAUDE_DIR/settings.json` when `OO_CLAUDE_DIR` is set
///   (trusted path, used as-is — no suffix appended); a set-but-empty value,
///   or an unset/empty `HOME`, is an error (exit 1). Never consults the git
///   root.
/// - project: `<git-root>/.claude/settings.json` (`find_root` walks up to
///   `.git` and falls back to cwd outside a repo).
pub fn claude_settings_path(cwd: &std::path::Path, global: bool) -> PathBuf {
    if global {
        if let Some(dir) = std::env::var_os("OO_CLAUDE_DIR") {
            if dir.is_empty() {
                eprintln!(
                    "oo: OO_CLAUDE_DIR is set but empty; expected the Claude Code config directory"
                );
                std::process::exit(1);
            }
            return PathBuf::from(dir).join("settings.json");
        }
        match std::env::var_os("HOME") {
            Some(home) if !home.as_os_str().is_empty() => {
                PathBuf::from(home).join(".claude").join("settings.json")
            }
            _ => {
                eprintln!(
                    "oo: cannot determine the global Claude Code config directory: $OO_CLAUDE_DIR is not set and $HOME is unset or empty"
                );
                std::process::exit(1)
            }
        }
    } else {
        find_root(cwd).join(".claude").join("settings.json")
    }
}

/// Install a Claude Code PreToolUse hook into the given settings.json.
///
/// Merges the `oo hook claude` entry into `hooks.PreToolUse` without
/// clobbering other keys or hooks, creates the file when absent, and is
/// idempotent (exactly one oo entry). A malformed existing file is NOT
/// overwritten — the error is reported and the file left intact.
pub fn install_settings_json(settings_path: &std::path::Path) -> Result<(), Error> {
    merge_oo_hook(settings_path).map(|_| ())
}

/// Entry point for `oo hook claude`: read the PreToolUse JSON from stdin and
/// print the `hookSpecificOutput` JSON when a Bash command rewrites, else
/// print nothing. Always returns 0 (fail-open); see [`handle`].
pub fn cmd_hook_claude() -> i32 {
    let mut buf = String::new();
    let _ = std::io::stdin().read_to_string(&mut buf);
    if let Some(out) = handle(Ok(buf)) {
        println!("{out}");
    }
    0
}

/// Core processor: take the raw stdin (as a `Result` of the string) and
/// return the `hookSpecificOutput` JSON to print on stdout, or `None` to
/// print nothing (pass-through). Isolated from stdin so tests call it
/// directly with fixed input.
fn handle(input: Result<String, std::io::Error>) -> Option<String> {
    // OO_DISABLE=1 is a hard pass-through, checked before any rewrite work.
    if std::env::var("OO_DISABLE").is_ok_and(|v| v == "1") {
        return None;
    }

    let Ok(text) = input else {
        return None; // unreadable stdin -> pass through
    };

    let Ok(root) = serde_json::from_str::<serde_json::Value>(text.trim()) else {
        return None; // invalid/empty JSON -> pass through
    };

    // Only Bash tool calls are rewritten; anything else passes through.
    if root.get("tool_name").and_then(|v| v.as_str()) != Some("Bash") {
        return None;
    }

    // Chain the field extractions: any missing / wrong-typed field short-circuits
    // to `None` (pass-through). `and_then` here keeps the borrow checker happy
    // while preserving the "first failure wins" ordering.
    let tool_input = root.get("tool_input")?.as_object()?;
    let command = tool_input.get("command")?.as_str()?;
    if command.trim().is_empty() {
        return None;
    }

    // In-process rewrite — the same function and patterns as `oo rewrite`.
    let rewritten = rewrite::rewrite(command, &REWRITE_PATTERNS)?;

    // Copy all tool_input fields into updatedInput, replacing only `command`,
    // so description/timeout/etc. are preserved (serde_json handles escaping).
    let mut updated = serde_json::Map::from_iter(tool_input.clone());
    updated.insert("command".to_string(), serde_json::Value::String(rewritten));

    let output = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecisionReason": "oo auto-rewrite",
            "updatedInput": serde_json::Value::Object(updated),
        }
    });

    Some(output.to_string())
}

/// Merge the `oo hook claude` entry into `hooks.PreToolUse` in
/// `settings_path`, writing the result back to the same path (plain write —
/// the caller never relies on the write being crash-atomic) and returning the
/// updated JSON string. See [`install_settings_json`].
///
/// Rules:
/// - Absent file → create with the single oo entry.
/// - Existing file → parse; if any group already carries the exact oo command
///   it is a no-op (idempotent). Otherwise the oo command is appended to an
///   existing `matcher: "Bash"` group, or a new group is pushed. Every other
///   key and hook is preserved.
/// - Malformed existing JSON → error (file NOT overwritten).
pub fn merge_oo_hook(settings_path: &std::path::Path) -> Result<String, Error> {
    let existing = match std::fs::read_to_string(settings_path) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            return Err(Error::Init(format!(
                "cannot read {}: {e}",
                settings_path.display()
            )));
        }
    };

    let mut doc = match &existing {
        None => serde_json::Value::Object(serde_json::Map::new()),
        Some(s) => serde_json::from_str::<serde_json::Value>(s).map_err(|e| {
            Error::Init(format!(
                "existing {} is not valid JSON — refusing to overwrite it: {e}",
                settings_path.display()
            ))
        })?,
    };

    if !doc.is_object() {
        return Err(Error::Init(format!(
            "existing {} is not a JSON object — refusing to overwrite it",
            settings_path.display()
        )));
    }

    // `hooks` must be absent or an object — a garbage-typed `hooks` key (e.g.
    // a string) is refused rather than silently clobbered, mirroring the
    // `PreToolUse` non-array refusal below.
    let doc_obj = doc.as_object_mut().unwrap();
    match doc_obj.get("hooks") {
        None => {
            doc_obj.insert(
                "hooks".to_string(),
                serde_json::Value::Object(serde_json::Map::new()),
            );
        }
        Some(h) if h.is_object() => {}
        Some(_) => {
            return Err(Error::Init(format!(
                "existing {} has a non-object hooks key — refusing to modify it",
                settings_path.display()
            )));
        }
    }
    let hooks_obj = doc_obj
        .get_mut("hooks")
        .and_then(|h| h.as_object_mut())
        .expect("hooks is an object");

    let command_entry = serde_json::json!({"type": "command", "command": HOOK_COMMAND});
    let new_group = serde_json::json!({"matcher": "Bash", "hooks": [command_entry.clone()]});

    // Find the PreToolUse array, creating it if absent.
    let pretooluse = hooks_obj
        .entry("PreToolUse".to_string())
        .or_insert_with(|| serde_json::Value::Array(vec![]));
    let Some(pretooluse) = pretooluse.as_array_mut() else {
        return Err(Error::Init(format!(
            "existing {} has a non-array hooks.PreToolUse — refusing to modify it",
            settings_path.display()
        )));
    };

    // Idempotency: any group already carrying the exact oo command is a no-op.
    let already_present = pretooluse.iter().any(|group| {
        group
            .get("hooks")
            .and_then(|h| h.as_array())
            .is_some_and(|cmds| {
                cmds.iter()
                    .any(|c| c.get("command").and_then(|v| v.as_str()) == Some(HOOK_COMMAND))
            })
    });

    if !already_present {
        // Reuse an existing `matcher: "Bash"` group whose `hooks` array we can
        // extend; otherwise push a fresh group.
        let reuse = pretooluse
            .iter_mut()
            .find(|g| g.get("matcher").and_then(|m| m.as_str()) == Some("Bash"))
            .and_then(|g| g.get_mut("hooks").and_then(|h| h.as_array_mut()));
        match reuse {
            Some(cmds) => cmds.push(command_entry),
            None => pretooluse.push(new_group),
        }
    }

    let serialized = serde_json::to_string_pretty(&doc)
        .map_err(|e| Error::Init(format!("JSON serialize failed: {e}")))?;

    // Plain write, matching the pi installer (`init_pi`); a malformed
    // existing file was already refused above, and the caller never relies
    // on the write being crash-atomic. `create_dir_all` first: plain
    // `fs::write` cannot create the parent, and both sibling installers do.
    if let Some(parent) = settings_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::Init(format!("cannot create {}: {e}", parent.display())))?;
    }
    std::fs::write(settings_path, &serialized)
        .map_err(|e| Error::Init(format!("cannot write {}: {e}", settings_path.display())))?;

    Ok(serialized)
}

#[cfg(test)]
#[path = "hook_tests.rs"]
mod tests;
