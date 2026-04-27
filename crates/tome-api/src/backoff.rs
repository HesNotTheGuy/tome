//! Exponential backoff state machine.
//!
//! Sequence: 1s, 2s, 4s, 8s, 16s, 32s, then 60s for any further attempts.
//! When the server provides a `Retry-After` header, it overrides the
//! schedule and the delay is honored exactly (no cap, no exponentiation).

use tokio::time::Duration;

#[derive(Debug, Default)]
pub struct BackoffState {
    attempts: u32,
}

impl BackoffState {
    pub fn new() -> Self {
        Self { attempts: 0 }
    }

    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    pub fn record_failure(&mut self) {
        self.attempts = self.attempts.saturating_add(1);
    }

    pub fn record_success(&mut self) {
        self.attempts = 0;
    }

    /// Compute the delay before the next attempt, given an optional
    /// `Retry-After` value from the server. Call after `record_failure`.
    pub fn next_delay(&self, retry_after: Option<Duration>) -> Duration {
        if let Some(d) = retry_after {
            return d;
        }
        let secs = match self.attempts {
            0 | 1 => 1,
            2 => 2,
            3 => 4,
            4 => 8,
            5 => 16,
            6 => 32,
            _ => 60,
        };
        Duration::from_secs(secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exponential_sequence_caps_at_60() {
        let mut b = BackoffState::new();
        let expected = [1, 2, 4, 8, 16, 32, 60, 60, 60];
        for &want in &expected {
            b.record_failure();
            let got = b.next_delay(None);
            assert_eq!(got, Duration::from_secs(want), "attempts={}", b.attempts());
        }
    }

    #[test]
    fn retry_after_overrides_exponential() {
        let mut b = BackoffState::new();
        b.record_failure();
        b.record_failure();
        b.record_failure();
        // Schedule would be 4s, but Retry-After of 17s wins.
        assert_eq!(
            b.next_delay(Some(Duration::from_secs(17))),
            Duration::from_secs(17)
        );
    }

    #[test]
    fn retry_after_is_not_capped() {
        // Spec: "wait exactly the requested duration" — no cap.
        let mut b = BackoffState::new();
        b.record_failure();
        assert_eq!(
            b.next_delay(Some(Duration::from_secs(3600))),
            Duration::from_secs(3600)
        );
    }

    #[test]
    fn success_resets_attempts() {
        let mut b = BackoffState::new();
        for _ in 0..5 {
            b.record_failure();
        }
        b.record_success();
        b.record_failure();
        assert_eq!(b.next_delay(None), Duration::from_secs(1));
    }
}
