use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn oo() -> Command {
    Command::cargo_bin("oo").unwrap()
}

#[test]
fn test_echo_passthrough() {
    oo().args(["echo", "hello"])
        .assert()
        .success()
        .stdout("hello\n");
}

#[test]
fn test_multiword_echo() {
    oo().args(["echo", "hello", "world"])
        .assert()
        .success()
        .stdout("hello world\n");
}

#[test]
fn test_false_failure() {
    oo().args(["false"])
        .assert()
        .failure()
        .stdout(predicate::str::starts_with("\u{2717}")); // ✗
}

#[test]
fn test_exit_code_preserved() {
    oo().args(["sh", "-c", "exit 42"]).assert().code(42);
}

#[test]
fn test_version() {
    oo().arg("version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn test_no_args_shows_help() {
    oo().assert()
        .success()
        .stdout(predicate::str::contains("Usage"));
}

#[test]
fn test_large_output_gets_indicator() {
    // seq 1 10000 produces ~49KB which is > 4KB threshold
    oo().args(["seq", "1", "10000"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("\u{25CF}")); // ●
}

#[test]
fn test_stderr_included_in_failure() {
    oo().args(["sh", "-c", "echo failure_msg >&2; exit 1"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("failure_msg"));
}

#[test]
fn test_forget_runs() {
    oo().arg("forget")
        .assert()
        .success()
        .stdout(predicate::str::contains("Cleared session data"));
}

#[test]
fn test_help_no_args_shows_usage() {
    oo().arg("help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage:"));
}

#[test]
fn test_help_includes_help_cmd_in_usage() {
    // Verify the help command itself appears in the no-args usage output
    oo().assert()
        .success()
        .stdout(predicate::str::contains("oo help <cmd>"));
}

#[test]
fn test_help_empty_arg() {
    Command::cargo_bin("oo")
        .unwrap()
        .args(&["help", ""])
        .assert()
        .failure();
}

// ---------------------------------------------------------------------------
// oo init
// ---------------------------------------------------------------------------

#[test]
fn test_init_creates_hooks_json() {
    let dir = TempDir::new().unwrap();
    oo().arg("init").current_dir(dir.path()).assert().success();

    let hooks_path = dir.path().join(".claude").join("hooks.json");
    assert!(
        hooks_path.exists(),
        ".claude/hooks.json must exist after oo init"
    );
}

#[test]
fn test_init_hooks_json_is_valid_json() {
    let dir = TempDir::new().unwrap();
    oo().arg("init").current_dir(dir.path()).assert().success();

    let content = std::fs::read_to_string(dir.path().join(".claude").join("hooks.json")).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(&content).expect("hooks.json must be valid JSON");
    assert!(parsed.get("hooks").is_some());
}

#[test]
fn test_init_prints_agents_snippet() {
    let dir = TempDir::new().unwrap();
    oo().arg("init")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Prefix all shell commands with `oo`",
        ));
}

