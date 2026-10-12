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
