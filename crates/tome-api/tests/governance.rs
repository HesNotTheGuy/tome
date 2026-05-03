//! Integration tests for the gatekeeper. Drives `MediaWikiClient` against
//! `MockTransport` with paused tokio time so every governance protection is
//! exercised deterministically and without a real network.

use std::sync::Arc;

use tokio::time::{Duration, Instant};
use tome_api::testing::{MockResponse, MockTransport};
use tome_api::{ClientConfig, KillSwitch, MediaWikiClient};
use tome_core::TomeError;

fn header(name: &str, value: &str) -> (String, String) {
    (name.to_string(), value.to_string())
}

fn config_with_max_attempts(n: u32) -> ClientConfig {
    ClientConfig {
        max_attempts: n,
        ..ClientConfig::default()
    }
}

#[tokio::test(start_paused = true)]
async fn fetch_html_caches_response_so_second_call_skips_transport() {
    let transport = Arc::new(MockTransport::new(vec![MockResponse::ok(
        200,
        b"<html>Photon</html>".to_vec(),
    )]));
    let kill = Arc::new(KillSwitch::new());
    let client = MediaWikiClient::new(ClientConfig::default(), transport.clone(), kill);

    let html1 = client.fetch_html("Photon", None).await.unwrap();
    let html2 = client.fetch_html("Photon", None).await.unwrap();

    assert_eq!(html1, "<html>Photon</html>");
    assert_eq!(html2, "<html>Photon</html>");
    assert_eq!(transport.requests_made(), 1, "second call should hit cache");
}

#[tokio::test(start_paused = true)]
async fn retry_after_overrides_backoff_schedule() {
    let transport = Arc::new(MockTransport::new(vec![
        MockResponse::ok_with_headers(429, vec![header("Retry-After", "17")], Vec::new()),
        MockResponse::ok(200, b"<html>ok</html>".to_vec()),
    ]));
    let kill = Arc::new(KillSwitch::new());
    let client = MediaWikiClient::new(config_with_max_attempts(3), transport.clone(), kill);

    let start = Instant::now();
    let html = client.fetch_html("X", None).await.unwrap();
    let elapsed = Instant::now().saturating_duration_since(start);

    assert_eq!(html, "<html>ok</html>");
    assert!(
        elapsed >= Duration::from_secs(17),
        "expected at least 17s wait, got {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(20),
        "Retry-After must not be capped or extended; got {elapsed:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn exponential_backoff_kicks_in_without_retry_after() {
    // 503, 503, 503, then 200. Backoff: 1, 2, 4 = 7s total.
    let transport = Arc::new(MockTransport::new(vec![
        MockResponse::ok(503, Vec::new()),
        MockResponse::ok(503, Vec::new()),
        MockResponse::ok(503, Vec::new()),
        MockResponse::ok(200, b"ok".to_vec()),
    ]));
    let kill = Arc::new(KillSwitch::new());
    let client = MediaWikiClient::new(config_with_max_attempts(7), transport.clone(), kill);

    let start = Instant::now();
    let html = client.fetch_html("X", None).await.unwrap();
    let elapsed = Instant::now().saturating_duration_since(start);

    assert_eq!(html, "ok");
    assert!(
        elapsed >= Duration::from_secs(7),
        "expected ~7s of backoff (1+2+4), got {elapsed:?}"
    );
    assert!(
        elapsed <= Duration::from_secs(8),
        "backoff overshot expected 7s envelope: {elapsed:?}"
    );
    assert_eq!(transport.requests_made(), 4);
}

#[tokio::test(start_paused = true)]
async fn ten_errors_open_the_breaker() {
    // 11 errors so the 11th call should hit the open breaker rather than the
    // transport. We use shorter max_attempts so each fetch fails quickly.
    let mut scripted = Vec::new();
    for _ in 0..11 {
        scripted.push(MockResponse::ok(500, Vec::new()));
    }
    let transport = Arc::new(MockTransport::new(scripted));
    let kill = Arc::new(KillSwitch::new());
    let client = MediaWikiClient::new(config_with_max_attempts(1), transport.clone(), kill);

    // Fire 10 calls; each is a single attempt (max_attempts=1) and increments
    // breaker by one error.
    for i in 0..10 {
        let title = format!("Page{i}");
        let err = client.fetch_html(&title, None).await.unwrap_err();
        // First 9 should be Api error; the 10th opens the breaker mid-call.
        assert!(matches!(
            err,
            TomeError::Api(_) | TomeError::CircuitBreakerOpen
        ));
    }
    // The 11th call must fail fast with CircuitBreakerOpen and never touch
    // the transport.
    let before_count = transport.requests_made();
    let err = client.fetch_html("Page10", None).await.unwrap_err();
    assert!(matches!(err, TomeError::CircuitBreakerOpen));
    assert_eq!(
        transport.requests_made(),
        before_count,
        "open breaker must not call transport"
    );
    assert!(client.breaker_is_open());
}

#[tokio::test(start_paused = true)]
async fn kill_switch_blocks_all_traffic() {
    let transport = Arc::new(MockTransport::new(vec![MockResponse::ok(
        200,
        b"never reached".to_vec(),
    )]));
    let kill = Arc::new(KillSwitch::new());
    let client = MediaWikiClient::new(ClientConfig::default(), transport.clone(), kill.clone());

    kill.engage();
    let err = client.fetch_html("X", None).await.unwrap_err();
    assert!(matches!(err, TomeError::KillSwitch));
    assert_eq!(transport.requests_made(), 0);

    // Disengaging restores normal operation.
    kill.disengage();
    let html = client.fetch_html("X", None).await.unwrap();
    assert_eq!(html, "never reached");
    assert_eq!(transport.requests_made(), 1);
}

#[tokio::test(start_paused = true)]
async fn rate_limit_paces_concurrent_calls() {
    // 11 responses; 11 sequential fetches at 10rps should take ~1s once
    // the burst capacity is exhausted (the 11th waits 100ms).
    let mut scripted = Vec::new();
    for _ in 0..11 {
        scripted.push(MockResponse::ok(200, Vec::new()));
    }
    let transport = Arc::new(MockTransport::new(scripted));
    let kill = Arc::new(KillSwitch::new());
    let client = MediaWikiClient::new(ClientConfig::default(), transport, kill);

    let start = Instant::now();
    for i in 0..11 {
        // Different titles so we don't hit the cache.
        let title = format!("Page{i}");
        client.fetch_html(&title, None).await.unwrap();
    }
    let elapsed = Instant::now().saturating_duration_since(start);
    assert!(
        elapsed >= Duration::from_millis(95),
        "11th call should wait ~100ms; got {elapsed:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn revision_specific_url_caches_independently() {
    let transport = Arc::new(MockTransport::new(vec![
        MockResponse::ok(200, b"latest".to_vec()),
        MockResponse::ok(200, b"specific revision".to_vec()),
    ]));
    let kill = Arc::new(KillSwitch::new());
    let client = MediaWikiClient::new(ClientConfig::default(), transport.clone(), kill);

    let latest = client.fetch_html("Photon", None).await.unwrap();
    let revid = client.fetch_html("Photon", Some(123)).await.unwrap();
    assert_eq!(latest, "latest");
    assert_eq!(revid, "specific revision");
    assert_eq!(transport.requests_made(), 2);

    // Refetching either should hit cache.
    let latest2 = client.fetch_html("Photon", None).await.unwrap();
    let revid2 = client.fetch_html("Photon", Some(123)).await.unwrap();
    assert_eq!(latest2, "latest");
    assert_eq!(revid2, "specific revision");
    assert_eq!(transport.requests_made(), 2);
}
