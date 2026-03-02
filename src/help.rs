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
    if cmd.is_empty() {
        return Err(Error::Help("command name must not be empty".to_owned()));
    }

    let encoded = encode_cmd(cmd);
    let url = format!("https://cheat.sh/{encoded}?T");

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
