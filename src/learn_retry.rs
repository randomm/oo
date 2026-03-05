use crate::error::Error;
use crate::learn::{call_anthropic, call_openai, patterns_dir, strip_fences, validate_pattern_toml};

/// Run the retry-with-feedback loop for learning patterns.
///
/// Makes up to 3 attempts (1 initial + 2 retries) to get valid TOML from the LLM.
/// If validation fails, builds a feedback message and retries.
pub fn retry_loop(
    command: &str,
    user_msg: &str,
    provider: &str,
    api_key: &str,
    model: &str,
) -> Result<String, Error> {
    let get_response = |msg: &str| -> Result<String, Error> {
        match provider {
            "anthropic" => call_anthropic(
                "https://api.anthropic.com/v1/messages",
                api_key,
                model,
                msg,
            ),
            "openai" => call_openai(
                "https://api.openai.com/v1/chat/completions",
                api_key,
                model,
                msg,
            ),
            "cerebras" => call_openai(
                "https://api.cerebras.ai/v1/chat/completions",
                api_key,
                model,
                msg,
            ),
            other => Err(Error::Learn(format!("unknown provider: {other}"))),
        }
    };

    let mut toml_response = get_response(user_msg)?;

    // Retry loop: up to 3 attempts total (1 initial + 2 retries)
    let mut last_error = String::new();
    for attempt in 0..3 {
        // Strip markdown fences if present
        let toml_clean = strip_fences(&toml_response);

        // Validate: parse as pattern
        match validate_pattern_toml(&toml_clean) {
            Ok(()) => {
                // Validation passed - return the clean TOML
                return Ok(toml_clean);
            }
            Err(e) => {
                last_error = e.to_string();
                if attempt < 2 {
                    // Build retry message with feedback
                    let retry_msg = format!(
                        "Your previous TOML was invalid: {}. Here is what you returned:\n{}\nOutput ONLY the corrected TOML, nothing else.",
                        last_error, &toml_clean
                    );
                    toml_response = get_response(&retry_msg)?;
                }
                // If attempt >= 2, we've exhausted retries - fall through to return error
            }
        }
    }

    // All retries exhausted
    Err(Error::Learn(format!("failed after 3 attempts: {last_error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_loop_saves_valid_toml() {
        // Test that retry_loop returns Ok when validation passes
        let command = "echo test";
        let user_msg = "Command: echo test\nExit code: 0\nOutput:\ntest";

        // This is a simplified test - in practice, this would need mocking
        // The actual retry logic is tested in learn_retry_tests.rs
        // This unit test exists to ensure the module compiles and exports work
    }
}