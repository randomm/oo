use std::process::Command;

use crate::error::Error;

/// Output from executing a shell command.
///
/// Captures the standard output, standard error, and exit status of a command
/// executed via `run()`.
pub struct CommandOutput {
    /// Standard output as raw bytes.
    pub stdout: Vec<u8>,

    /// Standard error as raw bytes.
    pub stderr: Vec<u8>,

    /// Process exit code (0 indicates success).
    pub exit_code: i32,
}

impl CommandOutput {
    /// Merged output: stdout followed by stderr.
    pub fn merged(&self) -> Vec<u8> {
        let mut out = self.stdout.clone();
        out.extend_from_slice(&self.stderr);
        out
    }

    /// Merged output as a lossy UTF-8 string.
    pub fn merged_lossy(&self) -> String {
        String::from_utf8_lossy(&self.merged()).into_owned()
    }
}

/// Execute a shell command and capture its output.
///
/// Runs the first argument as the program name with remaining arguments as parameters.
/// Captures stdout, stderr, and exit status.
///
/// # Errors
///
/// Returns an error if the command cannot be spawned or if there's an I/O error during execution.
pub fn run(args: &[String]) -> Result<CommandOutput, Error> {
    let output = Command::new(&args[0]).args(&args[1..]).output()?;

    let exit_code = output.status.code().unwrap_or(128);

    Ok(CommandOutput {
        stdout: output.stdout,
        stderr: output.stderr,
        exit_code,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_successful_command() {
        let result = run(&["echo".into(), "hello".into()]).unwrap();
        assert_eq!(result.exit_code, 0);
        assert_eq!(String::from_utf8_lossy(&result.stdout), "hello\n");
    }

    #[test]
    fn test_failing_command() {
        let result = run(&["false".into()]).unwrap();
        assert_ne!(result.exit_code, 0);
    }

    #[test]
    fn test_stderr_captured() {
        let result = run(&["sh".into(), "-c".into(), "echo err >&2".into()]).unwrap();
        assert_eq!(result.exit_code, 0);
        assert_eq!(String::from_utf8_lossy(&result.stderr), "err\n");
    }

    #[test]
    fn test_exit_code_preserved() {
        let result = run(&["sh".into(), "-c".into(), "exit 42".into()]).unwrap();
        assert_eq!(result.exit_code, 42);
    }

    #[test]
    fn test_merged_output() {
        let result = run(&["sh".into(), "-c".into(), "echo out; echo err >&2".into()]).unwrap();
        let merged = result.merged_lossy();
        assert!(merged.contains("out"));
        assert!(merged.contains("err"));
    }
}
