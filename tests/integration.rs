use assert_cmd::Command;
use predicates::prelude::*;
use std::os::unix::fs::PermissionsExt;
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
    // `git log` is categorized as Data, so large output gets indexed (●).
    // Create a temp git repo to generate logs that exceed 4KB threshold.
    let dir = TempDir::new().unwrap();
    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Create multiple commits with content to exceed 4KB
    for i in 0..100 {
        std::fs::write(dir.path().join("file.txt"), format!("content {}\n", i)).unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", &format!("commit {}", i)])
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .current_dir(dir.path())
            .output()
            .unwrap();
    }

    // Run `oo git log` in the repo — should get ● (Large/indexed) indicator
    oo().args(["git", "log"])
        .current_dir(dir.path())
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
// recall — snippet (bounded default) and --full integration tests
// ---------------------------------------------------------------------------
//
// These tests use OO_DATA_DIR to isolate the store so they don't touch the
// developer's real ~/.local/share/.oo/oo.db (or vice versa).
// ---------------------------------------------------------------------------

/// Seed a large indexed blob and return (temp_dir, blob_content).
///
/// The blob is > 4 KB so it hits the Large tier (● indexed indicator).
fn index_large_blob() -> (TempDir, String) {
    let dir = TempDir::new().unwrap();
    let blob: String = (0..200)
        .map(|i| {
            format!(
                "sentinel_line_{i:04} data_padding_abcdef_{}\n",
                "x".repeat(50)
            )
        })
        .collect();
    // ~ 200 * 85 = 17 KB — well above 4 KB threshold

    // OO_DATA_DIR isolates the store for this test run
    // `oo <cmd>` where cmd produces > 4KB output → Large tier → indexed
    let blob_file = dir.path().join("blob.txt");
    std::fs::write(&blob_file, &blob).unwrap();

    // Use `cat` to produce large output (cat is Data category → Large tier)
    let mut cmd = oo();
    cmd.args(["cat", blob_file.to_str().unwrap()]);
    cmd.env("OO_DATA_DIR", dir.path());
    cmd.assert().success();

    (dir, blob)
}

#[test]
fn test_recall_default_is_bounded() {
    // Index a large blob, recall it — default output must be bounded
    let (dir, blob) = index_large_blob();

    // Recall with a token that appears in the blob
    let mut cmd = oo();
    cmd.args(["recall", "sentinel_line_0100"]);
    cmd.env("OO_DATA_DIR", dir.path());
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "recall must exit 0");

    // The stdout must be bounded: well under the full blob size
    let stdout_len = stdout.len();
    let blob_len = blob.len();
    assert!(
        stdout_len < blob_len,
        "default recall stdout ({stdout_len} B) must be < full blob ({blob_len} B)"
    );

    // The output must not be empty (results were found)
    assert!(
        !stdout.trim().is_empty(),
        "default recall must produce output"
    );

    // The output should NOT contain the tail of the blob (last 100 chars)
    let blob_tail = &blob[blob.len().saturating_sub(100)..];
    assert!(
        !stdout.contains(blob_tail),
        "default recall must NOT contain the tail of the blob (bounded display)"
    );
}

#[test]
fn test_recall_full_shows_complete_blob() {
    // --full must show the complete stored content
    let (dir, _blob) = index_large_blob();

    let mut cmd = oo();
    cmd.args(["recall", "--full", "sentinel_line_0100"]);
    cmd.env("OO_DATA_DIR", dir.path());
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "recall --full must exit 0");

    // The full blob content must appear (at least the last sentinel line)
    assert!(
        stdout.contains("sentinel_line_0199"),
        "--full recall must contain the last sentinel line of the blob"
    );
}

#[test]
fn test_recall_full_flag_position_independent() {
    // Both `oo recall --full <q>` and `oo recall <q> --full` must work
    let (dir, _blob) = index_large_blob();

    // Flag before query
    let mut cmd1 = oo();
    cmd1.args(["recall", "--full", "sentinel_line_0100"]);
    cmd1.env("OO_DATA_DIR", dir.path());
    let out1 = cmd1.output().unwrap();
    assert!(out1.status.success(), "recall --full <q> must exit 0");

    // Flag after query
    let mut cmd2 = oo();
    cmd2.args(["recall", "sentinel_line_0100", "--full"]);
    cmd2.env("OO_DATA_DIR", dir.path());
    let out2 = cmd2.output().unwrap();
    assert!(out2.status.success(), "recall <q> --full must exit 0");

    // Both must contain the last sentinel (proves --full mode)
    let stdout1 = String::from_utf8_lossy(&out1.stdout);
    let stdout2 = String::from_utf8_lossy(&out2.stdout);
    assert!(
        stdout1.contains("sentinel_line_0199"),
        "flag-before must be --full mode"
    );
    assert!(
        stdout2.contains("sentinel_line_0199"),
        "flag-after must be --full mode"
    );
}

