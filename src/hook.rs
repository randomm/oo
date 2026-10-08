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

use crate::rewrite::{self, PATTERNS};
use crate::{error::Error, init::find_root};
use std::io::Read;
use std::path::PathBuf;

/// The installed hook command (the exact `command` string in settings.json).
pub const HOOK_COMMAND: &str = "oo hook claude";

/// Resolve the Claude Code `settings.json` path for the given scope.
///
/// - `global`: `$OO_CLAUDE_DIR/settings.json` when `OO_CLAUDE_DIR` is set
///   (trusted path, used as-is — no suffix appended). A set-but-empty value,
///   or an unset/empty `HOME`, is an error — the caller prints it and exits 1
///   (mirrors `init_pi::target_dir`, which returns `Err` for the identical
///   conditions). Never consults the git root.
/// - project: `<git-root>/.claude/settings.json` (`find_root` walks up to
///   `.git` and falls back to cwd outside a repo). Always `Ok`.
pub fn claude_settings_path(cwd: &std::path::Path, global: bool) -> Result<PathBuf, Error> {
    if global {
        if let Some(dir) = std::env::var_os("OO_CLAUDE_DIR") {
            if dir.is_empty() {
                return Err(Error::Init(
                    "OO_CLAUDE_DIR is set but empty; expected the Claude Code config directory"
                        .into(),
                ));
            }
            return Ok(PathBuf::from(dir).join("settings.json"));
        }
        match std::env::var_os("HOME") {
            Some(home) if !home.as_os_str().is_empty() => {
                Ok(PathBuf::from(home).join(".claude").join("settings.json"))
            }
            _ => Err(Error::Init(
                "cannot determine the global Claude Code config directory: \
                 $OO_CLAUDE_DIR is not set and $HOME is unset or empty"
                    .into(),
            )),
        }
    } else {
        Ok(find_root(cwd).join(".claude").join("settings.json"))
    }
}

/// Entry point for `oo hook claude`: read the PreToolUse JSON from stdin and
/// print the `hookSpecificOutput` JSON when a Bash command rewrites, else
/// print nothing. Always returns 0 (fail-open); see [`handle`].
///
/// Any **read error** — including a partial read — aborts the pass-through
/// before the rewrite path is reached, so a truncated buffer is never
/// treated as a complete event. A clean read is decoded lossily: invalid
/// UTF-8 becomes replacement characters and then fails JSON parsing, which
/// also passes through unchanged.
pub fn cmd_hook_claude() -> i32 {
    let mut buf = Vec::new();
    if std::io::stdin().read_to_end(&mut buf).is_err() {
        return 0; // unreadable/partial stdin -> fail open, no output
    }
    let text = String::from_utf8_lossy(&buf).to_string();
    if let Some(out) = handle(&text) {
        println!("{out}");
    }
    0
}

