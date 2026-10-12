use super::*;

#[test]
fn test_is_owned_payload_accepts_tmp_file_in_payload_dir() {
    let dir = PathBuf::from("/some/tmp");
    assert!(is_owned_payload(&dir.join(".tmpAbC123"), &dir));
}

#[test]
fn test_is_owned_payload_rejects_other_names_and_dirs() {
    let dir = PathBuf::from("/some/tmp");
    assert!(!is_owned_payload(&dir.join("payload.json"), &dir));
    assert!(!is_owned_payload(
        &PathBuf::from("/elsewhere/.tmpAbC123"),
        &dir
    ));
    assert!(!is_owned_payload(
        &dir.join("nested").join(".tmpAbC123"),
        &dir
    ));
}

#[test]
fn test_run_background_keeps_unowned_payload() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let path = dir.path().join("payload.json");
    std::fs::write(&path, b"not valid json {{{").expect("write");
    assert!(run_background(path.to_str().expect("utf8 path")).is_err());
    assert!(
        path.exists(),
        "a path that spawn_background did not create must not be deleted"
    );
}

#[test]
fn test_run_background_missing_payload_stays_missing() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let path = dir.path().join("absent.json");
    assert!(run_background(path.to_str().expect("utf8 path")).is_err());
    assert!(!path.exists());
}
