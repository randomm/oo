use super::*;

#[test]
fn test_background_temp_dir_falls_back_to_tmp_when_empty() {
    assert_eq!(
        background_temp_dir_from(PathBuf::new()),
        PathBuf::from("/tmp")
    );
}

#[test]
fn test_background_temp_dir_falls_back_to_tmp_when_relative() {
    assert_eq!(
        background_temp_dir_from(PathBuf::from("relative/dir")),
        PathBuf::from("/tmp")
    );
}

#[test]
fn test_background_temp_dir_keeps_absolute_dir() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    assert_eq!(
        background_temp_dir_from(dir.path().to_path_buf()),
        dir.path().to_path_buf()
    );
}

#[test]
fn test_run_background_removes_payload_on_invalid_json() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let path = dir.path().join("payload.json");
    std::fs::write(&path, b"not valid json {{{").expect("write");
    assert!(run_background(path.to_str().expect("utf8 path")).is_err());
    assert!(!path.exists(), "payload must be removed on parse error");
}

#[test]
fn test_run_background_removes_payload_on_exit_code_out_of_range() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let path = dir.path().join("payload.json");
    let data = serde_json::json!({
        "command": "echo hello",
        "output": "hello",
        "exit_code": 2147483648i64
    });
    std::fs::write(&path, data.to_string()).expect("write");
    assert!(run_background(path.to_str().expect("utf8 path")).is_err());
    assert!(!path.exists(), "payload must be removed on exit-code error");
}

#[test]
fn test_run_background_missing_payload_stays_missing() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let path = dir.path().join("absent.json");
    assert!(run_background(path.to_str().expect("utf8 path")).is_err());
    assert!(!path.exists());
}
