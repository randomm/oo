use super::*;

// `Error::Learn` displays with a "learn error: " prefix; each case is matched against its full text.
const CREDENTIALS_ERROR: &str = "learn error: ANTHROPIC_API_URL must not contain credentials";
const MALFORMED_ERROR: &str = "learn error: ANTHROPIC_API_URL is not a valid URL";
const SCHEME_OR_HOST_ERROR: &str = "learn error: ANTHROPIC_API_URL must use HTTPS (HTTP is only allowed for localhost, 127.0.0.1 and ::1)";

const ACCEPTED: &[&str] = &[
    "https://api.anthropic.com/v1/messages",
    "https://custom.example.com/api",
    "HTTPS://API.ANTHROPIC.COM/v1/messages",
    "http://localhost/v1/messages",
    "http://LOCALHOST/v1/messages",
    "HTTP://LOCALHOST/v1/messages",
    "http://localhost:9/v1/messages",
    "http://127.0.0.1/api",
    "http://127.0.0.1:8000/api",
    "http://[::1]/",
    "http://[::1]:8080/",
];

// Userinfo in any form, for both schemes; each must fail with the credentials error.
const USERINFO: &[&str] = &[
    "http://localhost:@evil.com/",
    "http://localhost:@nonexistent-host.invalid/v1/messages",
    "http://127.0.0.1@evil.com/",
    "http://user:s3cret@localhost/",
    "https://user:s3cret@api.anthropic.com/",
];

// Lookalike hosts and other schemes; each must fail with the scheme/host error.
const WRONG_SCHEME_OR_HOST: &[&str] = &[
    "http://localhost.evil.com/",
    "http://localhost./v1/messages",
    "http://127.0.0.1.evil.com/",
    "http://example.com",
    "http://192.168.1.1/api",
    "http://[::2]:8080/",
    "ftp://localhost/",
    // Rejected by the scheme check before URI parsing (http::Uri would also reject an authority-less URL).
    "file:///etc/passwd",
];

// Input that is not a URL at all (no parse, no scheme, or no host); each must fail with the malformed error.
const MALFORMED: &[&str] = &["", "http://", "https://", "not a url", "localhost:8000/api"];

fn rejection(url: &str) -> String {
    ApiUrl::parse(url)
        .expect_err("expected rejection")
        .to_string()
}

#[test]
fn test_parse_accepts_allowed_urls() {
    for url in ACCEPTED {
        assert!(ApiUrl::parse(url).is_ok(), "expected {url} to be accepted");
    }
}

#[test]
fn test_parse_returns_the_normalized_uri_it_validated() {
    // The request is sent to `uri()`, so its string form is what was checked.
    // The http crate lowercases the scheme; the host keeps its case.
    let parsed = ApiUrl::parse("HTTPS://API.ANTHROPIC.COM/v1/messages").expect("accepted");
    assert_eq!(
        parsed.uri().to_string(),
        "https://API.ANTHROPIC.COM/v1/messages"
    );
    for url in ACCEPTED {
        let normalized = ApiUrl::parse(url).expect("accepted").uri().to_string();
        let again = ApiUrl::parse(&normalized).expect("normalized form is accepted");
        assert_eq!(again.uri().to_string(), normalized, "not idempotent: {url}");
    }
}

#[test]
fn test_parse_rejects_userinfo_without_echoing_it() {
    for url in USERINFO {
        let msg = rejection(url);
        assert_eq!(msg, CREDENTIALS_ERROR, "{url}");
        assert!(
            !msg.contains("s3cret") && !msg.contains("user:"),
            "error must not echo credentials: {msg}"
        );
    }
}

#[test]
fn test_parse_rejects_percent_and_backslash_authority_tricks() {
    // The http crate reads `localhost%2f@evil.com` as userinfo; the backslash form
    // is not a valid URI. Neither may pass as localhost.
    assert_eq!(rejection("http://localhost%2f@evil.com"), CREDENTIALS_ERROR);
    assert_eq!(rejection("http://localhost\\@evil.com"), MALFORMED_ERROR);
}

#[test]
fn test_parse_rejects_wrong_scheme_or_host() {
    for url in WRONG_SCHEME_OR_HOST {
        assert_eq!(rejection(url), SCHEME_OR_HOST_ERROR, "{url:?}");
    }
}

#[test]
fn test_parse_rejects_malformed_input() {
    for url in MALFORMED {
        assert_eq!(rejection(url), MALFORMED_ERROR, "{url:?}");
    }
}

#[test]
fn test_scheme_or_host_error_names_every_loopback_host() {
    // The message spells IPv6 loopback as `::1`, so compare without the brackets.
    for host in LOOPBACK_HOSTS {
        let bare = host.trim_matches(|c| c == '[' || c == ']');
        assert!(
            SCHEME_OR_HOST_ERROR.contains(bare),
            "{host} missing from the error text"
        );
    }
}
