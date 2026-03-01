pub fn session_id() -> String {
    let ppid = unsafe { libc::getppid() };
    ppid.to_string()
}

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
}
