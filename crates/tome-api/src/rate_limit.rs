//! Token-bucket rate limiter using tokio's time abstraction.
//!
//! The bucket starts full (capacity = `rps`), so a fresh client can fire up to
//! `rps` requests immediately as a burst. Subsequent acquires wait for the
//! bucket to refill at `rps` tokens per second.
//!
//! Tests use `#[tokio::test(start_paused = true)]` so virtual time advances
//! only when all tasks are blocked on `tokio::time::sleep`. This makes timing
//! assertions deterministic without slowing the test suite.

use std::sync::Mutex;

use tokio::time::{Duration, Instant};

pub struct TokenBucket {
    capacity: f64,
    refill_per_sec: f64,
    state: Mutex<State>,
}

struct State {
    tokens: f64,
    last_refill: Option<Instant>,
}

impl TokenBucket {
    pub fn new(rps: u32) -> Self {
        let rate = rps as f64;
        Self {
            capacity: rate,
            refill_per_sec: rate,
            state: Mutex::new(State {
                tokens: rate,
                last_refill: None,
            }),
        }
    }

    /// Wait until a token is available, then consume one.
    pub async fn acquire(&self) {
        loop {
            let wait = {
                let mut state = self.state.lock().expect("rate-limit state mutex poisoned");
                let now = Instant::now();
                if let Some(last) = state.last_refill {
                    let elapsed = now.saturating_duration_since(last).as_secs_f64();
                    state.tokens =
                        (state.tokens + elapsed * self.refill_per_sec).min(self.capacity);
                }
                state.last_refill = Some(now);

                if state.tokens >= 1.0 {
                    state.tokens -= 1.0;
                    return;
                }
                let needed = 1.0 - state.tokens;
                Duration::from_secs_f64(needed / self.refill_per_sec)
            };
            tokio::time::sleep(wait).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn burst_consumes_all_capacity_immediately() {
        let bucket = TokenBucket::new(10);
        let start = Instant::now();
        for _ in 0..10 {
            bucket.acquire().await;
        }
        let elapsed = Instant::now().saturating_duration_since(start);
        assert!(
            elapsed < Duration::from_millis(5),
            "burst should be immediate, took {elapsed:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn eleventh_acquire_waits_for_refill() {
        let bucket = TokenBucket::new(10);
        for _ in 0..10 {
            bucket.acquire().await;
        }
        let before = Instant::now();
        bucket.acquire().await;
        let waited = Instant::now().saturating_duration_since(before);
        // 1/10s per token at 10 rps
        assert!(
            waited >= Duration::from_millis(95),
            "expected ~100ms wait, got {waited:?}"
        );
        assert!(
            waited <= Duration::from_millis(150),
            "wait was unexpectedly long: {waited:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn one_hundred_acquires_at_10rps_take_about_nine_seconds() {
        // 10 burst tokens are free; the remaining 90 must wait.
        // 90 acquires at 10/sec = 9 seconds.
        let bucket = TokenBucket::new(10);
        let start = Instant::now();
        for _ in 0..100 {
            bucket.acquire().await;
        }
        let elapsed = Instant::now().saturating_duration_since(start);
        assert!(
            elapsed >= Duration::from_secs_f64(8.9),
            "expected ~9s, got {elapsed:?}"
        );
        assert!(
            elapsed <= Duration::from_secs_f64(9.5),
            "expected ~9s, got {elapsed:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn capacity_does_not_grow_past_rps() {
        // Sleep long enough that the bucket would, naively, accumulate 100
        // tokens. The cap should hold it at `rps` (=10).
        let bucket = TokenBucket::new(10);
        for _ in 0..10 {
            bucket.acquire().await;
        }
        tokio::time::sleep(Duration::from_secs(10)).await;
        // Now we should have at most 10 fresh tokens; the 11th must wait.
        for _ in 0..10 {
            bucket.acquire().await;
        }
        let before = Instant::now();
        bucket.acquire().await;
        let waited = Instant::now().saturating_duration_since(before);
        assert!(
            waited >= Duration::from_millis(95),
            "capacity should be capped at rps; got immediate acquire after long idle"
        );
    }
}
