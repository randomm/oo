//! Validation of `ANTHROPIC_API_URL`.
//!
//! `ApiUrl` can only be built through `ApiUrl::parse`, so a request that takes an
//! `&ApiUrl` is guaranteed to target the host that was checked.

use ureq::http::Uri;

use crate::error::Error;

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
        let uri: Uri = url.parse().map_err(|_| malformed())?;
        let authority = uri.authority().ok_or_else(malformed)?;
        if authority.as_str().contains('@') {
            return Err(Error::Learn(
                "ANTHROPIC_API_URL must not contain credentials".into(),
            ));
        }
        let host = authority.host().to_ascii_lowercase();
        match uri.scheme_str().map(str::to_ascii_lowercase).as_deref() {
            Some("https") => Ok(Self(uri)),
            Some("http") if matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]") => {
                Ok(Self(uri))
            }
            _ => Err(Error::Learn(
                "ANTHROPIC_API_URL must use HTTPS (HTTP is only allowed for localhost, 127.0.0.1 and ::1)".into(),
            )),
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

#[cfg(test)]
#[path = "learn_url_tests.rs"]
mod tests;
