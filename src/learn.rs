use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::Error;
pub use crate::learn_prompt::SYSTEM_PROMPT;

// ---------------------------------------------------------------------------
// Config & Validation limits
// ---------------------------------------------------------------------------

/// Maximum allowed length for hint text to prevent payload bloat.
const MAX_HINT_LENGTH: usize = 1000;

/// Maximum allowed length for command text used in LLM prompt.
const MAX_COMMAND_LENGTH: usize = 100;

/// Maximum allowed length for a single filename component after sanitization.
const MAX_FILENAME_COMPONENT: usize = 50;

#[derive(Deserialize)]
struct ConfigFile {
    learn: Option<LearnConfig>,
}

/// Configuration for the `oo learn` LLM integration.
///
/// Specifies the LLM provider, model, and environment variable for the API key.
#[derive(Deserialize, Clone)]
pub struct LearnConfig {
    /// LLM provider name (currently only "anthropic" is supported).
    pub provider: String,

    /// Model identifier (e.g., "claude-haiku-4-5").
    pub model: String,

    /// Environment variable containing the API key.
    pub api_key_env: String,
}

/// Testable variant of learn config and paths — avoids env var mutation.
pub(crate) struct LearnParams<'a> {
    pub config: &'a LearnConfig,
    pub api_key: &'a str,
    pub base_url: &'a str,
    pub patterns_dir: &'a Path,
    pub learn_status_path: &'a Path,
    pub hint: Option<&'a str>,
}

/// Typed struct for the JSON data passed to the background learn process.
#[derive(Deserialize)]
struct LearnData {
    command: String,
    output: String,
    exit_code: i64,
    hint: Option<String>,
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

/// Get the directory containing user-defined patterns.
///
/// Returns `~/.config/oo/patterns` or the overridden `OO_CONFIG_DIR/patterns`.
pub fn patterns_dir() -> PathBuf {
    config_dir().join("patterns")
}

/// Path to the one-line status file written by the background learn process.
pub fn learn_status_path() -> PathBuf {
    config_dir().join("learn-status.log")
}

/// Load learn configuration from `~/.config/oo/config.toml`.
///
/// Returns the default configuration if the file doesn't exist.
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
    let hint = match params.hint {
        Some(h) if h.len() > MAX_HINT_LENGTH => {
            return Err(Error::Learn(format!(
                "--hint too long ({} > {} chars)",
                h.len(),
                MAX_HINT_LENGTH
            )));
        }
        h => h,
    };

    let truncated_command = crate::learn_utils::truncate_utf8(command, MAX_COMMAND_LENGTH);

    let user_msg = if let Some(h) = hint {
        format!(
            "Command: {truncated_command}\nExit code: {exit_code}\nHint: {h}\nOutput:\n{}",
            truncate_for_prompt(output)
        )
    } else {
        format!(
            "Command: {truncated_command}\nExit code: {exit_code}\nOutput:\n{}",
            truncate_for_prompt(output)
        )
    };

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
    let clean = crate::learn_utils::strip_fences(&toml);
    if validate_pattern_toml_with_limits(&clean).is_ok() {
        std::fs::create_dir_all(params.patterns_dir).map_err(|e| Error::Learn(e.to_string()))?;
        let filename = format!("{}.toml", label(command));
        let path = params.patterns_dir.join(&filename);
        std::fs::write(&path, &clean).map_err(|e| Error::Learn(e.to_string()))?;
        let _ =
            crate::commands::write_learn_status(params.learn_status_path, &label(command), &path);
        return Ok(());
    }
    last_err = "initial TOML validation failed".to_string();

