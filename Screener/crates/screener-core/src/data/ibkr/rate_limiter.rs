use std::collections::HashMap;
use std::time::Duration;

use governor::clock::DefaultClock;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Jitter, Quota, RateLimiter as GovernorRateLimiter};
use tokio::sync::Mutex;
use tokio::time::Instant as TokioInstant;

/// Enforces a per-symbol cooldown (IBKR's ~15s identical-request guidance)
/// independent of any global pacing. Pure `tokio::time`-based, so it's
/// deterministically testable with `tokio::time::pause`/`advance` without any
/// dependency on governor's own clock system.
struct SymbolCooldown {
    last_request: Mutex<HashMap<String, TokioInstant>>,
    cooldown: Duration,
}

impl SymbolCooldown {
    fn new(cooldown: Duration) -> Self {
        Self {
            last_request: Mutex::new(HashMap::new()),
            cooldown,
        }
    }

    async fn wait(&self, symbol: &str) {
        let wait_until = {
            let mut last_request = self.last_request.lock().await;
            let now = TokioInstant::now();
            let deadline = last_request
                .get(symbol)
                .map(|last| *last + self.cooldown)
                .filter(|deadline| *deadline > now);
            last_request.insert(symbol.to_string(), deadline.unwrap_or(now));
            deadline
        };
        if let Some(deadline) = wait_until {
            tokio::time::sleep_until(deadline).await;
        }
    }
}

pub struct IbkrRateLimiter {
    global: GovernorRateLimiter<NotKeyed, InMemoryState, DefaultClock>,
    per_symbol: SymbolCooldown,
}

impl IbkrRateLimiter {
    /// ~60 historical-data requests per rolling 10 minutes (one token every
    /// 10 seconds), plus a ~15 second cooldown before repeating a request
    /// for the same symbol.
    pub fn new() -> Self {
        let global_quota = Quota::with_period(Duration::from_secs(10)).expect("10s is a nonzero period");
        Self {
            global: GovernorRateLimiter::direct(global_quota),
            per_symbol: SymbolCooldown::new(Duration::from_secs(15)),
        }
    }

    /// Waits until it's safe to issue an IBKR historical-data request for `symbol`,
    /// respecting both the global pacer and this symbol's cooldown.
    pub async fn acquire(&self, symbol: &str) {
        let jitter = Jitter::up_to(Duration::from_millis(250));
        self.global.until_ready_with_jitter(jitter).await;
        self.per_symbol.wait(symbol).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use governor::clock::FakeRelativeClock;

    #[test]
    fn global_quota_allows_one_immediate_request_then_denies_until_replenished() {
        // Tests the Quota configuration IbkrRateLimiter::new() uses, directly
        // against governor's synchronous check() API with a fake clock —
        // deterministic, no timing races. (async until_ready_with_jitter
        // requires a ReasonablyRealtime clock, which FakeRelativeClock
        // deliberately does not implement, so that path isn't testable this
        // way — see the acquire()-level integration test instead.)
        let clock = FakeRelativeClock::default();
        let quota = Quota::with_period(Duration::from_secs(10)).unwrap();
        let limiter = GovernorRateLimiter::direct_with_clock(quota, clock.clone());

        assert!(limiter.check().is_ok(), "first request should be allowed immediately");
        assert!(limiter.check().is_err(), "second request should be denied before the period elapses");

        clock.advance(Duration::from_secs(10));
        assert!(limiter.check().is_ok(), "request should be allowed again once the period has elapsed");
    }

    #[tokio::test(start_paused = true)]
    async fn symbol_cooldown_permits_the_first_request_for_a_symbol_immediately() {
        let cooldown = SymbolCooldown::new(Duration::from_secs(15));
        let resolved = tokio::time::timeout(Duration::from_millis(1), cooldown.wait("AAPL")).await;
        assert!(resolved.is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn symbol_cooldown_delays_a_second_request_for_the_same_symbol() {
        let cooldown = SymbolCooldown::new(Duration::from_secs(15));
        cooldown.wait("AAPL").await;

        let wait_future = cooldown.wait("AAPL");
        tokio::pin!(wait_future);

        let timed_out = tokio::time::timeout(Duration::from_millis(1), &mut wait_future).await;
        assert!(timed_out.is_err(), "second wait for the same symbol should not resolve instantly");

        tokio::time::advance(Duration::from_secs(15)).await;
        let resolved = tokio::time::timeout(Duration::from_millis(100), wait_future).await;
        assert!(resolved.is_ok(), "second wait should resolve once the cooldown has elapsed");
    }

    #[tokio::test(start_paused = true)]
    async fn symbol_cooldown_treats_different_symbols_independently() {
        let cooldown = SymbolCooldown::new(Duration::from_secs(15));
        cooldown.wait("AAPL").await;

        let resolved = tokio::time::timeout(Duration::from_millis(1), cooldown.wait("MSFT")).await;
        assert!(resolved.is_ok(), "a different symbol should not be blocked by AAPL's cooldown");
    }
}