#[test]
fn test_recall_full_alone_gives_empty_query_error() {
    // `oo recall --full` alone → empty query → error exit 1
    oo().args(["recall", "--full"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("recall requires a query"));
}

#[test]
fn test_recall_default_displays_fts_snippet_not_content_prefix() {
    // Guard for the whole FTS5 snippet feature: the default (non-`--full`)
    // recall output must contain the matched sentinel token — proving the
    // store-provided FTS5 snippet (centered on the best match) is what gets
    // displayed. If the wiring is removed and `cmd_recall` falls back to
    // `bounded_display(&r.content)` (the first ~512 chars of the full blob),
    // the sentinel token — placed well beyond the first 512 chars — will be
    // absent and this test will fail.
    let dir = TempDir::new().unwrap();
    // Build a blob whose first 512 chars contain NO occurrence of the matched
    // token: 150 filler tokens (1350 chars) before the sentinel.
    let filler: String = (0..500).map(|i| format!("filler{i:04}_ ")).collect(); // 500*11 = 5500 chars of filler
    let blob: String = format!("{filler}sentinel_beta_9999 {filler}"); // ~11000 chars total (well above 4 KB → Bounded/Large tier)
    let blob_file = dir.path().join("blob.txt");
    std::fs::write(&blob_file, &blob).unwrap();

    let mut cmd = oo();
    cmd.args(["cat", blob_file.to_str().unwrap()]);
    cmd.env("OO_DATA_DIR", dir.path());
    cmd.assert().success();

    // Default (bounded) recall for a token that appears only in the middle of
    // the blob — never in the first 512 chars.
    let mut cmd = oo();
    cmd.args(["recall", "sentinel_beta_9999"]);
    cmd.env("OO_DATA_DIR", dir.path());
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "recall must exit 0");

    // The matched sentinel must be shown: an FTS5 snippet centered on the
    // match contains it; `bounded_display(&content)` (first ~512 chars, all
    // filler) does not. The blob is ~11 000 chars (well above 4 KB) so `oo cat`
    // indexes the full content and the FTS5 branch is exercised.
    assert!(
        stdout.contains("sentinel_beta_9999"),
        "default recall must display the FTS5 snippet centered on the matched token\ngot: {stdout}"
    );

    // Sanity: the output is bounded (not the full blob).
    let stdout_len = stdout.len();
    let blob_len = blob.len();
    assert!(
        stdout_len < blob_len,
        "default recall stdout ({stdout_len} B) must be < full blob ({blob_len} B)"
    );
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
    // When no learned patterns exist, built-in patterns are still shown
    let dir = TempDir::new().unwrap();
    oo().arg("patterns")
        .env("HOME", dir.path())
        .env("XDG_CONFIG_HOME", dir.path().join(".config"))
        .assert()
        .success()
        .stdout(predicate::str::contains("Built-in"))
        .stdout(predicate::str::contains("User"));
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

// ---------------------------------------------------------------------------
// Large Content/Unknown passthrough repros (issue #148)
//
// These are the exact live repros from the ticket: pre-fix, `oo cat` on a
// 110KB file emitted exactly 110,000 bytes and a 30,000-echo `sh -c` emitted
// exactly 150,000 bytes verbatim into the agent's context. Each test
// self-isolates its store via OO_DATA_DIR (fresh temp dir per test) so the
// recall assertions only see the row this test indexed.
//
// The recall assertions deliberately check RETRIEVABILITY ONLY — a distinctive
// marker token embedded in the tail of the original output must return a
// NON-EMPTY recall result whose stored content contains the token. They must
// NOT assert on the shape/boundedness of `oo recall`'s printed output; that is
// issue #147's job.
// ---------------------------------------------------------------------------

/// A single, unambiguous FTS5 token — guaranteed to be a one-token query that
/// the store's per-row phrase matching can resolve (no punctuation, no spaces).
const RECALL_MARKER: &str = "oo148tailmarker7f3a9c";

/// Build a 110,000-byte file whose contents are all distinct 22-character
/// lines (positions 0..5000), each containing the RECALL_MARKER token. The
/// file is large enough to trigger the >4KB threshold, and the marker is
/// guaranteed to be a single FTS5 token in the indexed row. The file's tail
/// (last 5000 lines) also contains the marker, so it survives the
/// byte-based head+tail truncation (tail budget = 40% of 4096 ≈ 1638 bytes ≈
/// ~74 lines of 22-char lines, well within the last 5000 lines).
fn write_repro_file(dir: &std::path::Path) -> std::path::PathBuf {
    let mut content = String::new();
    for i in 0..5000 {
        content.push_str(&format!("line{i:06} {RECALL_MARKER}\n"));
    }
    let path = dir.join("big.txt");
    std::fs::write(&path, content).unwrap();
    path
}

/// Isolate the store under a fresh temp data dir. Returns the TempDir (kept
/// alive for the duration of the test) and the data-dir path to pass to
/// OO_DATA_DIR on every oo invocation in the test.
fn isolated_store() -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path().join("oo-data");
    std::fs::create_dir_all(&data_dir).unwrap();
    (dir, data_dir)
}

