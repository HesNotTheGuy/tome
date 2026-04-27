//! `MediaWikiClient` — composes every governance primitive into a single
//! gateway. All outbound MediaWiki traffic must go through this type.

use std::sync::Arc;

use tokio::time::Instant;
use tome_core::{Result, TomeError};
use url::Url;

use crate::backoff::BackoffState;
use crate::breaker::CircuitBreaker;
use crate::cache::Cache;
use crate::kill_switch::KillSwitch;
use crate::log_buffer::{RequestEntry, RequestLog};
use crate::rate_limit::TokenBucket;
use crate::transport::{HttpRequest, HttpResponse, HttpTransport};

const MAX_REQUESTS_PER_SECOND_CEILING: u32 = 10;

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub base_url: String,
    pub user_agent: String,
    pub requests_per_second: u32,
    pub max_attempts: u32,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            base_url: tome_config::WIKIPEDIA_REST_HTML_BASE.to_string(),
            user_agent: tome_config::DEFAULT_USER_AGENT.to_string(),
            requests_per_second: tome_config::MAX_REQUESTS_PER_SECOND,
            max_attempts: 7, // 1 initial + 6 retries with the 1,2,4,8,16,32 schedule
        }
    }
}

pub struct MediaWikiClient {
    transport: Arc<dyn HttpTransport>,
    rate_limit: TokenBucket,
    breaker: CircuitBreaker,
    kill_switch: Arc<KillSwitch>,
    log: RequestLog,
    cache: Cache,
    config: ClientConfig,
}

impl MediaWikiClient {
    pub fn new(
        mut config: ClientConfig,
        transport: Arc<dyn HttpTransport>,
        kill_switch: Arc<KillSwitch>,
    ) -> Self {
        if config.requests_per_second == 0
            || config.requests_per_second > MAX_REQUESTS_PER_SECOND_CEILING
        {
            config.requests_per_second = MAX_REQUESTS_PER_SECOND_CEILING;
        }
        let rate_limit = TokenBucket::new(config.requests_per_second);
        Self {
            transport,
            rate_limit,
            breaker: CircuitBreaker::new(),
            kill_switch,
            log: RequestLog::default(),
            cache: Cache::new(),
            config,
        }
    }

    pub fn kill_switch(&self) -> &KillSwitch {
        &self.kill_switch
    }

    pub fn breaker_is_open(&self) -> bool {
        self.breaker.is_open()
    }

    pub fn recent_requests(&self) -> Vec<RequestEntry> {
        self.log.snapshot()
    }

    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }

    /// Fetch the rendered HTML for an article from the MediaWiki Core REST
    /// endpoint. If `revision` is provided, that specific revision is fetched
    /// (and cached forever — revisions are immutable).
    pub async fn fetch_html(&self, title: &str, revision: Option<u64>) -> Result<String> {
        let url = build_html_url(&self.config.base_url, title, revision)?;
        let request = HttpRequest::get(url)
            .header("User-Agent", &self.config.user_agent)
            .header("Accept", "text/html");
        let response = self.send_with_governance(request).await?;
        String::from_utf8(response.body)
            .map_err(|e| TomeError::Api(format!("response body is not utf-8: {e}")))
    }

    async fn send_with_governance(&self, request: HttpRequest) -> Result<HttpResponse> {
        if self.kill_switch.is_engaged() {
            return Err(TomeError::KillSwitch);
        }
        if self.breaker.is_open() {
            return Err(TomeError::CircuitBreakerOpen);
        }
        let cache_key = format!("{}:{}", request.method, request.url);
        if let Some(body) = self.cache.get(&cache_key) {
            return Ok(HttpResponse {
                status: 200,
                headers: vec![],
                body,
            });
        }

        let mut backoff = BackoffState::new();
        for attempt in 1..=self.config.max_attempts {
            self.rate_limit.acquire().await;

            let log_method = request.method.clone();
            let log_url = request.url.clone();
            let send_result = self.transport.send(request.clone()).await;

            match send_result {
                Ok(response) if response.is_success() => {
                    self.log.push(RequestEntry {
                        at: Instant::now(),
                        method: log_method,
                        url: log_url,
                        status: Some(response.status),
                        error: None,
                    });
                    self.cache.put(cache_key, response.body.clone());
                    return Ok(response);
                }
                Ok(response) => {
                    let status = response.status;
                    self.log.push(RequestEntry {
                        at: Instant::now(),
                        method: log_method,
                        url: log_url,
                        status: Some(status),
                        error: Some(format!("status {status}")),
                    });
                    self.breaker.record_error();
                    if self.breaker.is_open() {
                        return Err(TomeError::CircuitBreakerOpen);
                    }
                    if attempt >= self.config.max_attempts {
                        return Err(TomeError::Api(format!(
                            "max retries exceeded: status {status}"
                        )));
                    }
                    backoff.record_failure();
                    let delay = backoff.next_delay(response.retry_after());
                    tokio::time::sleep(delay).await;
                    if self.breaker.is_open() {
                        return Err(TomeError::CircuitBreakerOpen);
                    }
                }
                Err(e) => {
                    let err_string = e.to_string();
                    self.log.push(RequestEntry {
                        at: Instant::now(),
                        method: log_method,
                        url: log_url,
                        status: None,
                        error: Some(err_string.clone()),
                    });
                    self.breaker.record_error();
                    if self.breaker.is_open() {
                        return Err(TomeError::CircuitBreakerOpen);
                    }
                    if attempt >= self.config.max_attempts {
                        return Err(TomeError::Api(format!("transport: {err_string}")));
                    }
                    backoff.record_failure();
                    tokio::time::sleep(backoff.next_delay(None)).await;
                    if self.breaker.is_open() {
                        return Err(TomeError::CircuitBreakerOpen);
                    }
                }
            }
        }

        Err(TomeError::Api("retry loop exhausted".into()))
    }
}

fn build_html_url(base_url: &str, title: &str, revision: Option<u64>) -> Result<String> {
    let mut url = Url::parse(base_url)
        .map_err(|e| TomeError::Api(format!("invalid base url '{base_url}': {e}")))?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| TomeError::Api("base url cannot be a base".into()))?;
        segments.push(title);
        segments.push("html");
        if let Some(rev) = revision {
            segments.push(&rev.to_string());
        }
    }
    Ok(url.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_url_without_revision() {
        let url = build_html_url(
            "https://en.wikipedia.org/w/rest.php/v1/page",
            "Photon",
            None,
        )
        .unwrap();
        assert_eq!(
            url,
            "https://en.wikipedia.org/w/rest.php/v1/page/Photon/html"
        );
    }

    #[test]
    fn html_url_with_revision() {
        let url = build_html_url(
            "https://en.wikipedia.org/w/rest.php/v1/page",
            "Photon",
            Some(123_456_789),
        )
        .unwrap();
        assert_eq!(
            url,
            "https://en.wikipedia.org/w/rest.php/v1/page/Photon/html/123456789"
        );
    }

    #[test]
    fn html_url_percent_encodes_title() {
        let url = build_html_url(
            "https://en.wikipedia.org/w/rest.php/v1/page",
            "Higgs boson",
            None,
        )
        .unwrap();
        assert_eq!(
            url,
            "https://en.wikipedia.org/w/rest.php/v1/page/Higgs%20boson/html"
        );
    }

    #[test]
    fn config_caps_rps_at_ten() {
        assert_eq!(MAX_REQUESTS_PER_SECOND_CEILING, 10);
        assert_eq!(ClientConfig::default().requests_per_second, 10);
    }
}
