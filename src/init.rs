use crate::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

/// The AGENTS.md snippet printed to stdout on `oo init`.
///
/// Kept as a constant so unit tests can assert against the exact string.
pub const AGENTS_SNIPPET: &str = "\
## Shell commands
Prefix all shell commands with `oo`. Use `oo recall \"<query>\"` to search large outputs.\
";

/// Minimal Claude Code PreToolUse hooks configuration.
///
/// Intercepts `--help`/`-h` Bash calls so agents use `oo help <cmd>` instead.
///
/// Schema: `hooks` is an object keyed by event name; each event maps to an array
/// of hook configs with `matcher` (string tool name) and `hooks` (array of commands).
/// `exit 2` in a Claude Code hook blocks the tool call and shows the message to Claude.
/// Content filtering (`--help`/`-h`) is done inside the command script, not via a schema
/// field, because Claude Code has no `command_pattern` key in this schema version.
///
/// Claude Code sends hook input as JSON on stdin (not via env vars). The script reads
/// stdin with `cat`, extracts `.tool_input.command` with `jq`, checks for help flags,
/// then either blocks (exit 2, message to stderr) or passes through (echo input, exit 0).
pub const HOOKS_JSON: &str = r#"{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "input=$(cat); cmd=$(echo \"$input\" | jq -r '.tool_input.command // \"\"' 2>/dev/null); if echo \"$cmd\" | grep -qE '\\-\\-help| -h$| -h '; then echo 'Use: oo help <cmd> for a token-efficient command reference' >&2; exit 2; fi; echo \"$input\""
          }
        ]
      }
    ]
  }
}
"#;

/// Resolve the directory in which to create `.claude/`.
///
/// Walks upward from `cwd` looking for a `.git` directory — this is the git
/// root and the natural home for agent configuration.  Falls back to `cwd`
/// when no git repo is found, so the command works outside repos too.
pub fn find_root(cwd: &Path) -> PathBuf {
    let mut dir = cwd.to_path_buf();
    loop {
        if dir.join(".git").exists() {
            return dir;
        }
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => return cwd.to_path_buf(),
        }
    }
}

/// Run `oo init`: create `.claude/hooks.json` and print the AGENTS.md snippet.
///
/// Idempotent — if `hooks.json` already exists it warns and skips the write.
/// Uses the current working directory as the starting point for git-root detection.
pub fn run() -> Result<(), Error> {
    let cwd = std::env::current_dir()
        .map_err(|e| Error::Init(format!("cannot determine working directory: {e}")))?;

    run_in(&cwd)
}

