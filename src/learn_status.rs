//! The one-line status file shared by the background learn process (writer)
//! and the next foreground `oo` invocation (reader and clearer).

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::config::config_dir;

/// Path to the one-line status file written by the background learn process.
pub fn learn_status_path() -> PathBuf {
    config_dir().join("learn-status.log")
}

/// Open the learn status file for appending, creating the config directory
/// first so a fresh `OO_CONFIG_DIR` still receives failure entries.
fn open_learn_status(status_path: &Path) -> Result<std::fs::File, std::io::Error> {
    if let Some(parent) = status_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    // Lines embed command labels, so other local users must not read them.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(status_path)
}

/// Write a one-line status entry to the learn status file.
///
/// Called by the background process after successfully saving a pattern so
/// the NEXT foreground invocation can display the result.
pub fn write_learn_status(
    status_path: &Path,
    cmd_name: &str,
    pattern_path: &Path,
) -> Result<(), std::io::Error> {
    let line = format!(
        "learned pattern for {} → {}\n",
        cmd_name,
        pattern_path.display()
    );
    write_line(status_path, &line)
}

/// Write a one-line failure entry to the learn status file.
///
/// Called by the background process when `run_learn` returns `Err`, so the
/// NEXT foreground invocation can display the error.
pub fn write_learn_status_failure(
    status_path: &Path,
    cmd_name: &str,
    error_msg: &str,
) -> Result<(), std::io::Error> {
    let first_line = error_msg.lines().next().unwrap_or(error_msg);
    write_line(status_path, &format!("FAILED {cmd_name}: {first_line}\n"))
}

/// Append one whole line with a single `write_all`, so concurrent children
/// cannot interleave fragments of their entries.
fn write_line(status_path: &Path, line: &str) -> Result<(), std::io::Error> {
    let mut file = open_learn_status(status_path)?;
    file.write_all(line.as_bytes())
}

/// Check for a pending learn-status file, print its contents to stderr, then
/// delete the file.  Called early in each foreground invocation.
pub fn check_and_clear_learn_status(status_path: &Path) {
    if let Ok(content) = std::fs::read_to_string(status_path) {
        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("FAILED ") {
                // Format: "FAILED cmd-name: error message"
                if let Some((cmd, msg)) = rest.split_once(": ") {
                    eprintln!("oo: learn failed for {cmd} — {msg}");
                } else {
                    eprintln!("oo: learn failed — {rest}");
                }
            } else {
                eprintln!("oo: {line}");
            }
        }
        // Left in place, the same lines would print again on every run.
        if let Err(e) = std::fs::remove_file(status_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!(
                    "oo: could not remove learn status {}: {e}",
                    status_path.display()
                );
            }
        }
    }
}

#[cfg(test)]
#[path = "learn_status_tests.rs"]
mod tests;
