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

/// Restores a directory's mode on drop so the temp dir can be cleaned up.
#[cfg(unix)]
struct RestoreMode(std::path::PathBuf);

#[cfg(unix)]
impl Drop for RestoreMode {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&self.0, PermissionsExt::from_mode(0o700));
    }
}

#[cfg(unix)]
#[test]
fn unreadable_user_dir_exits_1_with_error_and_no_empty_state() {
    use std::os::unix::fs::PermissionsExt;
    let config = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let patterns = config.path().join("patterns");
    std::fs::create_dir_all(&patterns).unwrap();
    std::fs::set_permissions(&patterns, PermissionsExt::from_mode(0o000)).unwrap();
    let _restore = RestoreMode(patterns.clone());
    if std::fs::read_dir(&patterns).is_ok() {
        // Running as root: mode bits do not restrict reads, so there is nothing to test.
        return;
    }

    let output = Command::new(env!("CARGO_BIN_EXE_oo"))
        .arg("patterns")
        .current_dir(cwd.path())
        .env("OO_CONFIG_DIR", config.path())
        .env("OO_DATA_DIR", data.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(1), "stderr: {stderr}");
    assert!(
        stderr.starts_with(&format!("oo: {}: ", patterns.display())),
        "{stderr}"
    );
    assert!(!stdout.contains("no learned patterns yet"), "{stdout}");
}

#[cfg(unix)]
#[test]
fn unsearchable_user_dir_entry_exits_1_with_error_and_no_empty_state() {
    use std::os::unix::fs::PermissionsExt;
    let config = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let patterns = config.path().join("patterns");
    std::fs::create_dir_all(&patterns).unwrap();
    let entry = patterns.join("x.toml");
    std::fs::write(&entry, "command_match = \"x\"\n").unwrap();
    // Read but not search: read_dir succeeds, stat on each entry fails with EACCES.
    std::fs::set_permissions(&patterns, PermissionsExt::from_mode(0o400)).unwrap();
    let _restore = RestoreMode(patterns.clone());
    if std::fs::metadata(&entry).is_ok() {
        // Running as root: mode bits do not restrict search, so there is nothing to test.
        return;
    }

    let output = Command::new(env!("CARGO_BIN_EXE_oo"))
        .arg("patterns")
        .current_dir(cwd.path())
        .env("OO_CONFIG_DIR", config.path())
        .env("OO_DATA_DIR", data.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(1), "stderr: {stderr}");
    assert!(
        stderr.starts_with(&format!("oo: {}: ", entry.display())),
        "{stderr}"
    );
    assert!(!stdout.contains("no learned patterns yet"), "{stdout}");
}

#[cfg(unix)]
#[test]
fn dangling_symlink_user_pattern_is_skipped_as_empty() {
    let config = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let patterns = config.path().join("patterns");
    std::fs::create_dir_all(&patterns).unwrap();
    std::os::unix::fs::symlink(
        patterns.join("missing-target.toml"),
        patterns.join("x.toml"),
    )
    .unwrap();

    let out = run_patterns(&config, &data, &cwd);
    let user_header = format!("User ({}):\n", patterns.display());
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
