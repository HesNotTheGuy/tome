//! Test doubles. Behind a `cfg(any(test, feature = "testing"))` gate so they
//! are compiled into the production binary only when explicitly opted in.
//!
//! `MockTransport` lets us script HTTP responses for governance tests without
//! standing up a real HTTP server — the gatekeeper logic is fully exercised
//! against an in-process double.

use std::sync::Mutex;

use async_trait::async_trait;

use crate::transport::{HttpError, HttpRequest, HttpResponse, HttpTransport};

#[derive(Debug, Clone)]
pub enum MockResponse {
    Ok {
        status: u16,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    },
    NetworkError(String),
}

impl MockResponse {
    pub fn ok(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self::Ok {
            status,
            headers: Vec::new(),
            body: body.into(),
        }
    }

    pub fn ok_with_headers(
        status: u16,
        headers: Vec<(String, String)>,
        body: impl Into<Vec<u8>>,
    ) -> Self {
        Self::Ok {
            status,
            headers,
            body: body.into(),
        }
    }

    pub fn network_error(msg: impl Into<String>) -> Self {
        Self::NetworkError(msg.into())
    }
}

pub struct MockTransport {
    scripted: Mutex<Vec<MockResponse>>,
    log: Mutex<Vec<HttpRequest>>,
}

impl MockTransport {
    pub fn new(scripted: Vec<MockResponse>) -> Self {
        Self {
            scripted: Mutex::new(scripted),
            log: Mutex::new(Vec::new()),
        }
    }

    pub fn requests(&self) -> Vec<HttpRequest> {
        self.log.lock().expect("log mutex poisoned").clone()
    }

    pub fn requests_made(&self) -> usize {
        self.log.lock().expect("log mutex poisoned").len()
    }

    pub fn remaining_scripted(&self) -> usize {
        self.scripted.lock().expect("scripted mutex poisoned").len()
    }
}

#[async_trait]
impl HttpTransport for MockTransport {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        self.log
            .lock()
            .expect("log mutex poisoned")
            .push(request.clone());
        let mut scripted = self.scripted.lock().expect("scripted mutex poisoned");
        if scripted.is_empty() {
            return Err(HttpError::Network(
                "MockTransport: no more scripted responses".into(),
            ));
        }
        match scripted.remove(0) {
            MockResponse::Ok {
                status,
                headers,
                body,
            } => Ok(HttpResponse {
                status,
                headers,
                body,
            }),
            MockResponse::NetworkError(msg) => Err(HttpError::Network(msg)),
        }
    }
}
