use crate::error::Error;
use std::io::Read;
use std::time::Duration;

/// Percent-encode a command string for safe inclusion in a URL path.
///
/// Only unreserved characters (alphanumeric, `-`, `_`, `.`, `~`) are kept as-is.
/// Everything else is encoded as `%XX` using uppercase hex. This prevents
/// URL injection via crafted command strings containing `&`, `%`, `?`, `#`, etc.
pub fn encode_cmd(cmd: &str) -> String {
    let mut out = String::with_capacity(cmd.len());
    for byte in cmd.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b => {
                // Use a nibble lookup table to avoid unwrap() per AGENTS.md policy.
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                out.push('%');
                out.push(HEX[(b >> 4) as usize] as char);
                out.push(HEX[(b & 0xf) as usize] as char);
            }
        }
    }
    out
}

/// Fetch a compressed cheat sheet for `cmd` from cheat.sh.
///
/// `?T` strips ANSI colour codes so the output is plain text.
/// Response is capped at 64 KiB to prevent memory exhaustion.
pub fn lookup(cmd: &str) -> Result<String, Error> {
    lookup_with_base_url(cmd, "https://cheat.sh")
}

/// Testable variant that accepts a custom base URL (e.g. a mockito server).
///
/// Separating the base URL enables unit tests without live network access.
fn lookup_with_base_url(cmd: &str, base_url: &str) -> Result<String, Error> {
    if cmd.is_empty() {
        return Err(Error::Help("command name must not be empty".to_owned()));
    }

    let encoded = encode_cmd(cmd);
    let url = format!("{base_url}/{encoded}?T");

    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build();
    let agent: ureq::Agent = config.into();

    match agent.get(&url).call() {
        Ok(mut response) => {
            // Cap the response to 64 KiB to prevent unbounded memory growth.
            const MAX_BYTES: u64 = 65_536;
            let mut buf = String::new();
            response
                .body_mut()
                .as_reader()
                .take(MAX_BYTES)
                .read_to_string(&mut buf)
                .map_err(|e| Error::Help(format!("read response: {e}")))?;
            Ok(buf)
        }
        Err(ureq::Error::StatusCode(404)) => {
            Err(Error::Help(format!("no help available for '{cmd}'")))
        }
        Err(e) => Err(Error::Help(format!("network error: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_cmd_spaces() {
        assert_eq!(encode_cmd("git commit"), "git%20commit");
    }

    #[test]
    fn test_encode_cmd_simple() {
        assert_eq!(encode_cmd("find"), "find");
    }

    #[test]
    fn test_encode_cmd_special_chars() {
        // & and % must be encoded to prevent URL injection.
        assert_eq!(encode_cmd("foo&bar"), "foo%26bar");
        assert_eq!(encode_cmd("foo%bar"), "foo%25bar");
    }

    #[test]
    fn test_encode_cmd_hash_and_query() {
        assert_eq!(encode_cmd("foo#bar"), "foo%23bar");
        assert_eq!(encode_cmd("foo?bar"), "foo%3Fbar");
    }

    #[test]
    fn test_encode_cmd_slash() {
        assert_eq!(encode_cmd("a/b"), "a%2Fb");
    }

    #[test]
    fn test_encode_cmd_unreserved_passthrough() {
        // Unreserved chars (RFC 3986) must not be encoded.
        assert_eq!(encode_cmd("abc-XYZ_0.9~"), "abc-XYZ_0.9~");
    }

    #[test]
    fn test_encode_cmd_null_byte() {
        assert_eq!(encode_cmd("foo\0bar"), "foo%00bar");
    }

    #[test]
    fn test_lookup_empty_returns_error() {
        let result = lookup("");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("empty"), "expected 'empty' in: {msg}");
    }

    #[test]
    fn test_help_lookup_success() {
        // Mock cheat.sh returning 200 with help text.
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/ls?T")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body("ls - list directory contents\n  -l  long listing format\n")
            .create();

        let result = lookup_with_base_url("ls", &server.url());
        assert!(result.is_ok(), "expected Ok, got: {result:?}");
        let text = result.unwrap();
        assert!(
            text.contains("list directory"),
            "response body must be returned: {text}"
        );
        mock.assert();
    }

    #[test]
    fn test_help_lookup_not_found() {
        // Mock cheat.sh returning 404 — must produce a Help error.
        // The lookup function appends "?T" to the path, so we mock that exact path.
        let mut server = mockito::Server::new();
        let mock = server.mock("GET", "/nosuchcmd?T").with_status(404).create();

        let result = lookup_with_base_url("nosuchcmd", &server.url());
        assert!(result.is_err(), "expected Err on 404");
        let msg = result.unwrap_err().to_string();
        // 404 maps to Error::Help("no help available for '...'") in lookup_with_base_url.
        assert!(
            msg.contains("no help"),
            "expected 'no help' error message, got: {msg}"
        );
        mock.assert();
    }

    #[test]
    fn test_help_lookup_network_error() {
        // Pointing at an unreachable address must produce a network-error variant.
        // Port 1 is conventionally unroutable, so the connection is refused quickly.
        let result = lookup_with_base_url("ls", "http://127.0.0.1:1");
        assert!(result.is_err(), "expected Err on unreachable host");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("network") || msg.contains("error") || msg.contains("connect"),
            "expected network error message, got: {msg}"
        );
    }

    #[test]
    fn test_help_encode_cmd_special_chars_in_lookup() {
        // Verify encode_cmd is applied: "git commit" → "git%20commit" in URL path.
        let mut server = mockito::Server::new();
        // The path must use the percent-encoded form.
        let mock = server
            .mock("GET", "/git%20commit?T")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body("git commit - record changes to the repository\n")
            .create();

        let result = lookup_with_base_url("git commit", &server.url());
        assert!(result.is_ok(), "expected Ok: {result:?}");
        mock.assert();
    }

    #[test]
    fn test_help_response_cap() {
        // A response larger than 64 KiB must be truncated to exactly 64 KiB.
        let mut server = mockito::Server::new();
        // 128 KiB of 'x' characters — well above the 64 KiB cap.
        let large_body = "x".repeat(131_072);
        let mock = server
            .mock("GET", "/bigcmd?T")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body(large_body.as_str())
            .create();

        let result = lookup_with_base_url("bigcmd", &server.url());
        assert!(result.is_ok(), "expected Ok: {result:?}");
        let text = result.unwrap();
        assert!(
            text.len() <= 65_536,
            "response must be capped at 64 KiB, got {} bytes",
            text.len()
        );
        // Must have received some content (not empty)
        assert!(!text.is_empty(), "response must not be empty");
        mock.assert();
    }

    #[test]
    #[ignore = "requires network"]
    fn test_lookup_known_command() {
        let result = lookup("echo");
        assert!(result.is_ok(), "lookup failed: {:?}", result.err());
        let content = result.unwrap();
        assert!(!content.is_empty());
    }

    #[test]
    #[ignore = "requires network"]
    fn test_lookup_nonexistent_command() {
        // cheat.sh returns 200 with a "not found" page for most unknown commands
        // rather than a 404, so we assert Ok with non-empty content OR a Help Err —
        // either is acceptable; what is forbidden is a panic.
        let result = lookup("__nonexistent_oo_test_xyz__");
        match result {
            Ok(content) => assert!(!content.is_empty(), "expected non-empty response"),
            Err(e) => assert!(
                e.to_string().contains("no help") || e.to_string().contains("network"),
                "unexpected error variant: {e}"
            ),
        }
    }
}
