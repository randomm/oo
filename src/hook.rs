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

use crate::commands::REWRITE_PATTERNS;
use crate::rewrite;
use crate::{error::Error, init::find_root};
use std::io::Read;
use std::io::Write;
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
/// `settings_path`, writing atomically (temp file + rename) and returning the
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

    // Ensure `hooks` is an object.
    let doc_obj = doc.as_object_mut().unwrap();
    if !doc_obj.get("hooks").and_then(|h| h.as_object()).is_some() {
        doc_obj.insert(
            "hooks".to_string(),
            serde_json::Value::Object(serde_json::Map::new()),
        );
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

    // Atomic write: temp file in the same directory, then rename.
    write_atomic(settings_path, &serialized)?;

    Ok(serialized)
}

/// Write `contents` to `path` atomically: write to a temp file in the same
/// directory, then rename over `path`. On any failure the original file (if
/// any) is left untouched.
fn write_atomic(path: &std::path::Path, contents: &str) -> Result<(), Error> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Init(format!("cannot create {}: {e}", parent.display())))?;
        }
    }

    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let parent_prefix = path
        .parent()
        .map(|p| p.as_os_str().to_os_string())
        .filter(|p| !p.as_os_str().is_empty());
    let tmp = match parent_prefix {
        Some(prefix) => PathBuf::from(prefix).join(format!(".{file_name}.tmp")),
        None => PathBuf::from(format!(".{file_name}.tmp")),
    };

    let result = (|| -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(contents.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    })();

    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(Error::Init(format!(
            "atomic write to {} failed: {e}",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::builtins;

    // set_var/remove_var are process-global and tests run on parallel threads
    // — every env-mutating test takes this lock first.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn env_guard() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn input(json: &str) -> Result<String, std::io::Error> {
        Ok(json.to_string())
    }

    // -------------------------------------------------------------------
    // handle — the fail-open processor
    // -------------------------------------------------------------------

    #[test]
    fn handle_rewrites_bash_command() {
        let json = format!(
            r#"{{"tool_name":"Bash","tool_input":{{"command":"{cmd}"}}}}"#,
            cmd = "pytest tests/"
        );
        let out = handle(input(&json)).expect("a pytest command must rewrite");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["hookSpecificOutput"]["hookEventName"], "PreToolUse");
        assert_eq!(
            v["hookSpecificOutput"]["updatedInput"]["command"],
            "oo pytest tests/"
        );
    }

    #[test]
    fn handle_no_rewrite_returns_none() {
        // `cat file.txt` has no builtin pattern → no rewrite → None (pass-through).
        let json = r#"{"tool_name":"Bash","tool_input":{"command":"cat file.txt"}}"#;
        assert!(handle(input(json)).is_none());
    }

    #[test]
    fn handle_non_bash_tool_returns_none() {
        let json = r#"{"tool_name":"Read","tool_input":{"command":"pytest tests/"}}"#;
        assert!(handle(input(json)).is_none());
    }

    #[test]
    fn handle_invalid_json_returns_none() {
        let invalid_json = "{\"tool_name\":\"Bash\",\""; // truncated / malformed
        assert!(handle(input(invalid_json)).is_none());
        assert!(handle(input("")).is_none());
        assert!(handle(input("not json at all")).is_none());
    }

    #[test]
    fn handle_missing_command_field_returns_none() {
        let json = r#"{"tool_name":"Bash","tool_input":{"description":"no cmd"}}"#;
        assert!(handle(input(json)).is_none());
    }

    #[test]
    fn handle_empty_command_returns_none() {
        let json = r#"{"tool_name":"Bash","tool_input":{"command":"   "}}"#;
        assert!(handle(input(json)).is_none());
    }

    #[test]
    fn handle_preserves_other_tool_input_fields() {
        let json = r#"{"tool_name":"Bash","tool_input":{"command":"pytest tests/","description":"view history","timeout":12000}}"#;
        let out = handle(input(json)).expect("must rewrite");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let updated = &v["hookSpecificOutput"]["updatedInput"];
        assert_eq!(updated["command"], "oo pytest tests/");
        assert_eq!(updated["description"], "view history");
        assert_eq!(updated["timeout"], 12000);
    }

    #[test]
    fn handle_output_is_valid_json_with_escaped_command() {
        // A plain cargo build rewrites to the oo-prefixed form; the output
        // must be valid, parseable JSON.
        let json = r#"{"tool_name":"Bash","tool_input":{"command":"cargo build"}}"#;
        let out = handle(input(json)).expect("must rewrite");
        // Must be valid, parseable JSON.
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            v["hookSpecificOutput"]["updatedInput"]["command"],
            "oo cargo build"
        );
    }

    #[test]
    fn handle_unreadable_stdin_returns_none() {
        let err: Result<String, std::io::Error> =
            Err(std::io::Error::new(std::io::ErrorKind::Other, "boom"));
        assert!(handle(err).is_none());
    }

    #[test]
    fn builtin_patterns_nonempty() {
        // Guard: the in-process rewrite set is non-empty so the rewrite path is real.
        assert!(!builtins().is_empty());
    }

    // -------------------------------------------------------------------
    // claude_settings_path
    // -------------------------------------------------------------------

    #[test]
    fn settings_path_project_under_git_root() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        let sub = dir.path().join("a").join("b");
        std::fs::create_dir_all(&sub).unwrap();
        assert_eq!(
            claude_settings_path(&sub, false),
            dir.path().join(".claude").join("settings.json")
        );
    }

    #[test]
    fn settings_path_global_uses_claude_dir_override() {
        let override_dir = tempfile::TempDir::new().unwrap();
        let _guard = env_guard();
        unsafe {
            std::env::set_var("OO_CLAUDE_DIR", override_dir.path());
        }
        assert_eq!(
            claude_settings_path(std::path::Path::new("/tmp"), true),
            override_dir.path().join("settings.json")
        );
        unsafe { std::env::remove_var("OO_CLAUDE_DIR") };
    }

    // -------------------------------------------------------------------
    // merge_oo_hook — installer
    // -------------------------------------------------------------------

    fn count_oo_entries(v: &serde_json::Value) -> usize {
        v["hooks"]["PreToolUse"]
            .as_array()
            .map(|groups| {
                groups
                    .iter()
                    .filter_map(|g| g.get("hooks").and_then(|h| h.as_array()))
                    .map(|cmds| {
                        cmds.iter()
                            .filter(|c| {
                                c.get("command").and_then(|v| v.as_str()) == Some(HOOK_COMMAND)
                            })
                            .count()
                    })
                    .sum::<usize>()
            })
            .unwrap_or(0)
    }

    #[test]
    fn merge_creates_file_when_absent() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        let out = merge_oo_hook(&path).expect("install must succeed");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v["hooks"]["PreToolUse"].as_array().is_some());
        assert_eq!(count_oo_entries(&v), 1);
        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert_eq!(on_disk, out);
    }

    #[test]
    fn merge_preserves_other_keys_and_hooks() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        let existing = r#"{
            "model": "claude-3",
            "hooks": {
              "PreToolUse": [
                {"matcher": "Bash", "hooks": [{"type":"command","command":"rtk hook claude"}]}
              ],
              "PostToolUse": [
                {"matcher": "*", "hooks": [{"type":"command","command":"other"}]}
              ]
            }
          }"#;
        std::fs::write(&path, existing).unwrap();
        let out = merge_oo_hook(&path).expect("install must succeed");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        // Other top-level keys preserved.
        assert_eq!(v["model"], "claude-3");
        // PostToolUse untouched.
        assert_eq!(v["hooks"]["PostToolUse"][0]["matcher"], "*");
        // Existing rtk Bash hook preserved (not deduped).
        let groups = v["hooks"]["PreToolUse"].as_array().unwrap();
        let all_commands: Vec<&str> = groups
            .iter()
            .filter_map(|g| g["hooks"].as_array())
            .flat_map(|cmds| cmds.iter().filter_map(|c| c["command"].as_str()))
            .collect();
        assert!(all_commands.contains(&"rtk hook claude"));
        assert_eq!(count_oo_entries(&v), 1);
    }

    #[test]
    fn merge_appends_to_existing_bash_group() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        let existing = r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"rtk hook claude"}]}]}}"#;
        std::fs::write(&path, existing).unwrap();
        let out = merge_oo_hook(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        // A single Bash group carrying both hooks (no duplicate group).
        let groups = v["hooks"]["PreToolUse"].as_array().unwrap();
        let bash_groups: Vec<_> = groups.iter().filter(|g| g["matcher"] == "Bash").collect();
        assert_eq!(bash_groups.len(), 1);
        assert_eq!(count_oo_entries(&v), 1);
    }

    #[test]
    fn merge_is_idempotent() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        merge_oo_hook(&path).unwrap();
        let first = std::fs::read_to_string(&path).unwrap();
        let out2 = merge_oo_hook(&path).expect("second run must succeed");
        let second = std::fs::read_to_string(&path).unwrap();
        assert_eq!(first, second, "idempotent re-run must not change the file");
        let v: serde_json::Value = serde_json::from_str(&out2).unwrap();
        assert_eq!(count_oo_entries(&v), 1, "exactly one oo entry after re-run");
    }

    #[test]
    fn merge_malformed_existing_is_error_and_not_overwritten() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        let corrupt = "{\"hooks\": {"; // invalid JSON
        std::fs::write(&path, corrupt).unwrap();
        let err = merge_oo_hook(&path).expect_err("malformed JSON must error");
        assert!(format!("{err:?}").contains("not valid JSON"));
        // File must be unchanged.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), corrupt);
    }

    #[test]
    fn write_atomic_leaves_original_on_failure() {
        // A read-only parent directory makes create_dir_all/File::create fail
        // on the temp file; the original must survive.
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("sub").join("settings.json");
        std::fs::create_dir_all(&path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"original":true} "#).unwrap();
        // Make the parent read-only so the temp file cannot be created.
        use std::os::unix::fs::PermissionsExt;
        let ro = std::fs::Permissions::from_mode(0o555);
        std::fs::set_permissions(path.parent().unwrap(), ro).unwrap();
        let err = write_atomic(&path, r#"{"new":true}"#);
        // Restore permissions for cleanup.
        let rw = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(path.parent().unwrap(), rw).unwrap();
        assert!(err.is_err(), "write must fail in a read-only dir");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            r#"{"original":true} "#,
            "original must be untouched after a failed atomic write"
        );
    }
}
