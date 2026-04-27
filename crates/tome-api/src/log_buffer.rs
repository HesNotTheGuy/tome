//! Bounded ring buffer of recent outbound requests.
//!
//! Capped at 1000 entries by default (configurable). Powers the developer
//! debug view: the user can inspect what URL was hit, when, with what
//! status. User-entered text is never logged here.

use std::collections::VecDeque;
use std::sync::Mutex;

use tokio::time::Instant;

const DEFAULT_CAPACITY: usize = 1000;

#[derive(Debug, Clone)]
pub struct RequestEntry {
    pub at: Instant,
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub error: Option<String>,
}

pub struct RequestLog {
    capacity: usize,
    state: Mutex<VecDeque<RequestEntry>>,
}

impl Default for RequestLog {
    fn default() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }
}

impl RequestLog {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            state: Mutex::new(VecDeque::with_capacity(capacity.max(1))),
        }
    }

    pub fn push(&self, entry: RequestEntry) {
        let mut state = self.state.lock().expect("log mutex poisoned");
        if state.len() == self.capacity {
            state.pop_front();
        }
        state.push_back(entry);
    }

    pub fn snapshot(&self) -> Vec<RequestEntry> {
        let state = self.state.lock().expect("log mutex poisoned");
        state.iter().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.state.lock().expect("log mutex poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(url: &str) -> RequestEntry {
        RequestEntry {
            at: Instant::now(),
            method: "GET".into(),
            url: url.into(),
            status: Some(200),
            error: None,
        }
    }

    #[tokio::test(start_paused = true)]
    async fn pushes_grow_until_capacity() {
        let log = RequestLog::new(3);
        log.push(entry("/a"));
        log.push(entry("/b"));
        assert_eq!(log.len(), 2);
        log.push(entry("/c"));
        assert_eq!(log.len(), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn beyond_capacity_evicts_oldest() {
        let log = RequestLog::new(3);
        for url in ["/a", "/b", "/c", "/d", "/e"] {
            log.push(entry(url));
        }
        let snap = log.snapshot();
        assert_eq!(snap.len(), 3);
        assert_eq!(snap[0].url, "/c");
        assert_eq!(snap[1].url, "/d");
        assert_eq!(snap[2].url, "/e");
    }

    #[tokio::test(start_paused = true)]
    async fn snapshot_is_a_copy() {
        let log = RequestLog::new(2);
        log.push(entry("/a"));
        let snap1 = log.snapshot();
        log.push(entry("/b"));
        let snap2 = log.snapshot();
        assert_eq!(snap1.len(), 1);
        assert_eq!(snap2.len(), 2);
    }
}
