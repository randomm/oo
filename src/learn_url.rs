//! Validation of `ANTHROPIC_API_URL`.
//!
//! `ApiUrl` can only be built through `ApiUrl::parse`, so a request that takes an
//! `&ApiUrl` is guaranteed to target the host that was checked.

use ureq::http::Uri;

use crate::error::Error;

/// Hosts that may be reached over plain `http`. Matched against the lowercased
/// authority host, which keeps the brackets of an IPv6 literal.
const LOOPBACK_HOSTS: &[&str] = &["localhost", "127.0.0.1", "[::1]"];

/// An API URL that passed the scheme, host and userinfo checks.
#[derive(Debug, Clone)]
pub struct ApiUrl(Uri);

impl ApiUrl {
    /// Parse and validate `url`: `https` for any host, or `http` for loopback only.
    ///
    /// Userinfo is rejected for both schemes: `http://localhost:@evil.com/` would
    /// otherwise pass a host check that reads the text before the first `:`.
    /// No error message echoes the URL, because it may carry credentials.
    pub fn parse(url: &str) -> Result<Self, Error> {
        // The scheme is read from the text first: `http::Uri` rejects authority-less
        // URLs such as `file:///etc/passwd`, which belong in the scheme class.
        let (scheme, _) = url.split_once("://").ok_or_else(malformed)?;
        let https = scheme.eq_ignore_ascii_case("https");
        if !https && !scheme.eq_ignore_ascii_case("http") {
            return Err(scheme_or_host());
        }
        let uri: Uri = url.parse().map_err(|_| malformed())?;
        let authority = uri.authority().ok_or_else(malformed)?;
        if authority.as_str().contains('@') {
            return Err(Error::Learn(
                "ANTHROPIC_API_URL must not contain credentials".into(),
            ));
        }
        // `http::Uri` lowercases the scheme but not the host, so only the host is lowered here.
        let host = authority.host().to_ascii_lowercase();
        if https || LOOPBACK_HOSTS.contains(&host.as_str()) {
            Ok(Self(uri))
        } else {
            Err(scheme_or_host())
        }
    }

    /// The validated URI, the same value the HTTP request is sent to.
    pub fn uri(&self) -> &Uri {
        &self.0
    }
}

fn malformed() -> Error {
    Error::Learn("ANTHROPIC_API_URL is not a valid URL".into())
}

fn scheme_or_host() -> Error {
    Error::Learn(
        "ANTHROPIC_API_URL must use HTTPS (HTTP is only allowed for localhost, 127.0.0.1 and ::1)"
            .into(),
    )
}

#[cfg(test)]
#[path = "learn_url_tests.rs"]
mod tests;
