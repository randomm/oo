//! Golden-output tests for the `User (<dir>):` section of `oo patterns`.

use super::write_user_section;

fn render(dir: &std::path::Path) -> String {
    let mut out = Vec::new();
    write_user_section(&mut out, dir).unwrap();
    String::from_utf8(out).unwrap()
}

#[test]
fn empty_user_dir_prints_empty_state_under_header() {
    let dir = tempfile::TempDir::new().unwrap();
    assert_eq!(
        render(dir.path()),
        format!(
            "User ({}):\n  no learned patterns yet\n",
            dir.path().display()
        )
    );
}

#[test]
fn missing_user_dir_prints_empty_state_under_header() {
    let dir = tempfile::TempDir::new().unwrap();
    let missing = dir.path().join("does-not-exist");
    assert_eq!(
        render(&missing),
        format!("User ({}):\n  no learned patterns yet\n", missing.display())
    );
}

#[test]
fn only_unparseable_toml_prints_empty_state() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("broken.toml"), "command_match = [").unwrap();
    assert_eq!(
        render(dir.path()),
        format!(
            "User ({}):\n  no learned patterns yet\n",
            dir.path().display()
        )
    );
}

#[test]
fn stray_toml_directory_is_skipped_not_an_error() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join("stray.toml")).unwrap();
    assert_eq!(
        render(dir.path()),
        format!(
            "User ({}):\n  no learned patterns yet\n",
            dir.path().display()
        )
    );
}

#[cfg(unix)]
#[test]
fn unreadable_user_dir_is_an_error_not_empty() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::TempDir::new().unwrap();
    let locked = dir.path().join("locked");
    std::fs::create_dir(&locked).unwrap();
    std::fs::set_permissions(&locked, PermissionsExt::from_mode(0o000)).unwrap();
    let _restore = RestoreMode(locked.clone());
    if std::fs::read_dir(&locked).is_ok() {
        // Running as root: mode bits do not restrict reads, so there is nothing to test.
        return;
    }

    let mut out = Vec::new();
    let err = write_user_section(&mut out, &locked).unwrap_err();
    assert!(
        err.to_string()
            .starts_with(&format!("{}: ", locked.display())),
        "{err}"
    );
    let printed = String::from_utf8(out).unwrap();
    assert!(!printed.contains("no learned patterns yet"), "{printed}");
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
fn dangling_symlink_toml_is_skipped_as_empty() {
    let dir = tempfile::TempDir::new().unwrap();
    std::os::unix::fs::symlink(
        dir.path().join("missing-target.toml"),
        dir.path().join("x.toml"),
    )
    .unwrap();
    assert_eq!(
        render(dir.path()),
        format!(
            "User ({}):\n  no learned patterns yet\n",
            dir.path().display()
        )
    );
}

#[cfg(unix)]
#[test]
fn unsearchable_user_dir_entry_is_an_error_not_empty() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::TempDir::new().unwrap();
    let entry = dir.path().join("x.toml");
    std::fs::write(&entry, "command_match = \"x\"\n").unwrap();
    std::fs::set_permissions(dir.path(), PermissionsExt::from_mode(0o400)).unwrap();
    let _restore = RestoreMode(dir.path().to_path_buf());
    if std::fs::metadata(&entry).is_ok() {
        // Running as root: mode bits do not restrict search, so there is nothing to test.
        return;
    }

    let mut out = Vec::new();
    let err = write_user_section(&mut out, dir.path()).unwrap_err();
    assert!(
        err.to_string()
            .starts_with(&format!("{}: ", entry.display())),
        "{err}"
    );
    let printed = String::from_utf8(out).unwrap();
    assert!(!printed.contains("no learned patterns yet"), "{printed}");
}

#[test]
fn valid_pattern_is_listed_without_empty_state() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("pytest.toml"),
        "command_match = \"pytest\"\n",
    )
    .unwrap();
    assert_eq!(
        render(dir.path()),
        format!("User ({}):\n  pytest\n", dir.path().display())
    );
}

#[test]
fn valid_pattern_with_flags_lists_flags() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("pytest.toml"),
        "command_match = \"pytest\"\n[success]\n[failure]\n",
    )
    .unwrap();
    assert_eq!(
        render(dir.path()),
        format!(
            "User ({}):\n  pytest  [success] [failure]\n",
            dir.path().display()
        )
    );
}