/// Run `oo recall <marker>` against the isolated store and assert the result is
/// non-empty AND contains the marker token. Returns the recall stdout for the
/// caller to inspect if needed.
fn assert_recall_contains(data_dir: &std::path::Path, marker: &str) {
    let out = oo()
        .args(["recall", marker])
        .env("OO_DATA_DIR", data_dir)
        .assert()
        .success()
        .to_string();
    assert!(
        !out.contains("No results found"),
        "recall on the tail marker must return a non-empty result; got: {out:?}"
    );
    assert!(
        out.contains(marker),
        "recall result must contain the tail marker token; got: {out:?}"
    );
}

#[test]
fn test_content_arm_large_output_bounded_and_recallable() {
    // Repro A (Content arm): `oo cat` on a 110,000-byte file. Pre-fix this
    // emitted exactly 110,000 bytes verbatim. Post-fix: stdout must be bounded
    // far under the full size, the full content must be indexed, and a recall
    // seeded with the tail marker must return a non-empty result containing it.
    let file = TempDir::new().unwrap();
    let big = write_repro_file(file.path());

    let (_guard, data_dir) = isolated_store();
    oo().args(["cat", big.to_str().unwrap()])
        .env("OO_DATA_DIR", &data_dir)
        .assert()
        .success()
        // The pre-fix byte-exact 110,000-byte dump is the bug: bound well under it.
        // The fix caps display at 4096 bytes + one marker line; 20KB leaves a wide
        // margin over that while still being provably not the verbatim 110KB dump.
        .stdout(predicate::function(|out: &str| out.len() < 20_000))
        .stdout(predicate::str::contains(RECALL_MARKER));

    // The full content must have been indexed — recall on the tail marker
    // returns a NON-EMPTY result whose stored content contains the token.
    assert_recall_contains(&data_dir, RECALL_MARKER);
}

#[test]
fn test_unknown_arm_large_output_bounded_and_recallable() {
    // Repro B (Unknown arm): `oo sh -c '<30000 lines> + <marker line>'`.
    // Pre-fix this emitted ~150,000 bytes verbatim (30,000 x "line\n").
    // The shell loop runs first, then the marker line is appended, so the
    // marker sits in the TAIL of the output — the slice that must survive the
    // byte-based head+tail truncation to be displayed, and the whole output
    // (including the marker) is what gets indexed for recall.
    let (_guard, data_dir) = isolated_store();
    oo().args([
        "sh",
        "-c",
        &format!("for i in $(seq 1 30000); do echo line; done; echo {RECALL_MARKER}"),
    ])
    .env("OO_DATA_DIR", &data_dir)
    .assert()
    .success()
    .stdout(predicate::function(|out: &str| out.len() < 20_000))
    .stdout(predicate::str::contains(RECALL_MARKER));

    assert_recall_contains(&data_dir, RECALL_MARKER);
}

// ---------------------------------------------------------------------------
// cargo-dist configuration verification
// ---------------------------------------------------------------------------

