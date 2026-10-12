use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::config::config_dir;
use crate::error::Error;
use crate::learn_payload::{is_owned_payload, payload_dir, remove_payload};
pub use crate::learn_prompt::SYSTEM_PROMPT;
/// Re-exported so existing `learn::learn_status_path` callers keep compiling.
pub use crate::learn_status::learn_status_path;
use crate::learn_url::ApiUrl;

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

/// Get the directory containing user-defined patterns.
///
/// Returns `<config dir>/oo/patterns` (`~/.config/oo/patterns` on Linux,
/// `~/Library/Application Support/oo/patterns` on macOS), or `OO_CONFIG_DIR/patterns`
/// when `OO_CONFIG_DIR` is set.
pub fn patterns_dir() -> PathBuf {
    config_dir().join("patterns")
}

/// Load learn configuration from `config.toml` in the config directory
/// (see `patterns_dir` for the platform paths).
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

    let api_url = ApiUrl::parse(params.base_url)?;
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
            "anthropic" => call_anthropic(&api_url, params.api_key, &params.config.model, msg),
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
        let _ = crate::learn_status::write_learn_status(
            params.learn_status_path,
            &label(command),
            &path,
        );
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
            let _ = crate::learn_status::write_learn_status(
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

    let params = LearnParams {
        config: &config,
        api_key: &api_key,
        base_url: &base_url,
        patterns_dir: &patterns_dir(),
        learn_status_path: &crate::learn_status::learn_status_path(),
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
    let dir = payload_dir();
    let mut tmp = tempfile::Builder::new()
        .tempfile_in(&dir)
        .map_err(|e| Error::Learn(e.to_string()))?;
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
        .arg(&dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| Error::Learn(e.to_string()))?;

    // Prevent the parent from deleting the temp file on drop. On a loaded
    // system the child process may not have opened the file yet by the time
    // the parent exits this function. `keep()` makes the file persist on disk
    // until the child cleans it up in `run_background` (below).
    tmp_path.keep().map_err(|e| Error::Learn(e.to_string()))?;

    Ok(())
}

/// Entry point for the background learn child process.
///
/// `payload_dir` is the directory the parent created the payload in; ownership
/// is checked against it rather than against this process's environment.
pub fn run_background(data_path: &str, payload_dir: &Path) -> Result<(), Error> {
    let path = Path::new(data_path);
    // The payload can hold command output (possibly secret); remove it as soon
    // as it has been read, before any parse or learn step can fail.
    let read = std::fs::read_to_string(path);
    let removal = if is_owned_payload(path, payload_dir) {
        remove_payload(path).err()
    } else {
        None
    };

    // Every failure below reaches the status log, labelled with the command once
    // it is known; "learn" covers a payload that could not be parsed.
    let mut cmd_label = String::from("learn");
    let result = read
        .map_err(|e| Error::Learn(e.to_string()))
        .and_then(|content| {
            let data: LearnData =
                serde_json::from_str(&content).map_err(|e| Error::Learn(e.to_string()))?;
            cmd_label = label(&data.command);
            // Explicit bounds check to avoid silent truncation i64→i32
            let exit_code = i32::try_from(data.exit_code).map_err(|_| {
                Error::Learn(format!(
                    "exit_code out of range for i32: {}",
                    data.exit_code
                ))
            })?;
            run_learn_with_hint(&data.command, &data.output, exit_code, data.hint.as_deref())
        });

    let result = match removal {
        None => result,
        Some(e) => {
            let msg = format!("could not remove payload {}: {e}", path.display());
            Err(Error::Learn(match result {
                Ok(()) => msg,
                Err(learn_err) => format!("{msg}; {learn_err}"),
            }))
        }
    };

    if let Err(ref e) = result {
        let status_path = crate::learn_status::learn_status_path();
        if crate::learn_status::write_learn_status_failure(&status_path, &cmd_label, &e.to_string())
            .is_err()
        {
            // The detached child's stderr is null, so this only helps when run in the foreground.
            eprintln!("oo: could not record learn failure for {cmd_label}: {e}");
        }
    }

    result
}

// ---------------------------------------------------------------------------
// LLM API calls
// ---------------------------------------------------------------------------

fn call_anthropic(
    base_url: &ApiUrl,
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

    // Configure Agent with explicit timeout to prevent hanging on API calls.
    // Redirects are refused: ureq would replay x-api-key to the Location target.
    // max_redirects(0) makes ureq return a 3xx as an ordinary response, so the status check below refuses it.
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .timeout_connect(Some(Duration::from_secs(10)))
        .max_redirects(0)
        .build()
        .into();

    let mut response = agent
        .post(base_url.uri().clone())
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .send_json(&body)
        .map_err(|e| Error::Learn(format!("Anthropic API error: {e}")))?;

    let status = response.status().as_u16();
    if (300..400).contains(&status) {
        return Err(Error::Learn(format!(
            "unexpected redirect (status {status}): redirects are not followed; ANTHROPIC_API_URL must point directly at the API endpoint"
        )));
    }

    let response: serde_json::Value = response
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
