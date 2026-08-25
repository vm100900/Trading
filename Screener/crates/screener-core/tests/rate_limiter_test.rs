use std::time::Duration;

use screener_core::data::ibkr::IbkrRateLimiter;

#[tokio::test]
async fn first_acquire_for_a_fresh_limiter_resolves_quickly() {
    let limiter = IbkrRateLimiter::new();

    let start = tokio::time::Instant::now();
    limiter.acquire("AAPL").await;
    // The very first request against an empty global bucket and an
    // unused per-symbol cooldown should not need to wait at all.
    assert!(start.elapsed() < Duration::from_millis(200));
}