/// Inner implementation that accepts an explicit root — used by unit tests.
pub fn run_in(cwd: &Path) -> Result<(), Error> {
    let root = find_root(cwd);
    let claude_dir = root.join(".claude");
    let hooks_path = claude_dir.join("hooks.json");

    // create_dir_all is idempotent — no TOCTOU guard needed.
    fs::create_dir_all(&claude_dir)
        .map_err(|e| Error::Init(format!("cannot create {}: {e}", claude_dir.display())))?;

    if hooks_path.exists() {
        // Warn but do NOT overwrite — caller's config is authoritative.
        eprintln!(
            "oo init: {} already exists — skipping (delete it to regenerate)",
            hooks_path.display()
        );
    } else {
        fs::write(&hooks_path, HOOKS_JSON)
            .map_err(|e| Error::Init(format!("cannot write {}: {e}", hooks_path.display())))?;
        println!("Created {}", hooks_path.display());
    }

    println!();
    println!("Add this to your AGENTS.md:");
    println!();
    println!("{AGENTS_SNIPPET}");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // -----------------------------------------------------------------------
    // AGENTS_SNIPPET content
    // -----------------------------------------------------------------------

    #[test]
    fn snippet_contains_oo_prefix_instruction() {
        assert!(
            AGENTS_SNIPPET.contains("Prefix all shell commands with `oo`"),
            "snippet must instruct agents to prefix commands with oo"
        );
    }

    #[test]
    fn snippet_contains_recall_instruction() {
        assert!(
            AGENTS_SNIPPET.contains("oo recall"),
            "snippet must mention oo recall for large outputs"
        );
    }

    #[test]
    fn snippet_has_shell_commands_heading() {
        assert!(
            AGENTS_SNIPPET.starts_with("## Shell commands"),
            "snippet must start with ## Shell commands heading"
        );
    }

    // -----------------------------------------------------------------------
    // HOOKS_JSON validity
    // -----------------------------------------------------------------------

    #[test]
    fn hooks_json_is_valid_json() {
        let parsed: serde_json::Value =
            serde_json::from_str(HOOKS_JSON).expect("HOOKS_JSON must be valid JSON");
        assert!(
            parsed.get("hooks").is_some(),
            "hooks.json must have a top-level 'hooks' key"
        );
    }

    #[test]
    fn hooks_json_has_pretooluse_event() {
        // Schema: hooks is an object keyed by event name.
        let parsed: serde_json::Value = serde_json::from_str(HOOKS_JSON).unwrap();
        let pre_tool_use = parsed["hooks"].get("PreToolUse");
        assert!(
            pre_tool_use.is_some(),
            "hooks object must have a PreToolUse key"
        );
        assert!(
            pre_tool_use.unwrap().as_array().is_some(),
            "PreToolUse must be an array of hook configs"
        );
    }

    #[test]
    fn hooks_json_references_bash_tool() {
        // matcher is a string tool name (not an object) in the current Claude Code schema.
        let parsed: serde_json::Value = serde_json::from_str(HOOKS_JSON).unwrap();
        let configs = parsed["hooks"]["PreToolUse"].as_array().unwrap();
        let has_bash = configs
            .iter()
            .any(|c| c.get("matcher").and_then(|m| m.as_str()) == Some("Bash"));
        assert!(has_bash, "at least one PreToolUse config must target Bash");
    }

    #[test]
    fn hooks_json_hook_command_mentions_oo_help() {
        // Each config has a "hooks" array (plural) of command objects.
        let parsed: serde_json::Value = serde_json::from_str(HOOKS_JSON).unwrap();
        let configs = parsed["hooks"]["PreToolUse"].as_array().unwrap();
        let mentions_oo_help = configs.iter().any(|c| {
            c.get("hooks")
                .and_then(|hs| hs.as_array())
                .is_some_and(|hs| {
                    hs.iter().any(|h| {
                        h.get("command")
                            .and_then(|cmd| cmd.as_str())
                            .is_some_and(|s| s.contains("oo help"))
                    })
                })
        });
        assert!(
            mentions_oo_help,
            "a hook command must mention 'oo help' so agents know the alternative"
        );
    }

    #[test]
    fn hooks_json_command_reads_stdin_not_env_var() {
        // Claude Code sends hook input as JSON on stdin, not via $TOOL_INPUT env var.
        // This test verifies the command uses the correct contract:
        //   - `cat` to read stdin
        //   - `jq` to parse JSON
        //   - `.tool_input.command` to extract the right field
        //   - `echo "$input"` to pass through on the allow path (exit 0)
        let parsed: serde_json::Value = serde_json::from_str(HOOKS_JSON).unwrap();
        let configs = parsed["hooks"]["PreToolUse"].as_array().unwrap();
        let command_str = configs
            .iter()
            .find_map(|c| {
                c.get("hooks")
                    .and_then(|hs| hs.as_array())
                    .and_then(|hs| hs.first())
                    .and_then(|h| h.get("command"))
                    .and_then(|cmd| cmd.as_str())
            })
            .expect("must have at least one hook command");

        assert!(
            command_str.contains("cat"),
            "hook must read stdin with `cat`, not rely on env vars"
        );
        assert!(command_str.contains("jq"), "hook must parse JSON with `jq`");
        assert!(
            command_str.contains("tool_input.command"),
            "hook must extract `.tool_input.command` — the field Claude Code sends"
        );
        assert!(
            command_str.contains("echo \"$input\""),
            "hook must echo original stdin JSON on the allow path (exit 0)"
        );
        assert!(
            !command_str.contains("$TOOL_INPUT"),
            "hook must NOT use $TOOL_INPUT env var — Claude Code does not set it"
        );
    }

    // -----------------------------------------------------------------------
    // find_root
    // -----------------------------------------------------------------------

    #[test]
    fn find_root_returns_git_root() {
        let dir = TempDir::new().unwrap();
        let git_dir = dir.path().join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir_all(&sub).unwrap();

        // find_root from subdirectory should resolve to the git root.
        assert_eq!(find_root(&sub), dir.path());
    }

    #[test]
    fn find_root_falls_back_to_cwd_when_no_git() {
        let dir = TempDir::new().unwrap();
        // No .git → cwd is returned as-is.
        assert_eq!(find_root(dir.path()), dir.path());
    }

    // -----------------------------------------------------------------------
    // run_in — happy path
    // -----------------------------------------------------------------------

    #[test]
    fn run_in_creates_claude_dir_and_hooks_json() {
        let dir = TempDir::new().unwrap();
        run_in(dir.path()).expect("run_in must succeed in empty dir");

        let hooks_path = dir.path().join(".claude").join("hooks.json");
        assert!(hooks_path.exists(), ".claude/hooks.json must be created");
    }

    #[test]
    fn run_in_writes_valid_json_to_hooks_file() {
        let dir = TempDir::new().unwrap();
        run_in(dir.path()).unwrap();

        let content = fs::read_to_string(dir.path().join(".claude").join("hooks.json")).unwrap();
        let parsed: serde_json::Value =
            serde_json::from_str(&content).expect("written hooks.json must be valid JSON");
        assert!(parsed.get("hooks").is_some());
    }

    // -----------------------------------------------------------------------
    // run_in — idempotency
    // -----------------------------------------------------------------------

    #[test]
    fn run_in_does_not_overwrite_existing_hooks_json() {
        let dir = TempDir::new().unwrap();
        let claude_dir = dir.path().join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let hooks_path = claude_dir.join("hooks.json");

        // Pre-existing content written by a human.
        let custom = r#"{"hooks":[],"custom":true}"#;
        fs::write(&hooks_path, custom).unwrap();

        // run_in must leave the file untouched.
        run_in(dir.path()).unwrap();

        let after = fs::read_to_string(&hooks_path).unwrap();
        assert_eq!(
            after, custom,
            "pre-existing hooks.json must not be overwritten"
        );
    }

    #[test]
    fn run_in_is_idempotent_twice() {
        let dir = TempDir::new().unwrap();
        run_in(dir.path()).expect("first run must succeed");
        run_in(dir.path()).expect("second run must also succeed without error");

        // Content should be the canonical HOOKS_JSON from the first run.
        let content = fs::read_to_string(dir.path().join(".claude").join("hooks.json")).unwrap();
        assert_eq!(content, HOOKS_JSON);
    }
}
