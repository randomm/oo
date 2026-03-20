/// Get the current session identifier.
///
/// Returns the parent process ID as a string, providing a stable identifier
/// for all commands run within the same AI agent session.
pub fn session_id() -> String {
    let ppid = unsafe { libc::getppid() };
    ppid.to_string()
}

/// Get the project identifier for the current working directory.
///
/// Returns a unique identifier for the project, used to scope store operations.
/// Attempts to detect from git remote URL, git root directory, or current directory name.
pub fn project_id() -> String {
    #[cfg(feature = "vipune-store")]
    {
        vipune::detect_project(None)
    }
    #[cfg(not(feature = "vipune-store"))]
    {
        detect_project_fallback()
    }
}

#[cfg(not(feature = "vipune-store"))]
fn detect_project_fallback() -> String {
    // Try git remote origin
    if let Ok(output) = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
    {
        if output.status.success() {
            let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Some(name) = url.rsplit('/').next() {
                let name = name.strip_suffix(".git").unwrap_or(name);
                if !name.is_empty() {
                    return name.to_string();
                }
            }
        }
    }

    // Try git root directory name
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Some(name) = path.rsplit('/').next() {
                if !name.is_empty() {
                    return name.to_string();
                }
            }
        }
    }

    // Current directory name
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(name) = cwd.file_name() {
            return name.to_string_lossy().to_string();
        }
    }

    "unknown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_id_is_nonzero() {
        let id = session_id();
        let pid: u32 = id.parse().expect("session_id should be numeric");
        assert!(pid > 0);
    }

    #[test]
    fn test_session_id_stable() {
        assert_eq!(session_id(), session_id());
    }

    #[test]
    fn test_project_id_nonempty() {
        let id = project_id();
        assert!(!id.is_empty());
    }

    // --- detect_project_fallback branch tests ---
    // We test the function indirectly through project_id(), which always
    // delegates to detect_project_fallback() in the non-vipune build.

    #[test]
    fn test_project_id_is_string() {
        // project_id must return a valid (non-empty) UTF-8 string
        let id = project_id();
        assert!(!id.is_empty(), "project_id must not be empty");
        assert!(
            id.is_ascii() || !id.is_empty(),
            "project_id must be a string"
        );
    }

    #[test]
    fn test_project_id_no_newlines() {
        // The project identifier must not contain newlines (raw git output is trimmed)
        let id = project_id();
        assert!(
            !id.contains('\n'),
            "project_id must not contain newlines, got: {id:?}"
        );
    }

    #[test]
    #[cfg(not(feature = "vipune-store"))]
    fn test_detect_project_fallback_cwd_fallback() {
        // When run inside a temp dir with no git, detect_project_fallback must
        // still return a non-empty string (the directory name or "unknown").
        let tmp = tempfile::tempdir().expect("tempdir");
        let original = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(tmp.path()).expect("set_current_dir");

        let id = detect_project_fallback();

        std::env::set_current_dir(&original).expect("restore cwd");
        // Either the dir name (a UUID-ish string) or "unknown" — both are acceptable
        assert!(!id.is_empty(), "fallback must not be empty");
    }
}