/// Core processor: take the lossy-decoded stdin text and return the
/// `hookSpecificOutput` JSON to print on stdout, or `None` to print nothing
/// (pass-through). Isolated from stdin so tests call it directly with fixed
/// input.
fn handle(text: &str) -> Option<String> {
    // OO_DISABLE=1 is a hard pass-through, checked before any rewrite work.
    if std::env::var("OO_DISABLE").is_ok_and(|v| v == "1") {
        return None;
    }

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
    let rewritten = rewrite::rewrite(command, &PATTERNS)?;

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
/// `settings_path`, writing the result back to the same path **atomically**
/// (temp file in the same directory, then `rename` over the target — a
/// mid-write failure leaves either the old file or the complete new file,
/// never a truncated target).
///
/// Rules:
/// - Absent file → create with the single oo entry.
/// - Existing file → parse; if any group already carries the exact oo command
///   it is a no-op (idempotent). Otherwise the oo command is appended to an
///   existing `matcher: "Bash"` group, or a new group is pushed. Every other
///   key and hook is preserved.
/// - Malformed existing JSON → error (file NOT overwritten).
pub fn merge_oo_hook(settings_path: &std::path::Path) -> Result<(), Error> {
    let mut doc = load_or_create(settings_path)?;
    let pretooluse = pretooluse_array(settings_path, &mut doc)?;

    let command_entry = serde_json::json!({"type": "command", "command": HOOK_COMMAND});
    let new_group = serde_json::json!({"matcher": "Bash", "hooks": [command_entry.clone()]});

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

    // Write atomically: temp file in the same directory, then rename over
    // the target. A mid-write failure (disk full, power loss) leaves either
    // the old file or the complete new file — never a truncated target.
    if let Some(parent) = settings_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::Init(format!("cannot create {}: {e}", parent.display())))?;
    }
    let tmp_path = settings_path.with_file_name(format!(
        "{}.tmp",
        settings_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("settings.json")
    ));
    std::fs::write(&tmp_path, &serialized)
        .map_err(|e| Error::Init(format!("cannot write {}: {e}", tmp_path.display())))?;
    std::fs::rename(&tmp_path, settings_path).map_err(|rename_err| {
        let mut msg = format!(
            "cannot rename {} to {}: {rename_err}",
            tmp_path.display(),
            settings_path.display()
        );
        if let Err(cleanup_err) = std::fs::remove_file(&tmp_path) {
            msg.push_str(&format!(
                "; cleanup of the temp file also failed: {cleanup_err}"
            ));
        }
        Error::Init(msg)
    })?;

    Ok(())
}

/// Read the existing settings file (or create an empty object when absent),
/// parsing and validating that any existing content is a JSON object.
fn load_or_create(settings_path: &std::path::Path) -> Result<serde_json::Value, Error> {
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

    let doc = match &existing {
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
    Ok(doc)
}

/// Validate and locate the mutable `hooks.PreToolUse` array, creating the
/// `hooks` object / `PreToolUse` array when absent. A garbage-typed `hooks`
/// key (e.g. a string) or a non-array `PreToolUse` is refused rather than
/// silently clobbered.
fn pretooluse_array<'a>(
    settings_path: &std::path::Path,
    doc: &'a mut serde_json::Value,
) -> Result<&'a mut Vec<serde_json::Value>, Error> {
    let doc = match doc {
        serde_json::Value::Object(obj) => obj,
        _ => {
            return Err(Error::Init(format!(
                "existing {} is not a JSON object — refusing to modify it",
                settings_path.display()
            )));
        }
    };

    // Locate or create the `hooks` object; the object borrow comes straight
    // out of the match so no intermediate `unwrap` is needed.
    let hooks = if doc.get("hooks").is_none() {
        // No `hooks` key yet: insert a fresh object first, then re-borrow.
        doc.insert(
            "hooks".to_string(),
            serde_json::Value::Object(serde_json::Map::new()),
        );
        match doc.get_mut("hooks") {
            Some(serde_json::Value::Object(obj)) => obj,
            _ => {
                return Err(Error::Init(format!(
                    "existing {} has a non-object hooks key — refusing to modify it",
                    settings_path.display()
                )))
            }
        }
    } else {
        match doc.get_mut("hooks") {
            Some(serde_json::Value::Object(obj)) => obj,
            Some(_) => {
                return Err(Error::Init(format!(
                    "existing {} has a non-object hooks key — refusing to modify it",
                    settings_path.display()
                )));
            }
            None => {
                // The `if` branch handles the absent-key case; this is
                // unreachable.
                return Err(Error::Init(format!(
                    "existing {} has no hooks key — refusing to modify it",
                    settings_path.display()
                )));
            }
        }
    };

    let pretooluse = match hooks
        .entry("PreToolUse".to_string())
        .or_insert_with(|| serde_json::Value::Array(Vec::new()))
    {
        serde_json::Value::Array(arr) => arr,
        _ => {
            return Err(Error::Init(format!(
                "existing {} has a non-array hooks.PreToolUse — refusing to modify it",
                settings_path.display()
            )));
        }
    };
    Ok(pretooluse)
}

#[cfg(test)]
#[path = "hook_tests.rs"]
mod tests;
