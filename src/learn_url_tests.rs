use super::*;

const ACCEPTED: &[&str] = &[
    "https://api.anthropic.com/v1/messages",
    "https://custom.example.com/api",
    "HTTPS://API.ANTHROPIC.COM/v1/messages",
    "http://localhost/v1/messages",
    "http://localhost:9/v1/messages",
    "http://LOCALHOST/v1/messages",
    "HTTP://LOCALHOST/v1/messages",
    "http://127.0.0.1:8000/api",
    "http://127.0.0.1/api",
    "http://[::1]:8080/",
    "http://[::1]/",
];

const REJECTED: &[&str] = &[
    // userinfo bypasses of the host check
    "http://localhost:@evil.com/",
    "http://localhost:@nonexistent-host.invalid/v1/messages",
    "http://127.0.0.1@evil.com/",
    "http://user:pw@localhost/",
    "https://user:pw@api.anthropic.com/",
    "http://localhost%2f@evil.com",
    "http://localhost\\@evil.com",
    // lookalike hosts
    "http://localhost.evil.com/",
    "http://127.0.0.1.evil.com/",
    "http://example.com",
    "http://192.168.1.1/api",
    "http://[::2]:8080/",
    // non-http(s) schemes
    "ftp://localhost/",
    "file:///etc/passwd",
    // empty and malformed input
    "",
    "localhost:8000/api",
    "http://",
    "https://",
    "not a url",
];

#[test]
fn test_validate_accepts_allowed_urls() {
    for url in ACCEPTED {
        assert!(
            validate_anthropic_url(url).is_ok(),
            "expected {url} to be accepted"
        );
    }
}

#[test]
fn test_validate_rejects_bypass_and_malformed_urls() {
    for url in REJECTED {
        let result = validate_anthropic_url(url);
        assert!(result.is_err(), "expected {url:?} to be rejected");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("ANTHROPIC_API_URL must use HTTPS"),
            "wrong error for {url:?}: {msg}"
        );
    }
}
