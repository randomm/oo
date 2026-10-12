//! `oo patterns` must print the empty-state line under the User header when no
//! user patterns load, and list the pattern when one does.

use std::process::Command;
use tempfile::TempDir;

fn run_patterns(config: &TempDir, data: &TempDir, cwd: &TempDir) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_oo"))
        .arg("patterns")
        .current_dir(cwd.path())
        .env("OO_CONFIG_DIR", config.path())
        .env("OO_DATA_DIR", data.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn empty_user_dir_prints_empty_state_under_user_header() {
    let config = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();

    let out = run_patterns(&config, &data, &cwd);
    let user_header = format!("User ({}):\n", config.path().join("patterns").display());
    let tail = out.split(&user_header).nth(1).expect("User header present");
    assert_eq!(tail, "  no learned patterns yet\n");
}

#[test]
fn user_pattern_replaces_empty_state_line() {
    let config = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let patterns = config.path().join("patterns");
    std::fs::create_dir_all(&patterns).unwrap();
    std::fs::write(
        patterns.join("pytest.toml"),
        "command_match = \"^pytest\"\n",
    )
    .unwrap();

    let out = run_patterns(&config, &data, &cwd);
    assert!(!out.contains("no learned patterns yet"), "{out}");
    assert!(out.ends_with("  ^pytest\n"), "{out}");
}
