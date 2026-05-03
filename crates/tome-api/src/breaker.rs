//! Circuit breaker.
//!
//! After 10 errors in a 60-second window, the breaker opens and rejects all
//! requests for 5 minutes. Once the cooldown elapses, the next call to
//! `is_open` clears the state and traffic resumes.

use std::collections::VecDeque;
use std::sync::Mutex;

use tokio::time::{Duration, Instant};

const DEFAULT_ERROR_THRESHOLD: usize = 10;
const DEFAULT_ERROR_WINDOW: Duration = Duration::from_secs(60);
const DEFAULT_OPEN_DURATION: Duration = Duration::from_secs(300);

pub struct CircuitBreaker {
    error_threshold: usize,
    error_window: Duration,
    open_duration: Duration,
    state: Mutex<State>,
}

struct State {
    errors: VecDeque<Instant>,
    opened_at: Option<Instant>,
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

impl CircuitBreaker {
    pub fn new() -> Self {
        Self {
            error_threshold: DEFAULT_ERROR_THRESHOLD,
            error_window: DEFAULT_ERROR_WINDOW,
            open_duration: DEFAULT_OPEN_DURATION,
            state: Mutex::new(State {
                errors: VecDeque::new(),
                opened_at: None,
            }),
        }
    }

    /// Returns true if the breaker is currently rejecting requests. If the
    /// cooldown has elapsed, the state is cleared and false is returned.
    pub fn is_open(&self) -> bool {
        let mut state = self.state.lock().expect("breaker mutex poisoned");
        if let Some(opened) = state.opened_at {
            let now = Instant::now();
            if now.saturating_duration_since(opened) >= self.open_duration {
                state.opened_at = None;
                state.errors.clear();
                return false;
            }
            return true;
        }
        false
    }

    pub fn record_error(&self) {
        let mut state = self.state.lock().expect("breaker mutex poisoned");
        if state.opened_at.is_some() {
            return;
        }
        let now = Instant::now();
        // Drop errors that fell out of the rolling window.
        while let Some(&front) = state.errors.front() {
            if now.saturating_duration_since(front) > self.error_window {
                state.errors.pop_front();
            } else {
                break;
            }
        }
        state.errors.push_back(now);
        if state.errors.len() >= self.error_threshold {
            state.opened_at = Some(now);
        }
    }

    #[cfg(test)]
    fn errors_in_window(&self) -> usize {
        self.state.lock().unwrap().errors.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn closed_when_under_threshold() {
        let cb = CircuitBreaker::new();
        for _ in 0..9 {
            cb.record_error();
        }
        assert!(!cb.is_open());
    }

    #[tokio::test(start_paused = true)]
    async fn opens_at_threshold() {
        let cb = CircuitBreaker::new();
        for _ in 0..10 {
            cb.record_error();
        }
        assert!(cb.is_open());
    }

    #[tokio::test(start_paused = true)]
    async fn closes_after_cooldown() {
        let cb = CircuitBreaker::new();
        for _ in 0..10 {
            cb.record_error();
        }
        assert!(cb.is_open());

        tokio::time::sleep(Duration::from_secs(299)).await;
        assert!(cb.is_open(), "still inside cooldown");

        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(!cb.is_open(), "cooldown elapsed; should be closed");
    }

    #[tokio::test(start_paused = true)]
    async fn errors_outside_window_do_not_count() {
        let cb = CircuitBreaker::new();
        for _ in 0..9 {
            cb.record_error();
        }
        // Move past the 60s window: those 9 errors should age out.
        tokio::time::sleep(Duration::from_secs(61)).await;
        for _ in 0..9 {
            cb.record_error();
        }
        // Total in window is 9 — still closed.
        assert_eq!(cb.errors_in_window(), 9);
        assert!(!cb.is_open());
    }
}
