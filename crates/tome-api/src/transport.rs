//! HTTP transport abstraction.
//!
//! Decoupling the wire from the gatekeeper lets the test suite drive the
//! whole pipeline against `wiremock` (or any in-process double) without
//! patching `reqwest`. Production wires this trait to a `reqwest::Client`
//! in step 5 alongside `MediaWikiClient`.

use async_trait::async_trait;
use thiserror::Error;
use tokio::time::Duration;

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
}

impl HttpRequest {
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            method: "GET".into(),
            url: url.into(),
            headers: Vec::new(),
        }
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// Parse the `Retry-After` header. Spec: only the integer-seconds form is
    /// honored. HTTP-date form is rare from MediaWiki and treated as absent.
    pub fn retry_after(&self) -> Option<Duration> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("retry-after"))
            .and_then(|(_, v)| v.trim().parse::<u64>().ok())
            .map(Duration::from_secs)
    }

    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn is_retryable(&self) -> bool {
        self.status >= 400
    }
}

#[derive(Debug, Error)]
pub enum HttpError {
    #[error("network error: {0}")]
    Network(String),

    #[error("timeout")]
    Timeout,

    #[error("invalid url: {0}")]
    InvalidUrl(String),
}

#[async_trait]
pub trait HttpTransport: Send + Sync {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, HttpError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resp_with_retry_after(value: &str) -> HttpResponse {
        HttpResponse {
            status: 429,
            headers: vec![("Retry-After".into(), value.into())],
            body: Vec::new(),
        }
    }

    #[test]
    fn retry_after_seconds_parsed() {
        let r = resp_with_retry_after("17");
        assert_eq!(r.retry_after(), Some(Duration::from_secs(17)));
    }

    #[test]
    fn retry_after_with_whitespace_parsed() {
        let r = resp_with_retry_after("  42  ");
        assert_eq!(r.retry_after(), Some(Duration::from_secs(42)));
    }

    #[test]
    fn retry_after_header_name_case_insensitive() {
        let r = HttpResponse {
            status: 503,
            headers: vec![("retry-after".into(), "5".into())],
            body: vec![],
        };
        assert_eq!(r.retry_after(), Some(Duration::from_secs(5)));
    }

    #[test]
    fn retry_after_http_date_form_treated_as_absent() {
        let r = resp_with_retry_after("Wed, 21 Oct 2026 07:28:00 GMT");
        assert_eq!(r.retry_after(), None);
    }

    #[test]
    fn no_retry_after_header() {
        let r = HttpResponse {
            status: 503,
            headers: vec![],
            body: vec![],
        };
        assert_eq!(r.retry_after(), None);
    }

    #[test]
    fn status_classification() {
        let success = HttpResponse {
            status: 200,
            headers: vec![],
            body: vec![],
        };
        assert!(success.is_success());
        assert!(!success.is_retryable());

        let server_err = HttpResponse {
            status: 503,
            headers: vec![],
            body: vec![],
        };
        assert!(!server_err.is_success());
        assert!(server_err.is_retryable());

        let client_err = HttpResponse {
            status: 429,
            headers: vec![],
            body: vec![],
        };
        assert!(!client_err.is_success());
        assert!(client_err.is_retryable());
    }
}
