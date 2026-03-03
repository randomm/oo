use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::Error;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ConfigFile {
    learn: Option<LearnConfig>,
}

#[derive(Deserialize, Clone)]
pub struct LearnConfig {
    pub provider: String,
    pub model: String,
    pub api_key_env: String,
}

impl Default for LearnConfig {
    fn default() -> Self {
        Self {
            provider: "anthropic".into(),
            model: "claude-haiku-4-5-20251001".into(),
            api_key_env: "ANTHROPIC_API_KEY".into(),
        }
    }
}

fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("oo")
}

pub fn patterns_dir() -> PathBuf {
    config_dir().join("patterns")
}

pub fn load_learn_config() -> Result<LearnConfig, Error> {
    let path = config_dir().join("config.toml");
    if !path.exists() {
        return Ok(LearnConfig::default());
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| Error::Config(format!("{}: {e}", path.display())))?;
    let cf: ConfigFile =
        toml::from_str(&content).map_err(|e| Error::Config(format!("{}: {e}", path.display())))?;
    Ok(cf.learn.unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Background learning
// ---------------------------------------------------------------------------

const SYSTEM_PROMPT: &str = r#"You generate tool output classification patterns for a command runner.

Given a shell command, its stdout, stderr, and exit code, produce a TOML
pattern file that captures:

1. A regex to match this command (command_match)
2. For success (exit 0): a regex with named capture groups to extract a
   one-line summary, and a summary template using those groups
3. For failure (exit ≠ 0): a strategy to extract the actionable part of
   the output (tail N lines, head N lines, grep for pattern, or extract
   between markers)

Be aggressive about compression. A 1000-line passing test suite should
become "47 passed, 3.2s". A failing build should show only the first
error and its context, not the full cascade.

Respond with ONLY the TOML block. No explanation, no markdown fences."#;

/// Run the learn flow: call LLM, validate + save pattern.
pub fn run_learn(command: &str, output: &str, exit_code: i32) -> Result<(), Error> {
    let config = load_learn_config()?;

    let api_key = std::env::var(&config.api_key_env).map_err(|_| {
        Error::Learn(format!(
            "Set {} environment variable to use `oo learn`",
            config.api_key_env
        ))
    })?;

    let user_msg = format!(
        "Command: {command}\nExit code: {exit_code}\nOutput:\n{}",
        truncate_for_prompt(output)
    );

    eprintln!("  [learning pattern for \"{}\"]", label(command));

    let toml_response = match config.provider.as_str() {
        "anthropic" => call_anthropic(&api_key, &config.model, &user_msg)?,
        "openai" => call_openai(&api_key, &config.model, &user_msg)?,
        other => return Err(Error::Learn(format!("unknown provider: {other}"))),
    };

    // Strip markdown fences if present
    let toml_clean = strip_fences(&toml_response);

    // Validate: parse as pattern
    validate_pattern_toml(&toml_clean)?;

    // Save
    let dir = patterns_dir();
    std::fs::create_dir_all(&dir).map_err(|e| Error::Learn(e.to_string()))?;
    let filename = format!("{}.toml", label(command));
    let path = dir.join(&filename);
    std::fs::write(&path, &toml_clean).map_err(|e| Error::Learn(e.to_string()))?;

    eprintln!("  [saved pattern to {}]", path.display());
    Ok(())
}

/// Spawn the learning process in the background by re-exec'ing ourselves.
pub fn spawn_background(command: &str, output: &str, exit_code: i32) -> Result<(), Error> {
    let exe = std::env::current_exe().map_err(|e| Error::Learn(e.to_string()))?;

    // Write data to a temp file for the child to read
    let tmp = std::env::temp_dir().join(format!("oo-learn-{}", std::process::id()));
    let data = serde_json::json!({
        "command": command,
        "output": output,
        "exit_code": exit_code,
    });
    std::fs::write(&tmp, data.to_string()).map_err(|e| Error::Learn(e.to_string()))?;

    // Spawn detached child
    std::process::Command::new(exe)
        .arg("_learn_bg")
        .arg(&tmp)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| Error::Learn(e.to_string()))?;

    Ok(())
}

/// Entry point for the background learn child process.
pub fn run_background(data_path: &str) -> Result<(), Error> {
    let path = Path::new(data_path);
    let content = std::fs::read_to_string(path).map_err(|e| Error::Learn(e.to_string()))?;
    let data: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| Error::Learn(e.to_string()))?;

    let command = data["command"].as_str().unwrap_or("");
    let output = data["output"].as_str().unwrap_or("");
    let exit_code = data["exit_code"].as_i64().unwrap_or(0) as i32;

    let result = run_learn(command, output, exit_code);

    // Clean up temp file
    let _ = std::fs::remove_file(path);

    result
}

