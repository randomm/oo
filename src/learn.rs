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

/// Testable variant of learn config and paths — avoids env var mutation.
pub(crate) struct LearnParams<'a> {
    pub config: &'a LearnConfig,
    pub api_key: &'a str,
    pub base_url: &'a str,
    pub patterns_dir: &'a Path,
    pub learn_status_path: &'a Path,
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
            model: "claude-haiku-4-5".into(),
            api_key_env: "ANTHROPIC_API_KEY".into(),
        }
    } else {
        // Default to anthropic; will fail at runtime if no key is set.
        LearnConfig {
            provider: "anthropic".into(),
            model: "claude-haiku-4-5".into(),
            api_key_env: "ANTHROPIC_API_KEY".into(),
        }
    }
}

fn config_dir() -> PathBuf {
    if let Some(test_dir) = std::env::var_os("OO_CONFIG_DIR") {
        return PathBuf::from(test_dir);
    }
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

The agent reads your pattern to decide its next action. Returning nothing is the WORST outcome — an empty summary forces a costly recall cycle.

IMPORTANT: Use named capture groups (?P<name>...) only — never numbered groups like (\d+). Summary templates use {name} placeholders matching the named groups.

## oo's 4-tier system

- Passthrough: output <4 KB passes through unchanged
- Failure: failed commands get ✗ prefix with filtered error output
- Success: successful commands get ✓ prefix with a pattern-extracted summary
- Large: if regex fails to match, output is FTS5 indexed for recall

## Examples

Test runner — capture RESULT line, not header; strategy=tail for failures:
    command_match = "\\bcargo\\s+test\\b"
    [success]
    pattern = 'test result: ok\. (?P<passed>\d+) passed.*finished in (?P<time>[\d.]+)s'
    summary = "{passed} passed, {time}s"
    [failure]
    strategy = "tail"
    lines = 30

Build/lint — quiet on success (only useful when failing); strategy=head for failures:
    command_match = "\\bcargo\\s+build\\b"
    [success]
    pattern = "(?s).*"
    summary = ""
    [failure]
    strategy = "head"
    lines = 20

## Rules

- Test runners: capture SUMMARY line (e.g. 'test result: ok. 5 passed'), NOT headers (e.g. 'running 5 tests')
- Build/lint tools: empty summary for success; head/lines=20 for failures
- Large tabular output (ls, git log): omit success section — falls to Large tier

## Command Categories

Note: oo categorizes commands (Status: tests/builds/lints, Content: git show/diff/cat, Data: git log/ls/gh, Unknown: others). Patterns are most valuable for Status commands. Content commands always pass through regardless of size; Data commands are indexed when large and unpatterned."#;

/// Run the learn flow with explicit config and base URL — testable variant.
///
/// This internal function bypasses `load_learn_config()` and env var lookup,
/// making it suitable for testing without environment mutation.
pub(crate) fn run_learn_with_config(
    params: &LearnParams,
    command: &str,
    output: &str,
    exit_code: i32,
) -> Result<(), Error> {
    let user_msg = format!(
        "Command: {command}\nExit code: {exit_code}\nOutput:\n{}",
        truncate_for_prompt(output)
    );

    let get_response = |msg: &str| -> Result<String, Error> {
        match params.config.provider.as_str() {
            "anthropic" => {
                call_anthropic(params.base_url, params.api_key, &params.config.model, msg)
            }
            other => Err(Error::Learn(format!("unknown provider: {other}"))),
        }
    };

    // First attempt
    let mut last_err;
    let toml = get_response(&user_msg)?;
    let clean = strip_fences(&toml);
    match validate_pattern_toml(&clean) {
        Ok(()) => {
            std::fs::create_dir_all(params.patterns_dir)
                .map_err(|e| Error::Learn(e.to_string()))?;
            let filename = format!("{}.toml", label(command));
            let path = params.patterns_dir.join(&filename);
            std::fs::write(&path, &clean).map_err(|e| Error::Learn(e.to_string()))?;
            let _ = crate::commands::write_learn_status(
                params.learn_status_path,
                &label(command),
                &path,
            );
            return Ok(());
        }
        Err(e) => last_err = e,
    }

    // Up to 2 retries
    for _ in 0..2 {
        let retry_msg = format!(
            "Your previous TOML was invalid: {last_err}. Here is what you returned:\n{clean}\nOutput ONLY the corrected TOML, nothing else."
        );
        let toml = get_response(&retry_msg)?;
        let clean = strip_fences(&toml);
        match validate_pattern_toml(&clean) {
            Ok(()) => {
                std::fs::create_dir_all(params.patterns_dir)
                    .map_err(|e| Error::Learn(e.to_string()))?;
                let filename = format!("{}.toml", label(command));
                let path = params.patterns_dir.join(&filename);
                std::fs::write(&path, &clean).map_err(|e| Error::Learn(e.to_string()))?;
                let _ = crate::commands::write_learn_status(
                    params.learn_status_path,
                    &label(command),
                    &path,
                );
                return Ok(());
            }
            Err(e) => last_err = e,
        }
    }

    Err(Error::Learn(format!("failed after 3 attempts: {last_err}")))
}

/// Run the learn flow: call LLM, validate + save pattern.
pub fn run_learn(command: &str, output: &str, exit_code: i32) -> Result<(), Error> {
    let config = load_learn_config()?;

    let api_key = std::env::var(&config.api_key_env).map_err(|_| {
        Error::Learn(format!(
            "Set {} environment variable to use `oo learn`",
            config.api_key_env
        ))
    })?;

    let base_url = std::env::var("ANTHROPIC_API_URL")
        .unwrap_or_else(|_| "https://api.anthropic.com/v1/messages".to_string());

    validate_anthropic_url(&base_url)?;

    let params = LearnParams {
        config: &config,
        api_key: &api_key,
        base_url: &base_url,
        patterns_dir: &patterns_dir(),
        learn_status_path: &learn_status_path(),
    };

    run_learn_with_config(&params, command, output, exit_code)
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

    if let Err(ref e) = result {
        let cmd_label = label(command);
        let status_path = learn_status_path();
        let _ =
            crate::commands::write_learn_status_failure(&status_path, &cmd_label, &e.to_string());
    }

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
        "temperature": 0.0,
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn label(command: &str) -> String {
    let mut words = command.split_whitespace();
    let first = words
        .next()
        .unwrap_or("unknown")
        .rsplit('/')
        .next()
        .unwrap_or("unknown");
    // Include the second word only when it is a subcommand (not a flag).
    match words.next() {
        Some(second) if !second.starts_with('-') => {
            // Sanitize: keep only ASCII alphanumeric and hyphens to ensure
            // the label is safe as a filename component.
            let sanitized: String = second
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect();
            if sanitized.is_empty() {
                first.to_string()
            } else {
                format!("{first}-{sanitized}")
            }
        }
        _ => first.to_string(),
    }
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

/// Validate ANTHROPIC_API_URL uses HTTPS (with localhost exceptions).
fn validate_anthropic_url(url: &str) -> Result<(), Error> {
    if url.starts_with("https://") {
        return Ok(());
    }
    // HTTP only allowed for localhost/127.0.0.1
    // Extract host portion: "http://HOST:port/path" or "http://HOST/path"
    if let Some(rest) = url.strip_prefix("http://") {
        let host = rest.split([':', '/']).next().unwrap_or("");
        if host == "localhost" || host == "127.0.0.1" {
            return Ok(());
        }
    }
    Err(Error::Learn(format!(
        "ANTHROPIC_API_URL must use HTTPS (got: {url}). HTTP is only allowed for localhost/127.0.0.1."
    )))
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

// Tests live in separate files to keep this module under 500 lines.
#[cfg(test)]
#[path = "learn_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "learn_prompt_tests.rs"]
mod prompt_tests;
