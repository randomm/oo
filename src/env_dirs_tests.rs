use super::*;

#[test]
fn override_is_used_verbatim_without_oo_suffix() {
    let dir = PathBuf::from("/some/override");
    assert_eq!(config_dir_from(Some(dir.clone().into())), dir);
}

#[test]
fn unset_override_appends_oo_to_platform_dir() {
    let expected = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("oo");
    assert_eq!(config_dir_from(None), expected);
}

#[test]
fn empty_override_is_treated_as_unset() {
    assert_eq!(validate_override(OsStr::new("")), Ok(None));
}

#[test]
fn whitespace_only_override_is_treated_as_unset() {
    assert_eq!(validate_override(OsStr::new("   ")), Ok(None));
    assert_eq!(validate_override(OsStr::new("\t")), Ok(None));
}

#[test]
fn absolute_override_is_accepted_verbatim() {
    let raw = if cfg!(windows) { r"C:\oo" } else { "/abs/oo" };
    assert_eq!(
        validate_override(OsStr::new(raw)),
        Ok(Some(PathBuf::from(raw)))
    );
}

#[test]
fn relative_override_is_rejected() {
    assert!(validate_override(OsStr::new("rel/dir")).is_err());
    assert!(validate_override(OsStr::new(".")).is_err());
}

#[test]
fn padded_absolute_override_is_rejected_not_trimmed() {
    let raw = if cfg!(windows) { r" C:\oo" } else { " /abs/oo" };
    assert!(validate_override(OsStr::new(raw)).is_err());
}

#[test]
fn relative_config_override_falls_back_to_platform_dir() {
    let expected = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("oo");
    assert_eq!(config_dir_from(Some("relative".into())), expected);
}

#[test]
fn empty_config_override_falls_back_to_platform_dir() {
    let expected = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("oo");
    assert_eq!(config_dir_from(Some("".into())), expected);
}

#[test]
fn resolve_override_returns_none_for_relative_value() {
    assert_eq!(
        resolve_override("OO_TEST_RESOLVE_REL", Some("rel".into())),
        None
    );
}

#[test]
fn warning_gate_fires_once_per_name() {
    assert!(mark_warned("OO_TEST_GATE_ONCE"));
    assert!(!mark_warned("OO_TEST_GATE_ONCE"));
    assert!(!mark_warned("OO_TEST_GATE_ONCE"));
}

#[test]
fn warning_gate_is_per_variable() {
    assert!(mark_warned("OO_TEST_GATE_A"));
    assert!(mark_warned("OO_TEST_GATE_B"));
}