// ---------------------------------------------------------------------------
// LLM API calls
// ---------------------------------------------------------------------------

fn call_anthropic(api_key: &str, model: &str, user_msg: &str) -> Result<String, Error> {
    let body = serde_json::json!({
        "model": model,
        "max_tokens": 1024,
        "system": SYSTEM_PROMPT,
        "messages": [{"role": "user", "content": user_msg}],
    });

    let response: serde_json::Value = ureq::post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .send_json(&body)
        .map_err(|e| Error::Learn(format!("Anthropic API error: {e}")))?
        .body_mut()
        .read_json()
        .map_err(|e| Error::Learn(format!("response parse error: {e}")))?;

    response["content"][0]["text"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| Error::Learn("unexpected Anthropic response format".into()))
}

fn call_openai(api_key: &str, model: &str, user_msg: &str) -> Result<String, Error> {
    let body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": user_msg},
        ],
    });

    let response: serde_json::Value = ureq::post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", &format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .send_json(&body)
        .map_err(|e| Error::Learn(format!("OpenAI API error: {e}")))?
        .body_mut()
        .read_json()
        .map_err(|e| Error::Learn(format!("response parse error: {e}")))?;

    response["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| Error::Learn("unexpected OpenAI response format".into()))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn label(command: &str) -> String {
    command
        .split_whitespace()
        .next()
        .unwrap_or("unknown")
        .rsplit('/')
        .next()
        .unwrap_or("unknown")
        .to_string()
}

fn truncate_for_prompt(output: &str) -> &str {
    if output.len() > 4000 {
        &output[..4000]
    } else {
        output
    }
}

fn strip_fences(s: &str) -> String {
    let trimmed = s.trim();
    if let Some(rest) = trimmed.strip_prefix("```toml") {
        rest.strip_suffix("```").unwrap_or(rest).trim().to_string()
    } else if let Some(rest) = trimmed.strip_prefix("```") {
        rest.strip_suffix("```").unwrap_or(rest).trim().to_string()
    } else {
        trimmed.to_string()
    }
}