    // Up to 2 retries
    for _ in 0..2 {
        let retry_msg = format!(
            "Your previous TOML was invalid: {last_err}. Here is what you returned:\n{clean}\nOutput ONLY the corrected TOML, nothing else."
        );
        let toml = get_response(&retry_msg)?;
        let clean = crate::learn_utils::strip_fences(&toml);
        if validate_pattern_toml_with_limits(&clean).is_ok() {
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
        last_err = "retry TOML validation failed".to_string();
    }

    Err(Error::Learn(format!("failed after 3 attempts: {last_err}")))
}

/// Run the learn flow: call LLM, validate + save pattern.
///
/// Loads configuration from environment, calls the LLM to generate a pattern,
/// validates the result, and saves the pattern to disk. Retries up to 2 times
/// if the LLM returns invalid TOML.
pub fn run_learn(command: &str, output: &str, exit_code: i32) -> Result<(), Error> {
    run_learn_with_hint(command, output, exit_code, None)
}

/// Internal variant of run_learn that accepts an optional hint.
///
/// Used by run_background to pass through agent-provided hints.
fn run_learn_with_hint(
    command: &str,
    output: &str,
    exit_code: i32,
    hint: Option<&str>,
) -> Result<(), Error> {
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
        hint,
    };

    run_learn_with_config(&params, command, output, exit_code)
}

/// Spawn the learning process in the background by re-exec'ing ourselves.
pub fn spawn_background(
    command: &str,
    output: &str,
    exit_code: i32,
    hint: Option<&str>,
) -> Result<(), Error> {
    let exe = std::env::current_exe().map_err(|e| Error::Learn(e.to_string()))?;

    // Use a secure named temp file to avoid PID-based predictable filenames
    // (symlink/TOCTOU attacks). The file is kept alive until the child spawns.
    let mut tmp = tempfile::NamedTempFile::new().map_err(|e| Error::Learn(e.to_string()))?;
    let mut data = serde_json::json!({
        "command": command,
        "output": output,
        "exit_code": exit_code,
    });
    if let Some(h) = hint {
        data["hint"] = serde_json::Value::String(h.to_string());
    }
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
    let data: LearnData =
        serde_json::from_str(&content).map_err(|e| Error::Learn(e.to_string()))?;

    let command = &data.command;
    let output = &data.output;

    // Explicit bounds check to avoid silent truncation i64→i32
    let exit_code = i32::try_from(data.exit_code).map_err(|_| {
        Error::Learn(format!(
            "exit_code out of range for i32: {}",
            data.exit_code
        ))
    })?;

    let hint = data.hint.as_deref();

    let result = run_learn_with_hint(command, output, exit_code, hint);

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

    use std::time::Duration;

    // Configure Agent with explicit timeout to prevent hanging on API calls
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .timeout_connect(Some(Duration::from_secs(10)))
        .build()
        .into();

    let response: serde_json::Value = agent
        .post(base_url)
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

    // Sanitize first word: keep only ASCII alphanumeric and hyphens,
    // prevent path traversal, dotfiles, special chars, overlong filenames.
    let sanitized_first: String = first
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(MAX_FILENAME_COMPONENT)
        .collect();

    if sanitized_first.is_empty() {
        return "unknown".to_string();
    }

    // Include the second word only when it is a subcommand (not a flag).
    match words.next() {
        Some(second) if !second.starts_with('-') => {
            // Sanitize: keep only ASCII alphanumeric and hyphens to ensure
            // the label is safe as a filename component.
            let sanitized_second: String = second
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
                .take(MAX_FILENAME_COMPONENT)
                .collect();
            if sanitized_second.is_empty() {
                sanitized_first
            } else {
                format!("{sanitized_first}-{sanitized_second}")
            }
        }
        _ => sanitized_first,
    }
}

fn truncate_for_prompt(output: &str) -> &str {
    crate::learn_utils::truncate_utf8(output, 4000)
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

/// Validate a TOML pattern string using the same regex limits as TOML loading.
///
/// Uses `validate_pattern_regexes` from pattern::toml module for consistency.
fn validate_pattern_toml_with_limits(toml_str: &str) -> Result<(), Error> {
    crate::pattern::validate_pattern_regexes(toml_str)
        .map_err(|e| Error::Learn(format!("pattern validation: {e}")))
}

// Tests live in separate files to keep this module under 500 lines.
#[cfg(test)]
#[path = "learn_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "learn_prompt_tests.rs"]
mod prompt_tests;
