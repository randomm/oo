use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::Error;
use crate::pattern::FailureSection;

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
        detect_provider()
    }
}

// Auto-detect provider from available API keys (checked in priority order).
fn detect_provider() -> LearnConfig {
    detect_provider_with(|key| std::env::var(key).ok())
}

// Testable variant — accepts a closure for env lookup to avoid env mutation in tests.
fn detect_provider_with<F: Fn(&str) -> Option<String>>(env_lookup: F) -> LearnConfig {
    if env_lookup("ANTHROPIC_API_KEY").is_some() {
        LearnConfig {
            provider: "anthropic".into(),
            model: "claude-haiku-4-5-20251001".into(),
            api_key_env: "ANTHROPIC_API_KEY".into(),
        }
    } else if env_lookup("OPENAI_API_KEY").is_some() {
        LearnConfig {
            provider: "openai".into(),
            model: "gpt-4o-mini".into(),
            api_key_env: "OPENAI_API_KEY".into(),
        }
    } else if env_lookup("CEREBRAS_API_KEY").is_some() {
        LearnConfig {
            provider: "cerebras".into(),
            // Cerebras model ID: check https://cloud.cerebras.ai/models for current model catalog
            model: "zai-glm-4.7".into(),
            api_key_env: "CEREBRAS_API_KEY".into(),
        }
    } else {
        // Default to anthropic; will fail at runtime if no key is set.
        LearnConfig {
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

/// Path to the one-line status file written by the background learn process.
pub fn learn_status_path() -> PathBuf {
    config_dir().join("learn-status.log")
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

const SYSTEM_PROMPT: &str = r#"You generate output classification patterns for `oo`, a shell command runner used by an LLM coding agent.

The agent reads your pattern to decide its next action. Returning nothing is the WORST outcome — an empty summary forces a costly recall cycle that wastes more tokens than a slightly verbose summary would.

## oo's 4-tier system

- Passthrough: output <4 KB passes through unchanged
- Failure: failed commands get ✗ prefix with filtered error output
- Success: successful commands get ✓ prefix with a pattern-extracted summary (your patterns target this tier)
- Large: if your regex fails to match, output falls through to this tier (FTS5 indexed for recall) — not catastrophic

## Output format

Respond with ONLY a TOML block. Fences optional.

    command_match = "^pytest"
    [success]
    pattern = '(?P<n>\d+) passed'
    summary = "{n} passed"
    [failure]
    strategy = "grep"
    grep = "error|Error|FAILED"

## Rules

- For build/test commands: compress aggressively (e.g. "47 passed, 3.2s" or "error: …first error only")
- For large tabular output (ls, docker ps, git log): omit the success section — let it fall through to Large tier (FTS5 indexed)
- A regex that's too broad is better than one that matches and returns empty"#;

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

    let toml_response = match config.provider.as_str() {
        "anthropic" => call_anthropic(
            "https://api.anthropic.com/v1/messages",
            &api_key,
            &config.model,
            &user_msg,
        )?,
        "openai" => call_openai(
            "https://api.openai.com/v1/chat/completions",
            &api_key,
            &config.model,
            &user_msg,
        )?,
        "cerebras" => call_openai(
            "https://api.cerebras.ai/v1/chat/completions",
            &api_key,
            &config.model,
            &user_msg,
        )?,
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

    // Write status file for the foreground process to display on next invocation
    let status_path = learn_status_path();
    let cmd_label = label(command);
    let _ = crate::commands::write_learn_status(&status_path, &cmd_label, &path);

    Ok(())
}

/// Spawn the learning process in the background by re-exec'ing ourselves.
pub fn spawn_background(command: &str, output: &str, exit_code: i32) -> Result<(), Error> {
    let exe = std::env::current_exe().map_err(|e| Error::Learn(e.to_string()))?;

    // Use a secure named temp file to avoid PID-based predictable filenames
    // (symlink/TOCTOU attacks). The file is kept alive until the child spawns.
    let mut tmp = tempfile::NamedTempFile::new().map_err(|e| Error::Learn(e.to_string()))?;
    let data = serde_json::json!({
        "command": command,
        "output": output,
        "exit_code": exit_code,
    });
    tmp.write_all(data.to_string().as_bytes())
        .map_err(|e| Error::Learn(e.to_string()))?;

    // Convert to TempPath: closes the file handle but keeps the file on disk
    // until the TempPath is dropped — after the child has been spawned.
    let tmp_path = tmp.into_temp_path();

    // Spawn detached child
    std::process::Command::new(exe)
        .arg("_learn_bg")
        .arg(&tmp_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| Error::Learn(e.to_string()))?;

    // Prevent the parent from deleting the temp file on drop. On a loaded
    // system the child process may not have opened the file yet by the time
    // the parent exits this function. `keep()` makes the file persist on disk
    // until the child cleans it up at run_background (line ~218 below).
    tmp_path.keep().map_err(|e| Error::Learn(e.to_string()))?;

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

fn call_anthropic(
    base_url: &str,
    api_key: &str,
    model: &str,
    user_msg: &str,
) -> Result<String, Error> {
    let body = serde_json::json!({
        "model": model,
        "max_tokens": 1024,
        "system": SYSTEM_PROMPT,
        "messages": [{"role": "user", "content": user_msg}],
    });

    let response: serde_json::Value = ureq::post(base_url)
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

fn call_openai(
    base_url: &str,
    api_key: &str,
    model: &str,
    user_msg: &str,
) -> Result<String, Error> {
    let body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": user_msg},
        ],
    });

    let response: serde_json::Value = ureq::post(base_url)
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
    truncate_utf8(output, 4000)
}

// Truncate at a char boundary to avoid panics on multibyte UTF-8 sequences.
fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
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
        // Deserialization target: field must exist for TOML parsing even if not read in code
        #[allow(dead_code)] // used only for TOML deserialization validation
        success: Option<SuccessCheck>,
        failure: Option<FailureSection>,
    }
    #[derive(Deserialize)]
    struct SuccessCheck {
        pattern: String,
        // Deserialization target: field must exist for TOML parsing even if not read in code
        #[allow(dead_code)] // used only for TOML deserialization validation
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

    if let Some(f) = &check.failure {
        match f.strategy.as_deref().unwrap_or("tail") {
            "grep" => {
                let pat = f.grep_pattern.as_deref().ok_or_else(|| {
                    Error::Learn("failure grep strategy requires a 'grep' field".into())
                })?;
                if pat.is_empty() {
                    return Err(Error::Learn("failure grep regex must not be empty".into()));
                }
                regex::Regex::new(pat)
                    .map_err(|e| Error::Learn(format!("invalid failure grep regex: {e}")))?;
            }
            "between" => {
                let start = f.start.as_deref().ok_or_else(|| {
                    Error::Learn("between strategy requires 'start' field".into())
                })?;
                if start.is_empty() {
                    return Err(Error::Learn("between 'start' must not be empty".into()));
                }
                regex::Regex::new(start)
                    .map_err(|e| Error::Learn(format!("invalid start regex: {e}")))?;
                let end = f
                    .end
                    .as_deref()
                    .ok_or_else(|| Error::Learn("between strategy requires 'end' field".into()))?;
                if end.is_empty() {
                    return Err(Error::Learn("between 'end' must not be empty".into()));
                }
                regex::Regex::new(end)
                    .map_err(|e| Error::Learn(format!("invalid end regex: {e}")))?;
            }
            "tail" | "head" => {} // no regex to validate
            other => {
                return Err(Error::Learn(format!("unknown failure strategy: {other}")));
            }
        }
    }

    Ok(())
}

// Tests live in a separate file to keep this module under 500 lines.
#[cfg(test)]
#[path = "learn_tests.rs"]
mod tests;