fn validate_pattern_toml(toml_str: &str) -> Result<(), Error> {
    // Try to parse as our pattern format
    #[derive(Deserialize)]
    struct Check {
        command_match: String,
        #[allow(dead_code)]
        success: Option<SuccessCheck>,
        #[allow(dead_code)]
        failure: Option<serde_json::Value>,
    }
    #[derive(Deserialize)]
    struct SuccessCheck {
        pattern: String,
        #[allow(dead_code)]
        summary: String,
    }

    let check: Check =
        toml::from_str(toml_str).map_err(|e| Error::Learn(format!("invalid TOML: {e}")))?;

    // Verify regexes compile
    regex::Regex::new(&check.command_match)
        .map_err(|e| Error::Learn(format!("invalid command_match regex: {e}")))?;

    if let Some(s) = &check.success {
        regex::Regex::new(&s.pattern)
            .map_err(|e| Error::Learn(format!("invalid success pattern regex: {e}")))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_fences_toml() {
        let input = "```toml\ncommand_match = \"test\"\n```";
        assert_eq!(strip_fences(input), "command_match = \"test\"");
    }

    #[test]
    fn test_strip_fences_plain() {
        let input = "```\ncommand_match = \"test\"\n```";
        assert_eq!(strip_fences(input), "command_match = \"test\"");
    }

    #[test]
    fn test_strip_fences_none() {
        let input = "command_match = \"test\"";
        assert_eq!(strip_fences(input), "command_match = \"test\"");
    }

    #[test]
    fn test_validate_valid_toml() {
        let toml = r#"
command_match = "^mytest"
[success]
pattern = '(?P<n>\d+) passed'
summary = "{n} passed"
"#;
        assert!(validate_pattern_toml(toml).is_ok());
    }

    #[test]
    fn test_validate_invalid_regex() {
        let toml = r#"
command_match = "[invalid"
"#;
        assert!(validate_pattern_toml(toml).is_err());
    }

    #[test]
    fn test_truncate_for_prompt() {
        let short = "hello";
        assert_eq!(truncate_for_prompt(short), "hello");

        let long = "x".repeat(5000);
        assert_eq!(truncate_for_prompt(&long).len(), 4000);
    }

    #[test]
    fn test_label_extraction() {
        assert_eq!(label("pytest -x"), "pytest");
        assert_eq!(label("/usr/bin/cargo test"), "cargo");
    }

    #[test]
    fn test_default_config() {
        let config = LearnConfig::default();
        assert_eq!(config.provider, "anthropic");
    }

    #[test]
    fn test_default_config_model() {
        let config = LearnConfig::default();
        assert!(!config.model.is_empty(), "model must not be empty");
    }

    #[test]
    fn test_default_config_api_key_env() {
        let config = LearnConfig::default();
        assert_eq!(config.api_key_env, "ANTHROPIC_API_KEY");
    }

    #[test]
    fn test_validate_valid_toml_no_success() {
        // A minimal valid TOML with only command_match
        let toml = r#"command_match = "^cargo""#;
        assert!(validate_pattern_toml(toml).is_ok());
    }

    #[test]
    fn test_validate_invalid_toml_syntax() {
        // Malformed TOML should return an error
        let toml = "this is not valid = [toml";
        assert!(validate_pattern_toml(toml).is_err());
    }

    #[test]
    fn test_validate_missing_command_match() {
        // TOML without required command_match field should fail
        let toml = r#"
[success]
pattern = "ok"
summary = "done"
"#;
        assert!(validate_pattern_toml(toml).is_err());
    }

    #[test]
    fn test_validate_invalid_command_match_regex() {
        // Invalid regex in command_match should return error
        let toml = r#"command_match = "[invalid_regex""#;
        assert!(validate_pattern_toml(toml).is_err());
    }

    #[test]
    fn test_validate_invalid_success_pattern_regex() {
        // Invalid regex in success.pattern should return error
        let toml = r#"
command_match = "^cargo"
[success]
pattern = "[invalid"
summary = "done"
"#;
        assert!(validate_pattern_toml(toml).is_err());
    }

    #[test]
    fn test_strip_fences_whitespace_preserved() {
        // Content inside fences is trimmed but inner structure is kept
        let input = "```toml\n\ncommand_match = \"test\"\n\n```";
        let result = strip_fences(input);
        assert!(
            result.contains("command_match"),
            "content must be preserved"
        );
    }

    #[test]
    fn test_truncate_for_prompt_boundary() {
        // Exactly 4000 chars should not be truncated
        let exact = "a".repeat(4000);
        assert_eq!(truncate_for_prompt(&exact).len(), 4000);

        // 4001 chars should be truncated to 4000
        let over = "a".repeat(4001);
        assert_eq!(truncate_for_prompt(&over).len(), 4000);
    }

    #[test]
    fn test_label_path_extraction() {
        // Full paths: only the last component
        assert_eq!(label("/usr/local/bin/rustc"), "rustc");
        assert_eq!(label("./target/release/oo"), "oo");
    }

    #[test]
    fn test_label_empty_command() {
        // Edge case: empty string
        assert_eq!(label(""), "unknown");
    }
}
