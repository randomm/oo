use std::process::Command;

use crate::error::Error;

pub struct CommandOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
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
