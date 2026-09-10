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

/// Return true if `body` looks like an HTML document rather than a plain-text
/// cheat sheet.
///
/// Detection scans only the first 1024 bytes after trimming leading
/// whitespace and the UTF-8 BOM, so a bare `<` mid-document (e.g.
/// `git checkout <branch>` in a valid sheet) can never trigger a false
/// positive. Only HTML document markers count, matched case-insensitively
/// anywhere within the window: `<!DOCTYPE`, `<html`, `<head>`, `<style>`.
fn looks_like_html(body: &str) -> bool {
    // Trim leading whitespace and any UTF-8 BOM (U+FEFF).
    let trimmed = body.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == '\u{feff}');
    // Operate on bytes (not chars) per spec: the 1024-byte window must not
    // land a marker check on a non-char boundary.
    let bytes = trimmed.as_bytes();
    let window: &[u8] = &bytes[..bytes.len().min(1024)];
    let lower = window.to_ascii_lowercase();
    const MARKERS: &[&[u8]] = &[b"<!doctype", b"<html", b"<head>", b"<style>"];
    MARKERS
        .iter()
        .any(|m| lower.windows(m.len()).any(|w| w == *m))
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
            // cheat.sh answers some unknown/multi-word topics (and, under
            // browser-like User-Agents, even valid topics) with a 200 status
            // and an HTML search page. The status code cannot distinguish this,
            // so we inspect the body for HTML document markers and map the
            // case to the same Error::Help variant as a 404.
            if looks_like_html(&buf) {
                return Err(Error::Help(format!("no help available for '{cmd}'")));
            }
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

    // --- HTML-detection tests (mockito, deterministic, no network) ---

    #[test]
    fn test_help_html_doctype_rejected() {
        // Primary regression: cheat.sh returning 200 with an HTML body for an
        // unknown topic must map to a "no help" error, not raw HTML.
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/somecmd?T")
            .with_status(200)
            .with_header("content-type", "text/html")
            .with_body("<!DOCTYPE html>\n<html>\n<head><title>Search</title>\n</head>\n<body>no topic</body>\n</html>\n")
            .create();

        let result = lookup_with_base_url("somecmd", &server.url());
        assert!(
            result.is_err(),
            "expected Err for HTML body, got: {result:?}"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("no help"),
            "expected 'no help' error, got: {msg}"
        );
        mock.assert();
    }

    #[test]
    fn test_help_html_html_marker_rejected() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/htmlmarker?T")
            .with_status(200)
            .with_header("content-type", "text/html")
            .with_body("<html><body>cheat.sh</body></html>")
            .create();

        let result = lookup_with_base_url("htmlmarker", &server.url());
        assert!(
            result.is_err(),
            "expected Err for <html marker, got: {result:?}"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("no help"),
            "expected 'no help' error, got: {msg}"
        );
        mock.assert();
    }

    #[test]
    fn test_help_html_head_marker_rejected() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/headmarker?T")
            .with_status(200)
            .with_header("content-type", "text/html")
            .with_body("<head><title>cheat.sh/git</title></head>")
            .create();

        let result = lookup_with_base_url("headmarker", &server.url());
        assert!(
            result.is_err(),
            "expected Err for <head> marker, got: {result:?}"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("no help"),
            "expected 'no help' error, got: {msg}"
        );
        mock.assert();
    }

    #[test]
    fn test_help_html_style_marker_rejected() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/stylemarker?T")
            .with_status(200)
            .with_header("content-type", "text/html")
            .with_body("<style>body { margin: 0; }</style><body>x</body>")
            .create();

        let result = lookup_with_base_url("stylemarker", &server.url());
        assert!(
            result.is_err(),
            "expected Err for <style> marker, got: {result:?}"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("no help"),
            "expected 'no help' error, got: {msg}"
        );
        mock.assert();
    }

    #[test]
    fn test_help_html_after_leading_whitespace_and_bom_rejected() {
        // Mirrors the live browser-UA sample which began with "\n<html>...";
        // leading whitespace and a UTF-8 BOM must be trimmed before scanning.
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/bomhtml?T")
            .with_status(200)
            .with_header("content-type", "text/html")
            .with_body("\u{feff}\n\n  <html>\n<head><title>cheat.sh/git</title></head>")
            .create();

        let result = lookup_with_base_url("bomhtml", &server.url());
        assert!(
            result.is_err(),
            "expected Err for BOM+whitespace HTML, got: {result:?}"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("no help"),
            "expected 'no help' error, got: {msg}"
        );
        mock.assert();
    }

    #[test]
    fn test_help_html_marker_outside_window_allowed() {
        // A bare `<` (and even an `<html` marker) appearing only AFTER the
        // 1024-byte detection window must NOT be flagged as HTML — pins the
        // window boundary.
        let mut server = mockito::Server::new();
        // 1024 bytes of harmless text, then a marker well outside the window.
        let padded = "p".repeat(1024) + "\n<html><body>x</body></html>";
        let mock = server
            .mock("GET", "/farhtml?T")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body(padded.as_str())
            .create();

        let result = lookup_with_base_url("farhtml", &server.url());
        assert!(result.is_ok(), "expected Ok, got: {result:?}");
        mock.assert();
    }

    #[test]
    fn test_help_plain_text_with_angle_brackets_allowed() {
        // False-positive guard: a valid plain-text sheet containing angle
        // brackets in code examples must still be returned, not classified
        // as HTML. Pins "check document markers, not bare <".
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/git%20checkout?T")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body("git checkout - switch branches\n  git checkout <branch>\n  git clone --depth 1 <remote-url>\n  More: <https://git-scm.com/docs/git-checkout>\n")
            .create();

        let result = lookup_with_base_url("git checkout", &server.url());
        assert!(
            result.is_ok(),
            "expected Ok for plain sheet with <>, got: {result:?}"
        );
        let text = result.unwrap();
        assert!(
            text.contains("git checkout <branch>"),
            "body must be returned intact: {text}"
        );
        mock.assert();
    }

    #[test]
    fn test_help_response_cap_html_rejected() {
        // A >64 KiB HTML body must return the "no help" error rather than
        // Ok-with-cap — pins the cap staying in place for HTML too.
        let mut server = mockito::Server::new();
        let html_prefix = "<!DOCTYPE html>\n<html>\n<head><title>cheat.sh</title>\n</head>\n<body>";
        let padded = format!("{}{}</body></html>", html_prefix, "x".repeat(131_072));
        let mock = server
            .mock("GET", "/bighml?T")
            .with_status(200)
            .with_header("content-type", "text/html")
            .with_body(padded.as_str())
            .create();

        let result = lookup_with_base_url("bighml", &server.url());
        assert!(
            result.is_err(),
            "expected Err for large HTML body, got: {result:?}"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("no help"),
            "expected 'no help' error, got: {msg}"
        );
        mock.assert();
    }

    #[test]
    #[ignore = "requires network"]
    fn test_lookup_nonexistent_command() {
        // Unknown topics now map to the Error::Help("no help available…")
        // variant whether cheat.sh answers with 404 or 200+HTML — the old
        // either-branch assertion (accepting Ok with non-empty content) is
        // wrong by design. Only a network failure can produce a different
        // error variant.
        let result = lookup("__nonexistent_oo_test_xyz__");
        let err = result.expect_err("unknown topic must not return Ok content");
        assert!(
            matches!(&err, Error::Help(_)),
            "expected Error::Help variant, got: {err}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("no help"),
            "expected 'no help' in message, got: {msg}"
        );
    }

    // --- looks_like_html unit tests ---------------------------------------

    #[test]
    fn test_looks_like_html_markers() {
        assert!(looks_like_html("<!DOCTYPE html><html></html>"));
        assert!(looks_like_html("<html><body>page</body></html>"));
        assert!(looks_like_html("<head><title>t</title></head>"));
        assert!(looks_like_html("<style>body{}</style>"));
        assert!(looks_like_html("  \n <!doctype html>"));
        assert!(looks_like_html("\u{feff}<html>"));
    }

    #[test]
    fn test_looks_like_html_plain_text_with_angle_brackets() {
        // Valid sheets contain bare `<` and `https://` URLs — not HTML.
        let sheet = "git clone --depth 1 <remote-url>\nsee <https://git-scm.com> for more\nif (a < b) print";
        assert!(!looks_like_html(sheet));
    }

    #[test]
    fn test_looks_like_html_window_boundary() {
        // A marker only appearing after the 1024-byte window is plain text.
        let padding = "x ".repeat(550); // 1100 bytes
        assert!(!looks_like_html(&format!("{padding}<html>")));
    }

    #[test]
    fn test_looks_like_html_multibyte_at_window_edge() {
        // BOM (3 bytes in UTF-8) near the window edge: the window slice must
        // land on a char boundary (not mid-BOM) without panicking, and a
        // marker just inside the window is still detected.
        let body = format!("\u{feff}{}<style>", "x".repeat(1000));
        assert!(looks_like_html(&body));
        // A 4-byte char (U+10000) straddling the window edge must not panic
        // the slice; the marker sits well past the window → plain text.
        let body = format!("{}\u{10000}<style>", "x".repeat(1020));
        assert!(!looks_like_html(&body));
    }
}