#[test]
fn test_cargo_toml_has_shell_installer_enabled() {
    // Verify that Cargo.toml configures shell installer for cargo-dist
    let cargo_toml = include_str!("../Cargo.toml");
    assert!(
        cargo_toml.contains(r#"installers = ["shell"]"#),
        "Cargo.toml must have shell installer enabled: installers = [\"shell\"]"
    );
}

#[test]
fn test_cargo_toml_has_install_path_cargo_home() {
    // Verify that Cargo.toml configures install-path as CARGO_HOME
    let cargo_toml = include_str!("../Cargo.toml");
    assert!(
        cargo_toml.contains(r#"install-path = "CARGO_HOME""#),
        "Cargo.toml must have install-path = \"CARGO_HOME\""
    );
}

#[test]
fn test_cargo_toml_has_cargo_dist_version() {
    // Verify that Cargo.toml specifies a cargo-dist version
    let cargo_toml = include_str!("../Cargo.toml");
    assert!(
        cargo_toml.contains("cargo-dist-version ="),
        "Cargo.toml must specify cargo-dist-version for dist config"
    );
}

// ---------------------------------------------------------------------------
// project patterns (.oo/patterns)
// ---------------------------------------------------------------------------

#[test]
fn test_project_patterns_listed_when_present() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".git")).unwrap();
    let pat_dir = dir.path().join(".oo").join("patterns");
    std::fs::create_dir_all(&pat_dir).unwrap();
    std::fs::write(
        pat_dir.join("mytest.toml"),
        "command_match = \"^mytest\"\n[success]\npattern = '(?P<n>\\d+) ok'\nsummary = \"{n} ok\"\n",
    )
    .unwrap();

    oo().arg("patterns")
        .current_dir(dir.path())
        .env("HOME", dir.path())
        .env("XDG_CONFIG_HOME", dir.path().join(".config"))
        .assert()
        .success()
        .stdout(predicate::str::contains("Project"))
        .stdout(predicate::str::contains("^mytest"));
}

#[test]
fn test_project_patterns_override_builtins() {
    // Create a project pattern for "echo" with a success pattern that matches
    // the output, proving project patterns are loaded and take effect.
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".git")).unwrap();
    let pat_dir = dir.path().join(".oo").join("patterns");
    std::fs::create_dir_all(&pat_dir).unwrap();

    // A pattern that matches "echo" and summarises any output as "proj-match"
    std::fs::write(
        pat_dir.join("echo.toml"),
        "command_match = \"^echo\\\\b\"\n[success]\npattern = '(?s)(?P<all>.+)'\nsummary = \"proj-match\"\n",
    )
    .unwrap();

    // Generate enough output to exceed SMALL_THRESHOLD so the pattern is consulted.
    // SMALL_THRESHOLD is 4096 bytes; we need > 4096 bytes of output.
    let big_arg = "x".repeat(5000);
    oo().args(["echo", &big_arg])
        .current_dir(dir.path())
        .env("HOME", dir.path())
        .env("XDG_CONFIG_HOME", dir.path().join(".config"))
        .assert()
        .success()
        .stdout(predicate::str::contains("proj-match"));
}

// ---------------------------------------------------------------------------
// Savings suffix (issue #150)
// ---------------------------------------------------------------------------

/// Create a temp bin dir containing an executable script named `name` whose
/// body is `script_body`, and return the PATH with that dir prepended.
fn make_fake_bin(name: &str, script_body: &str) -> (TempDir, String) {
    let dir = TempDir::new().unwrap();
    let bin_dir = dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let bin = bin_dir.join(name);
    std::fs::write(&bin, script_body).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    (dir, path)
}