#[test]
fn test_init_second_run_does_not_overwrite() {
    let dir = TempDir::new().unwrap();

    // First run creates the file.
    oo().arg("init").current_dir(dir.path()).assert().success();

    // Overwrite with sentinel content.
    let hooks_path = dir.path().join(".claude").join("hooks.json");
    std::fs::write(&hooks_path, r#"{"hooks":[],"sentinel":true}"#).unwrap();

    // Second run must not clobber the file.
    oo().arg("init").current_dir(dir.path()).assert().success();

    let after = std::fs::read_to_string(&hooks_path).unwrap();
    assert!(
        after.contains("\"sentinel\":true"),
        "pre-existing hooks.json must not be overwritten on second oo init"
    );
}

#[test]
fn test_init_second_run_succeeds_without_error() {
    let dir = TempDir::new().unwrap();
    oo().arg("init").current_dir(dir.path()).assert().success();
    // Second invocation must exit 0 — idempotent.
    oo().arg("init").current_dir(dir.path()).assert().success();
}

// ---------------------------------------------------------------------------
// oo init --format generic
// ---------------------------------------------------------------------------

#[test]
fn test_init_format_generic_exits_success() {
    let dir = TempDir::new().unwrap();
    oo().args(["init", "--format", "generic"])
        .current_dir(dir.path())
        .assert()
        .success();
}

#[test]
fn test_init_format_generic_prints_agents_snippet() {
    let dir = TempDir::new().unwrap();
    oo().args(["init", "--format", "generic"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Prefix all shell commands with `oo`",
        ));
}

#[test]
fn test_init_format_generic_prints_setup_section() {
    let dir = TempDir::new().unwrap();
    oo().args(["init", "--format", "generic"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("## Setup"));
}

#[test]
fn test_init_format_generic_prints_shell_commands_instructions() {
    let dir = TempDir::new().unwrap();
    oo().args(["init", "--format", "generic"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("oo recall"))
        .stdout(predicate::str::contains("oo help"))
        .stdout(predicate::str::contains("oo learn"));
}

#[test]
fn test_init_format_generic_prints_alias_section() {
    let dir = TempDir::new().unwrap();
    oo().args(["init", "--format", "generic"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("alias o='oo'"));
}

#[test]
fn test_init_format_generic_does_not_create_hooks_json() {
    let dir = TempDir::new().unwrap();
    oo().args(["init", "--format", "generic"])
        .current_dir(dir.path())
        .assert()
        .success();
    // Generic format must NOT create any files.
    let hooks_path = dir.path().join(".claude").join("hooks.json");
    assert!(
        !hooks_path.exists(),
        "oo init --format generic must not create .claude/hooks.json"
    );
}

// ---------------------------------------------------------------------------
// oo init --format claude
// ---------------------------------------------------------------------------

#[test]
fn test_init_format_claude_creates_hooks_json() {
    let dir = TempDir::new().unwrap();
    oo().args(["init", "--format", "claude"])
        .current_dir(dir.path())
        .assert()
        .success();
    let hooks_path = dir.path().join(".claude").join("hooks.json");
    assert!(
        hooks_path.exists(),
        ".claude/hooks.json must exist after oo init --format claude"
    );
}

#[test]
fn test_init_format_claude_prints_agents_snippet() {
    let dir = TempDir::new().unwrap();
    oo().args(["init", "--format", "claude"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Prefix all shell commands with `oo`",
        ));
}

// ---------------------------------------------------------------------------
// recall command
// ---------------------------------------------------------------------------

#[test]
fn test_recall_no_args() {
    // `oo recall` with no query should fail with a helpful error
    oo().arg("recall")
        .assert()
        .failure()
        .stderr(predicate::str::contains("recall requires a query"));
}

#[test]
fn test_recall_no_results() {
    // A query that matches nothing should exit 0 and mention no results
    oo().args(["recall", "xyzzy_nonexistent_query_12345"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No results found"));
}

// ---------------------------------------------------------------------------
// learn command
// ---------------------------------------------------------------------------

#[test]
fn test_learn_no_args() {
    // `oo learn` with no command should fail with a helpful error
    oo().arg("learn")
        .assert()
        .failure()
        .stderr(predicate::str::contains("learn requires a command"));
}

#[test]
fn test_learn_no_api_key() {
    // `oo learn echo hello` spawns a background child that checks the API key.
    // The foreground process always exits 0 (it just runs the command and detaches).
    // What we can verify: stderr mentions the learning intent for the command.
    oo().args(["learn", "echo", "hello"])
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .assert()
        .success()
        .stderr(predicate::str::contains("[learning pattern for"));
}

// ---------------------------------------------------------------------------
// run command edge cases
// ---------------------------------------------------------------------------

#[test]
fn test_run_command_not_found() {
    // A command that doesn't exist should result in a failure indicator or error
    oo().args(["nonexistent_binary_xyz_abc_123"])
        .assert()
        .failure();
}

#[test]
fn test_run_multiword_command() {
    // Multiple args are passed through correctly
    oo().args(["echo", "hello", "world"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello world"));
}

#[test]
fn test_passthrough_git_version() {
    // `git --version` produces small output (< 4096 bytes) and exits 0.
    // oo must pass it through verbatim — no ✓ or ● prefix.
    // This tests the passthrough classification tier end-to-end.
    oo().args(["git", "--version"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("git version"));
}

#[test]
fn test_failure_stderr_output() {
    // `ls /nonexistent_path_xyz` should exit non-zero and show ✗ with stderr content
    oo().args(["ls", "/nonexistent_path_xyz_abc_12345"])
        .assert()
        .failure()
        .stdout(predicate::str::starts_with("\u{2717}")); // ✗
}

// ---------------------------------------------------------------------------
// parse_action dispatch (CLI integration)
// ---------------------------------------------------------------------------

#[test]
fn test_dispatch_version() {
    // `oo version` must exit 0
    oo().arg("version").assert().success();
}

#[test]
fn test_dispatch_forget() {
    // `oo forget` must exit 0 and clear session data
    oo().arg("forget")
        .assert()
        .success()
        .stdout(predicate::str::contains("Cleared session data"));
}

#[test]
fn test_dispatch_run() {
    // `oo echo hi` dispatches to run — exits 0 with command output
    oo().args(["echo", "hi"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hi"));
}

#[test]
fn test_dispatch_recall_query_joined() {
    // `oo recall hello world` — multi-word query joined, exits 0
    oo().args(["recall", "hello", "world"]).assert().success();
}

#[test]
fn test_dispatch_help() {
    // `oo help` — shows usage
    oo().arg("help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage"));
}

#[test]
fn test_dispatch_init() {
    // `oo init` — runs the init command (tested more fully in init block above)
    let dir = TempDir::new().unwrap();
    oo().arg("init").current_dir(dir.path()).assert().success();
}

// ---------------------------------------------------------------------------
// oo patterns subcommand
// ---------------------------------------------------------------------------

#[test]
fn test_patterns_no_learned_patterns() {
    // When no patterns exist (or patterns dir is absent), exit 0 and print "no learned patterns yet"
    // Set XDG_CONFIG_HOME so that dirs::config_dir() on Linux respects the temp HOME.
    let dir = TempDir::new().unwrap();
    oo().arg("patterns")
        .env("HOME", dir.path())
        .env("XDG_CONFIG_HOME", dir.path().join(".config"))
        .assert()
        .success()
        .stdout(predicate::str::contains("no learned patterns yet"));
}

#[test]
fn test_patterns_with_learned_pattern() {
    // When a valid pattern file exists, list it.
    // Use double_o::learn::patterns_dir() to resolve the platform-correct path
    // (macOS: ~/Library/Application Support/oo/patterns, Linux: ~/.config/oo/patterns).
    let dir = TempDir::new().unwrap();
    // Build the platform-appropriate path under our temp HOME by temporarily
    // setting HOME and querying dirs::config_dir equivalent logic.
    // We know patterns_dir() = dirs::config_dir()/oo/patterns, so replicate that.
    #[cfg(target_os = "macos")]
    let patterns_dir = dir
        .path()
        .join("Library")
        .join("Application Support")
        .join("oo")
        .join("patterns");
    #[cfg(not(target_os = "macos"))]
    let patterns_dir = dir.path().join(".config").join("oo").join("patterns");

    std::fs::create_dir_all(&patterns_dir).unwrap();
    std::fs::write(
        patterns_dir.join("pytest.toml"),
        "command_match = \"^pytest\"\n[success]\npattern = '(?P<n>\\d+) passed'\nsummary = \"{n} passed\"\n",
    )
    .unwrap();
    oo().arg("patterns")
        .env("HOME", dir.path())
        .env("XDG_CONFIG_HOME", dir.path().join(".config"))
        .assert()
        .success()
        .stdout(predicate::str::contains("^pytest"));
}

#[test]
fn test_learn_provider_logged_to_stderr() {
    // Part 1: provider name must appear in stderr before background spawn
    oo().args(["learn", "echo", "hello"])
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env_remove("CEREBRAS_API_KEY")
        .assert()
        .success()
        .stderr(predicate::str::contains("anthropic"));
}

// ---------------------------------------------------------------------------
// version / format tests
// ---------------------------------------------------------------------------

#[test]
fn test_version_shows_oo_prefix() {
    // The plain "oo" logo must appear in the version output
    oo().arg("version")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("oo "));
}

#[test]
fn test_version_shows_version_number() {
    // Must contain the version from Cargo.toml
    let version = env!("CARGO_PKG_VERSION");
    oo().arg("version")
        .assert()
        .success()
        .stdout(predicate::str::contains(version));
}

// ---------------------------------------------------------------------------
// help command integration
// ---------------------------------------------------------------------------

/// Network-dependent: looks up cheat.sh for `ls`.
/// Marked ignore — run manually when network is available.
#[test]
#[ignore = "requires network access to cheat.sh"]
fn test_help_with_valid_command() {
    oo().args(["help", "ls"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty().not());
}
