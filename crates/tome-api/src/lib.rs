//! MediaWiki API client.
//!
//! This crate is the **sole gatekeeper** for outbound network traffic. The
//! protections it enforces cannot be bypassed by other crates:
//!
//! - [`rate_limit::TokenBucket`] caps requests at 10/sec (configurable lower
//!   in settings; never higher).
//! - [`backoff::BackoffState`] computes exponential delays (1, 2, 4, 8, 16,
//!   32, capped at 60s) and honors `Retry-After` exactly when present.
//! - [`breaker::CircuitBreaker`] opens after 10 errors in a 60-second window
//!   and stays open for 5 minutes.
//! - [`kill_switch::KillSwitch`] lets the user halt all traffic instantly.
//! - [`log_buffer::RequestLog`] keeps the last 1000 requests for the debug
//!   view.
//! - [`cache::Cache`] short-circuits any URL we've already fetched in this
//!   session — safe because Wikipedia revisions are immutable.
//!
//! Time is modeled with `tokio::time` so tests use `start_paused = true` for
//! deterministic virtual-time assertions without a real network. The
//! [`testing`] module exposes `MockTransport` for fast in-process tests; the
//! [`reqwest_transport::ReqwestTransport`] is the production transport.

pub mod backoff;
pub mod breaker;
pub mod cache;
pub mod client;
pub mod kill_switch;
pub mod log_buffer;
pub mod rate_limit;
pub mod reqwest_transport;
pub mod testing;
pub mod transport;

pub use backoff::BackoffState;
pub use breaker::CircuitBreaker;
pub use cache::Cache;
pub use client::{ClientConfig, MediaWikiClient};
pub use kill_switch::KillSwitch;
pub use log_buffer::{RequestEntry, RequestLog};
pub use rate_limit::TokenBucket;
pub use reqwest_transport::ReqwestTransport;
pub use transport::{HttpError, HttpRequest, HttpResponse, HttpTransport};