/// Quiet Success: a Status-category command with >4 KB output and no matching
/// builtin success pattern produces `✓ {label}` (empty summary). The
/// `[saved …]` suffix must appear on the indicator line.
#[test]
fn test_quiet_success_shows_savings() {
    let (keep, path) = make_fake_bin(
        "go",
        "#!/bin/sh\nfor i in $(seq 1 500); do echo 'building component $i ...'; done\nexit 0\n",
    );
    let _ = keep;
    oo().args(["go", "build"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicate::str::starts_with("\u{2713}"))
        .stdout(predicate::str::contains("[saved "))
        // The suffix must be on the indicator line, not on subsequent lines.
        .stdout(predicate::function(|out: &str| {
            let first_line = out.lines().next().unwrap_or("");
            first_line.starts_with('\u{2713}') && first_line.contains("[saved ")
        }));
}

/// Pattern-summarised Success: >4 KB output matching a builtin pattern with a
/// non-empty summary → `✓ {label} ({summary}) [saved …]`.
#[test]
fn test_summary_success_shows_savings() {
    let (keep, path) = make_fake_bin(
        "cargo",
        "#!/bin/sh\nfor i in $(seq 1 500); do echo \"running test $i ...\"; done\necho \"test result: ok. 42 passed; 0 failed; finished in 1.23s\"\nexit 0\n",
    );
    let _ = keep;
    oo().args(["cargo", "test"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicate::str::starts_with("\u{2713}"))
        .stdout(predicate::str::contains("42 passed"))
        .stdout(predicate::str::contains("[saved "))
        .stdout(predicate::function(|out: &str| {
            let first_line = out.lines().next().unwrap_or("");
            first_line.starts_with('\u{2713}') && first_line.contains("[saved ")
        }));
}

/// Filtered Failure: >4 KB output, non-zero exit. The `[saved …]` suffix must
/// appear on the `✗` indicator line; the filtered output lines that follow
/// must NOT contain the suffix.
#[test]
fn test_failure_shows_savings() {
    let (keep, path) = make_fake_bin(
        "pytest",
        "#!/bin/sh\nfor i in $(seq 1 500); do echo \"ERROR: test_failing_$i failed with assertion error\"; done\nexit 1\n",
    );
    let _ = keep;
    oo().args(["pytest", "-x"])
        .env("PATH", path)
        .assert()
        .failure()
        .stdout(predicate::str::starts_with("\u{2717}"))
        .stdout(predicate::str::contains("[saved "))
        .stdout(predicate::function(|out: &str| {
            let first_line = out.lines().next().unwrap_or("");
            first_line.starts_with('\u{2717}') && first_line.contains("[saved ")
        }));
}

/// REGRESSION LOCK — pre-existing failure formatting: the `✗ {label}`
/// indicator line is followed by a deliberate BLANK LINE before the error
/// body (the `\n` in the payload plus `println!`'s own newline). This must
/// survive both with and without a savings suffix present.
#[test]
fn test_failure_indicator_blank_line_with_suffix() {
    let (keep, path) = make_fake_bin(
        "pytest",
        "#!/bin/sh\nfor i in $(seq 1 500); do echo \"ERROR: test_failing_$i failed with assertion error\"; done\nexit 1\n",
    );
    let _ = keep;
    oo().args(["pytest", "-x"])
        .env("PATH", path)
        .assert()
        .failure()
        .stdout(predicate::function(|out: &str| {
            let mut lines = out.lines();
            let indicator = lines.next().unwrap_or("");
            let blank = lines.next().unwrap_or("");
            indicator.starts_with('\u{2717}') && indicator.contains("[saved ") && blank.is_empty()
        }));
}

/// Same regression as above for the small-failure case where the savings are
/// below `MIN_SAVINGS` and no suffix is printed: the blank line still holds.
#[test]
fn test_failure_indicator_blank_line_without_suffix() {
    oo().args(["false"])
        .assert()
        .failure()
        .stdout(predicate::function(|out: &str| {
            let mut lines = out.lines();
            let indicator = lines.next().unwrap_or("");
            let blank = lines.next().unwrap_or("");
            indicator.starts_with('\u{2717}') && !indicator.contains("[saved ") && blank.is_empty()
        }));
}

/// Large arm regression lock: the `● {label} (indexed … → use `oo recall` to
/// query)` string is unchanged — no `[saved …]` suffix (it already reports
/// its size; no double-reporting).
#[test]
fn test_large_arm_wording_unchanged_no_savings() {
    let dir = TempDir::new().unwrap();
    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    for i in 0..100 {
        std::fs::write(dir.path().join("file.txt"), format!("content {}\n", i)).unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", &format!("commit {}", i)])
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .current_dir(dir.path())
            .output()
            .unwrap();
    }

    oo().args(["git", "log"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::starts_with("\u{25CF}"))
        .stdout(predicate::str::contains("indexed"))
        .stdout(predicate::str::contains("use `oo recall` to query"))
        .stdout(predicate::str::contains("[saved ").not());
}

/// Bounded arm regression lock: the `● {label} (output truncated: …)` line is
/// unchanged — no `[saved …]` suffix (its framing line already carries the
/// total size; no double-reporting).
#[test]
fn test_bounded_arm_wording_unchanged_no_savings() {
    let dir = TempDir::new().unwrap();
    let big_file = dir.path().join("big.txt");
    let content = std::iter::repeat("x\n").take(5000).collect::<String>();
    std::fs::write(&big_file, &content).unwrap();

    oo().args(["cat", big_file.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("\u{25CF}"))
        .stdout(predicate::str::contains("output truncated:"))
        .stdout(predicate::str::contains("use `oo recall` to query"))
        .stdout(predicate::str::contains("[saved ").not());
}

/// Passthrough arm: small output (< 4 KB) passes through verbatim with no
/// indicator line and no savings figure.
#[test]
fn test_passthrough_no_savings() {
    oo().args(["echo", "hello"])
        .assert()
        .success()
        .stdout("hello\n");
}
