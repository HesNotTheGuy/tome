//! Real HTTP transport built on `reqwest`. The only crate-internal use of
//! reqwest in Tome — every other layer goes through the [`HttpTransport`]
//! trait so it can be tested without a network.

use async_trait::async_trait;
use tokio::time::Duration;

use crate::transport::{HttpError, HttpRequest, HttpResponse, HttpTransport};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new() -> Result<Self, HttpError> {
        let client = reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .map_err(|e| HttpError::Network(format!("build client: {e}")))?;
        Ok(Self { client })
    }
}

#[async_trait]
impl HttpTransport for ReqwestTransport {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        let mut builder = match request.method.as_str() {
            "GET" => self.client.get(&request.url),
            other => {
                return Err(HttpError::InvalidUrl(format!(
                    "method not supported: {other}"
                )));
            }
        };
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        let response = builder.send().await.map_err(|e| {
            if e.is_timeout() {
                HttpError::Timeout
            } else {
                HttpError::Network(e.to_string())
            }
        })?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter_map(|(k, v)| {
                let value = v.to_str().ok()?.to_string();
                Some((k.to_string(), value))
            })
            .collect();
        let body = response
            .bytes()
            .await
            .map_err(|e| HttpError::Network(e.to_string()))?
            .to_vec();
        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}
