//! Tests for `src/hook.rs` (sibling module via `#[path]`, keeping the
//! production file under the 500-line cap).
mod tests {
    use crate::hook::*;
    use crate::pattern::builtins;

    // set_var/remove_var are process-global and tests run on parallel threads
    // — every env-mutating test takes this lock first.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn env_guard() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
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
        let out = handle(&json).expect("a pytest command must rewrite");
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
        assert!(handle(json).is_none());
    }

    #[test]
    fn handle_non_bash_tool_returns_none() {
        let json = r#"{"tool_name":"Read","tool_input":{"command":"pytest tests/"}}"#;
        assert!(handle(json).is_none());
    }

    #[test]
    fn handle_invalid_json_returns_none() {
        let invalid_json = "{\"tool_name\":\"Bash\",\""; // truncated / malformed
        assert!(handle(invalid_json).is_none());
        assert!(handle("").is_none());
        assert!(handle("not json at all").is_none());
    }

    #[test]
    fn handle_invalid_utf8_passes_through() {
        // The production path decodes lossily: an invalid byte (e.g. 0xFF)
        // becomes a replacement character and the resulting text is not
        // valid JSON, so `handle` must return `None` (pass-through) rather
        // than panic or emit output.
        let lossy = String::from_utf8_lossy(&[b'{', 0xFF, b'}']).to_string();
        assert!(handle(&lossy).is_none());
    }

    #[test]
    fn handle_missing_command_field_returns_none() {
        let json = r#"{"tool_name":"Bash","tool_input":{"description":"no cmd"}}"#;
        assert!(handle(json).is_none());
    }

    #[test]
    fn handle_empty_command_returns_none() {
        let json = r#"{"tool_name":"Bash","tool_input":{"command":"   "}}"#;
        assert!(handle(json).is_none());
    }

    #[test]
    fn handle_preserves_other_tool_input_fields() {
        let json = r#"{"tool_name":"Bash","tool_input":{"command":"pytest tests/","description":"view history","timeout":12000}}"#;
        let out = handle(json).expect("must rewrite");
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
        let out = handle(json).expect("must rewrite");
        // Must be valid, parseable JSON.
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            v["hookSpecificOutput"]["updatedInput"]["command"],
            "oo cargo build"
        );
    }

    #[test]
    fn handle_json_escapes_quotes_backslashes_and_newlines_in_command() {
        // The spec's edge-case list calls this a coverage gap: a command
        // containing embedded quotes, backslashes, or newlines must round-trip
        // through serde_json so the hookSpecificOutput is valid, parseable JSON
        // with the exact original command recovered from updatedInput.command.
        //
        // We use serde_json to build the input so the command string is
        // correctly JSON-escaped. The command contains a double-quote and a
        // backslash — both must be escaped in the JSON output.
        let cmd = "cargo build -- \"a\\nb\"";
        let input_obj = serde_json::json!({
            "tool_name": "Bash",
            "tool_input": { "command": cmd }
        });
        let json = input_obj.to_string();
        let v_in: serde_json::Value =
            serde_json::from_str(&json).expect("input JSON must be valid");
        assert_eq!(
            v_in["tool_input"]["command"].as_str().unwrap(),
            cmd,
            "input command must round-trip"
        );
        let out = handle(&json).expect("must rewrite");
        // Must be valid, parseable JSON.
        let v: serde_json::Value =
            serde_json::from_str(&out).expect("hook output must be valid JSON");
        // The rewritten command must be `oo ` prepended, with the original
        // quotes/backslashes preserved byte-for-byte.
        let expected = format!("oo {cmd}");
        assert_eq!(
            v["hookSpecificOutput"]["updatedInput"]["command"], expected,
            "updatedInput.command must carry the exact original command with oo prefix"
        );
        // The raw JSON string must contain escaped sequences for the special
        // characters (serde_json escapes " -> \", \\ -> \\\\).
        assert!(
            out.contains("\\\""),
            "raw JSON output must escape embedded double-quotes, got: {out}"
        );
        assert!(
            out.contains("\\\\"),
            "raw JSON output must escape embedded backslashes, got: {out}"
        );
    }

    // -------------------------------------------------------------------
    // read_hook_input_bounded — capped stdin reader
    // -------------------------------------------------------------------

    /// A reader that immediately fails with a simulated I/O error.
    struct FailingReader;

    impl std::io::Read for FailingReader {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "simulated stdin failure",
            ))
        }
    }

    #[test]
    fn bounded_read_oversize_input_returns_none() {
        // A payload 1 MiB + 1 byte is larger than MAX_HOOK_INPUT_BYTES, so
        // the reader must reject it (fail open) rather than buffer the
        // whole thing.
        let oversize = vec![b'a'; 1024 * 1024 + 1];
        assert!(read_hook_input_bounded(oversize.as_slice()).is_none());
    }

    #[test]
    fn bounded_read_within_cap_returns_string() {
        let input = b"{\"tool_name\":\"Bash\"}".to_vec();
        let text =
            read_hook_input_bounded(input.as_slice()).expect("an input within the cap must read");
        assert_eq!(text, "{\"tool_name\":\"Bash\"}");
    }

    #[test]
    fn bounded_read_exactly_at_cap_returns_string() {
        // Exactly MAX_HOOK_INPUT_BYTES is allowed (the +1 byte distinguishes
        // "at the cap" from "over the cap").
        let at_cap = vec![b'x'; 1024 * 1024];
        let text =
            read_hook_input_bounded(at_cap.as_slice()).expect("input at exactly the cap must read");
        assert_eq!(text.len(), 1024 * 1024);
    }

    #[test]
    fn bounded_read_error_returns_none() {
        assert!(read_hook_input_bounded(FailingReader).is_none());
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
            claude_settings_path(&sub, false).expect("project path must be Ok"),
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
            claude_settings_path(std::path::Path::new("/tmp"), true)
                .expect("override path must be Ok"),
            override_dir.path().join("settings.json")
        );
        unsafe { std::env::remove_var("OO_CLAUDE_DIR") };
    }

    #[test]
    fn settings_path_global_empty_override_is_error() {
        let _guard = env_guard();
        unsafe {
            std::env::set_var("OO_CLAUDE_DIR", "");
        }
        let err = claude_settings_path(std::path::Path::new("/tmp"), true)
            .expect_err("empty OO_CLAUDE_DIR must error");
        assert!(
            format!("{err:?}").contains("OO_CLAUDE_DIR is set but empty"),
            "message: {err:?}"
        );
        unsafe { std::env::remove_var("OO_CLAUDE_DIR") };
    }

    #[test]
    fn settings_path_global_without_home_or_override_is_error() {
        let _guard = env_guard();
        unsafe {
            std::env::remove_var("OO_CLAUDE_DIR");
            std::env::remove_var("HOME");
        }
        let err = claude_settings_path(std::path::Path::new("/tmp"), true)
            .expect_err("missing HOME must error");
        assert!(
            format!("{err:?}").contains("$HOME is unset or empty"),
            "message: {err:?}"
        );
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
        merge_oo_hook(&path).expect("install must succeed");
        let on_disk = std::fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&on_disk).unwrap();
        assert!(v["hooks"]["PreToolUse"].as_array().is_some());
        assert_eq!(count_oo_entries(&v), 1);
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
        merge_oo_hook(&path).expect("install must succeed");
        let on_disk = std::fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&on_disk).unwrap();
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
        merge_oo_hook(&path).unwrap();
        let on_disk = std::fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&on_disk).unwrap();
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
        merge_oo_hook(&path).expect("second run must succeed");
        let second = std::fs::read_to_string(&path).unwrap();
        assert_eq!(first, second, "idempotent re-run must not change the file");
        let v: serde_json::Value = serde_json::from_str(&second).unwrap();
        assert_eq!(count_oo_entries(&v), 1, "exactly one oo entry after re-run");
    }

    #[test]
    fn merge_non_object_hooks_key_is_error_and_not_overwritten() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        let existing = r#"{"model": "claude-3", "hooks": "oops"}"#;
        std::fs::write(&path, existing).unwrap();
        let err = merge_oo_hook(&path).expect_err("non-object hooks must error");
        assert!(
            format!("{err:?}").contains("non-object hooks"),
            "message: {err:?}"
        );
        // File must be unchanged.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), existing);
    }

    /// Write `existing`, expect `merge_oo_hook` to error, and assert the
    /// file is byte-identical afterwards.
    fn merge_rejects_malformed_bash_group(existing: &str) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, existing).unwrap();
        let err = merge_oo_hook(&path).expect_err("malformed Bash group must error");
        assert!(
            format!("{err:?}").contains("malformed") && format!("{err:?}").contains("Bash"),
            "message: {err:?}"
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            existing,
            "file must be left untouched"
        );
    }

    #[test]
    fn merge_bash_group_hooks_with_string_entries_is_error() {
        merge_rejects_malformed_bash_group(
            r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":["some-string"]}]}}"#,
        );
    }

    #[test]
    fn merge_bash_group_hooks_non_array_is_error() {
        merge_rejects_malformed_bash_group(
            r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":"oops"}]}}"#,
        );
    }

    #[test]
    fn merge_bash_group_missing_hooks_is_error() {
        merge_rejects_malformed_bash_group(r#"{"hooks":{"PreToolUse":[{"matcher":"Bash"}]}}"#);
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
}
