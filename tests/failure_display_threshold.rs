//! Failure-display threshold behaviour (issue #194).
//!
//! A failing unrecognised command whose output is at most 120 lines must be
//! displayed in full, with no truncation marker and no recall hint, and must
//! not be indexed (an `oo recall` for one of its lines finds nothing). Above
//! 120 lines the display is head/marker/tail and the full output must be
//! indexed for `oo recall`.

use assert_cmd::Command;
use std::path::PathBuf;
use tempfile::TempDir;

const TOOL: &str = "zfailtool194";
const HIDDEN_MARKER: &str = "oo194hiddenmarker7e2a";

fn oo() -> Command {
    Command::cargo_bin("oo").unwrap()
}

/// Runs `oo TOOL` where `TOOL` is a fake failing executable with `script_body`
/// on PATH, against an isolated data dir. Returns the temp root (which owns
/// `oo-data/`, the data dir) and the captured stdout.
fn run_failing_tool(script_body: &str) -> (TempDir, String) {
    use std::os::unix::fs::PermissionsExt;
    let root = TempDir::new().unwrap();
    let bin_dir = root.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let bin = bin_dir.join(TOOL);
    std::fs::write(&bin, script_body).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let data_dir = data_dir_of(&root);
    std::fs::create_dir_all(&data_dir).unwrap();

    let assert = oo()
        .args([TOOL])
        .env("PATH", &path)
        .env("OO_DATA_DIR", &data_dir)
        .assert()
        .failure();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    (root, stdout)
}

fn data_dir_of(root: &TempDir) -> PathBuf {
    root.path().join("oo-data")
}

fn script_printing(lines: u32) -> String {
    format!("#!/bin/sh\nfor i in $(seq 1 {lines}); do echo \"line $i\"; done\nexit 1\n")
}

#[test]
fn test_failure_100_lines_is_displayed_in_full_and_not_indexed() {
    let (_root, stdout) = run_failing_tool(&script_printing(100));

    for i in 1..=100 {
        assert!(
            stdout.lines().any(|l| l == format!("line {i}")),
            "line {i} must be displayed in full; got: {stdout:?}"
        );
    }
    assert!(
        !stdout.contains("lines truncated"),
        "100 lines must not be truncated; got: {stdout:?}"
    );
    assert!(
        !stdout.contains("oo recall"),
        "100 lines must not advertise recall; got: {stdout:?}"
    );
}

#[test]
fn test_failure_200_lines_is_indexed_and_recallable() {
    // Marker at line 100 falls in the truncated middle of a 200-line output.
    let script = format!(
        "#!/bin/sh\nfor i in $(seq 1 200); do if [ $i -eq 100 ]; then echo {HIDDEN_MARKER}; else echo \"line $i\"; fi; done\nexit 1\n"
    );
    let (root, stdout) = run_failing_tool(&script);

    assert!(
        stdout.contains("lines truncated"),
        "200 lines must be truncated in display; got: {stdout:?}"
    );
    assert!(
        !stdout.contains(HIDDEN_MARKER),
        "marker must be outside the displayed slice; got: {stdout:?}"
    );
    assert!(
        stdout.contains("use `oo recall` to query"),
        "recall must be advertised for 200 lines; got: {stdout:?}"
    );

    let out = oo()
        .args(["recall", HIDDEN_MARKER])
        .env("OO_DATA_DIR", data_dir_of(&root))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let recall = String::from_utf8_lossy(&out).to_string();
    assert!(
        recall.contains(HIDDEN_MARKER),
        "recall must return the hidden marker; got: {recall:?}"
    );
}

#[test]
fn test_failure_120_lines_is_displayed_in_full_and_not_indexed() {
    let (root, stdout) = run_failing_tool(&script_printing(120));

    for i in 1..=120 {
        assert!(
            stdout.lines().any(|l| l == format!("line {i}")),
            "line {i} must be displayed in full; got: {stdout:?}"
        );
    }
    assert!(!stdout.contains("lines truncated"));
    assert!(
        !stdout.contains("oo recall"),
        "120 lines must not advertise recall; got: {stdout:?}"
    );

    let recall = oo()
        .args(["recall", "line 60"])
        .env("OO_DATA_DIR", data_dir_of(&root))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let recall = String::from_utf8_lossy(&recall).to_string();
    assert!(
        recall.contains("No results found."),
        "120-line output must not be indexed; recall got: {recall:?}"
    );
}

#[test]
fn test_failure_121_lines_is_truncated_with_marker_and_split_at_72_48() {
    let (root, stdout) = run_failing_tool(&script_printing(121));

    // head = 72 lines (1..=72), one line dropped (73), tail = 48 lines (74..=121)
    assert!(stdout.lines().any(|l| l == "line 72"), "got: {stdout:?}");
    assert!(stdout.lines().any(|l| l == "line 74"), "got: {stdout:?}");
    assert!(
        stdout.contains("... [1 lines truncated] ..."),
        "got: {stdout:?}"
    );
    assert!(!stdout.contains("line 73"), "got: {stdout:?}");

    let recall = oo()
        .args(["recall", "--full", "line 73"])
        .env("OO_DATA_DIR", data_dir_of(&root))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let recall = String::from_utf8_lossy(&recall).to_string();
    assert!(
        recall.contains("line 73"),
        "dropped line 73 must be recallable; recall got: {recall:?}"
    );
}
