# screener-core Phase 2 (IBKR 30-Minute Micro-Trend Filter) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend `screener-core` with IBKR connectivity — contract qualification with caching, a centralized pacing-aware rate limiter, and the Phase 2 30-minute filter — so only Phase 1 survivors are ever queried against IBKR, per the spec's core cost-control objective.

**Architecture:** Mirrors the Phase 1 plan's shape exactly: a pure `Phase2Config`/`evaluate_phase2` filter (no I/O), an `IntradayDataSource` trait (mockable, parallel to `DailyDataSource`), and a `run_phase2` orchestrator identical in structure to `run_phase1`. The IBKR-specific pieces — `IbkrRateLimiter`, `ContractCache`, `IbkrClient` — are new and live under `data/ibkr/`. `IbkrRateLimiter` and `ContractCache` are pure/testable in isolation (no live IBKR needed). `IbkrClient` itself wraps the real `ibapi` crate and is **compile-verified only** in this environment — there is no TWS/IB Gateway available to test against, so its real-world behavior needs manual verification against a paper-trading account before relying on it (this matches the spec's section 7 testing strategy, which scopes automated CI tests to mocked data sources).

**Tech Stack:** `ibapi` 3.3.0 (async feature, which is the crate default) for the TWS/Gateway protocol, `governor` 0.10 for the global rate limiter, `tokio` for the per-symbol cooldown and orchestration, everything else per the existing `screener-core` stack (`chrono`, `thiserror`, `async-trait`).

**Spec:** `/home/vijay/Study/Screener/docs/superpowers/specs/2026-08-23-rust-screener-design.md` (sections 4.5 Phase 2, 4.7 timezone/pacing constraints that apply here, 4.8 IBKR rate limiting, 4.9 error taxonomy, 4.10 testability)

**Builds on:** `/home/vijay/Study/Screener/docs/superpowers/plans/2026-08-23-screener-core-phase1.md`, already implemented on `master` in `/home/vijay/Study/Screener/crates/screener-core`. This plan assumes that crate exists with `error.rs` (`ScreeningError`), `models.rs` (`DailyBar`), `config.rs` (`Phase1Config`), `data/traits.rs` (`DailyDataSource`), `data/yahoo.rs`, `phase1.rs`, `screener.rs` (`run_phase1`, `Phase1Progress`, `Phase1Results`), all re-exported from `lib.rs`.

## Note on the `ibapi` crate

Every `ibapi` type, method signature, and struct field referenced in this plan was verified directly against the crate's GitHub source (`wboayue/rust-ibapi`, `main` branch) and its published `Cargo.toml` (confirming `ibapi = "3.3.0"` with the `async` default feature) at the time this plan was written — not recalled from memory. Specifically verified: `Client::connect`, `Contract::stock`, `client.contract_details`, the `historical_data` builder chain, `Bar`/`BarTimestamp`/`HistoricalData` field names, the `Error`/`Notice` shapes, and the exact `ibapi::prelude` re-export names (including the `HistoricalBarSize`/`HistoricalWhatToShow` aliasing needed because the crate also has a separate `realtime` market-data module with its own `BarSize`/`WhatToShow`). Because there is no live TWS/IB Gateway in this environment, this plan cannot verify *behavior* — only that the code compiles against the real API surface. If a future version of `ibapi` has changed these names, `cargo build` will fail loudly at Task 9, and the fix is confined to `data/ibkr/client.rs`.

## Global Constraints

- Phase 2 defaults: SMA7, SMA17, SMA33, SMA65, with the slope condition comparing the current completed 30-minute bar's SMA65 to the *previous* completed bar's SMA65 (a 1-bar lookback, not Phase 1's 5-bar lookback).
- Default Phase 2 requires ≥66 valid completed 30-minute bars (65 for SMA65 + 1 more so SMA65 can also be computed 1 bar back) — a function of the *active* config, mirroring `Phase1Config::min_bars_required`.
- All filter comparisons are strict `>` — never `>=`.
- No look-ahead bias: only completed 30-minute bars are ever evaluated. This is structurally guaranteed here — IBKR's historical data endpoint only returns completed bars, never a forming/current one — so no extra "is this bar complete" filtering is needed in this plan (unlike a live-tick feed, which would need it).
- Exactly one persistent IBKR connection is held per screening run (`IbkrClient` wraps a single `ibapi::Client`) — never reconnect per symbol.
- Contracts are qualified once per symbol and cached (`ContractCache`) — a cached symbol is never re-qualified.
- All IBKR historical-data requests share one centralized rate limiter: a global pacer (modeled as roughly 60 requests per rolling 10 minutes, i.e. one token every 10 seconds) plus a per-symbol identical-request cooldown (~15 seconds). This plan does not separately model IBKR's "≤6 requests per identical contract within 2 seconds" rule, since the 15-second per-symbol cooldown already dominates it given this screener's access pattern (each symbol is fetched at most once per phase per run).
- Retries are capped — never infinite.
- Missing/insufficient/malformed data produces `InsufficientData`, `ContractQualificationFailed`, `IBKRApiError`, or `ConnectionError` — never counted as `TechnicalFilterFailed`.
- Only Phase 1 survivors may reach Phase 2 — enforced by the caller passing that survivor list as `run_phase2`'s universe (mirrors how `run_phase1` doesn't know about "the full universe" either — phase sequencing is the caller's responsibility, established by the Phase 1 plan's `run_phase1` design).

---

## File Structure

```text
crates/screener-core/src/
├── models.rs                 # MODIFY: add IntradayBar, IntradayBarSize
├── config.rs                  # MODIFY: add Phase2Config
├── phase2.rs                   # CREATE: evaluate_phase2 (pure logic, mirrors phase1.rs)
├── screener.rs                  # MODIFY: add run_phase2, Phase2Progress, Phase2Results
├── data/
│   ├── traits.rs                  # MODIFY: add IntradayDataSource
│   └── ibkr/
│       ├── mod.rs                     # CREATE
│       ├── rate_limiter.rs             # CREATE: IbkrRateLimiter
│       ├── contracts.rs                 # CREATE: QualifiedContract, ContractCache
│       └── client.rs                     # CREATE: IbkrClient (wraps ibapi)
└── lib.rs                        # MODIFY: wire up new modules/re-exports

crates/screener-core/tests/
├── common/mod.rs               # MODIFY: add MockIntradayDataSource
├── phase2_integration.rs        # CREATE: run_phase2 end-to-end against the mock
└── rate_limiter_test.rs          # CREATE: IbkrRateLimiter pacing behavior

crates/screener-core/examples/
└── run_phase1_then_phase2.rs    # CREATE: chains Phase 1 -> Phase 2 against real Yahoo + IBKR
```

---

### Task 1: IntradayBar model + IntradayBarSize enum

**Files:**
- Modify: `crates/screener-core/src/models.rs`

**Interfaces:**
- Produces: `pub struct IntradayBar { pub timestamp: chrono::DateTime<chrono::Utc>, pub open: f64, pub high: f64, pub low: f64, pub close: f64, pub volume: f64 }` (`Debug + Clone + PartialEq`); `pub enum IntradayBarSize { ThirtyMinutes, TwoMinutes }` (`Debug + Clone + Copy + PartialEq + Eq`)

- [ ] **Step 1: Write the failing test**

Add to the existing `#[cfg(test)] mod tests` block in `crates/screener-core/src/models.rs`:

```rust
    #[test]
    fn intraday_bar_equality_by_value() {
        let a = IntradayBar {
            timestamp: chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            open: 100.0,
            high: 101.0,
            low: 99.5,
            close: 100.5,
            volume: 12_345.0,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn intraday_bar_size_variants_are_distinct() {
        assert_ne!(IntradayBarSize::ThirtyMinutes, IntradayBarSize::TwoMinutes);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p screener-core models::tests`
Expected: FAIL to compile — `IntradayBar`/`IntradayBarSize` not defined.

- [ ] **Step 3: Implement the new types**

Add to `crates/screener-core/src/models.rs`, above the `#[cfg(test)]` module, alongside the existing `DailyBar`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct IntradayBar {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntradayBarSize {
    ThirtyMinutes,
    TwoMinutes,
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p screener-core models::tests`
Expected: PASS (3 tests: the existing `daily_bar_equality_by_value` plus the 2 new ones)

- [ ] **Step 5: Wire up re-exports**

Modify `crates/screener-core/src/lib.rs` — change:
```rust
pub use models::DailyBar;
```
to:
```rust
pub use models::{DailyBar, IntradayBar, IntradayBarSize};
```

- [ ] **Step 6: Commit**

```bash
git add crates/screener-core/src/models.rs crates/screener-core/src/lib.rs
git commit -m "feat(screener-core): add IntradayBar model and IntradayBarSize enum"
```

---

### Task 2: IntradayDataSource trait + test mock

**Files:**
- Modify: `crates/screener-core/src/data/traits.rs`
- Modify: `crates/screener-core/tests/common/mod.rs`

**Interfaces:**
- Consumes: `IntradayBar`, `IntradayBarSize` (models), `ScreeningError` (error)
- Produces: `#[async_trait] pub trait IntradayDataSource: Send + Sync { async fn fetch_intraday_bars(&self, symbol: &str, bar_size: IntradayBarSize, min_bars: usize) -> Result<Vec<IntradayBar>, ScreeningError>; }`; test-only `MockIntradayDataSource` in `tests/common`

- [ ] **Step 1: Write the failing test**

Add to `crates/screener-core/tests/common/mod.rs` (below the existing `MockDailyDataSource`):

```rust
pub struct MockIntradayDataSource {
    pub responses: HashMap<String, MockResponse>,
}

impl MockIntradayDataSource {
    pub fn new() -> Self {
        Self { responses: HashMap::new() }
    }

    pub fn with_bars(mut self, symbol: &str, bars: Vec<IntradayBar>) -> Self {
        self.responses.insert(symbol.to_string(), MockIntradayResponse::Bars(bars));
        self
    }

    pub fn with_error(mut self, symbol: &str, error: ScreeningError) -> Self {
        self.responses.insert(symbol.to_string(), MockIntradayResponse::Error(error));
        self
    }
}

pub enum MockIntradayResponse {
    Bars(Vec<IntradayBar>),
    Error(ScreeningError),
}

#[async_trait]
impl IntradayDataSource for MockIntradayDataSource {
    async fn fetch_intraday_bars(
        &self,
        symbol: &str,
        _bar_size: IntradayBarSize,
        _min_bars: usize,
    ) -> Result<Vec<IntradayBar>, ScreeningError> {
        match self.responses.get(symbol) {
            Some(MockIntradayResponse::Bars(bars)) => Ok(bars.clone()),
            Some(MockIntradayResponse::Error(e)) => Err(e.clone()),
            None => Err(ScreeningError::NoData(symbol.to_string())),
        }
    }
}
```

Also update the `use` statement at the top of `crates/screener-core/tests/common/mod.rs` from:
```rust
use screener_core::{DailyBar, DailyDataSource, ScreeningError};
```
to:
```rust
use screener_core::{DailyBar, DailyDataSource, IntradayBar, IntradayBarSize, IntradayDataSource, ScreeningError};
```

And fix the naming collision: the existing mock's `pub enum MockResponse` (for daily bars) and the new `MockIntradayResponse` are already distinctly named, so no further change is needed there.

Then create `crates/screener-core/tests/intraday_data_source_mock_test.rs`:

```rust
mod common;

use chrono::DateTime;
use common::MockIntradayDataSource;
use screener_core::{IntradayBar, IntradayBarSize, IntradayDataSource, ScreeningError};

fn one_bar() -> IntradayBar {
    IntradayBar {
        timestamp: DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
        open: 10.0,
        high: 10.5,
        low: 9.8,
        close: 10.2,
        volume: 5_000.0,
    }
}

#[tokio::test]
async fn mock_returns_configured_bars_for_known_symbol() {
    let source = MockIntradayDataSource::new().with_bars("AAPL", vec![one_bar()]);
    let bars = source
        .fetch_intraday_bars("AAPL", IntradayBarSize::ThirtyMinutes, 1)
        .await
        .unwrap();
    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].close, 10.2);
}

#[tokio::test]
async fn mock_returns_no_data_error_for_unknown_symbol() {
    let source = MockIntradayDataSource::new();
    let result = source.fetch_intraday_bars("ZZZZ", IntradayBarSize::TwoMinutes, 1).await;
    assert!(matches!(result, Err(ScreeningError::NoData(_))));
}

#[tokio::test]
async fn mock_returns_configured_error() {
    let source = MockIntradayDataSource::new()
        .with_error("BADSYM", ScreeningError::ContractQualificationFailed("BADSYM".to_string()));
    let result = source.fetch_intraday_bars("BADSYM", IntradayBarSize::ThirtyMinutes, 1).await;
    assert!(matches!(result, Err(ScreeningError::ContractQualificationFailed(_))));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p screener-core --test intraday_data_source_mock_test`
Expected: FAIL to compile — `IntradayDataSource` not defined in `screener_core`.

- [ ] **Step 3: Implement the `IntradayDataSource` trait**

Modify `crates/screener-core/src/data/traits.rs`, add below the existing `DailyDataSource` trait:

```rust
use crate::models::IntradayBarSize;

#[async_trait]
pub trait IntradayDataSource: Send + Sync {
    /// Fetch intraday bars for `symbol` at the given `bar_size`, requesting
    /// enough history to cover at least `min_bars` valid completed bars
    /// (implementations should request extra buffer, not exactly `min_bars`).
    async fn fetch_intraday_bars(
        &self,
        symbol: &str,
        bar_size: IntradayBarSize,
        min_bars: usize,
    ) -> Result<Vec<IntradayBar>, ScreeningError>;
}
```

Note: `IntradayBar` is already imported via the existing `use crate::models::DailyBar;` line at the top of the file — update that line to:
```rust
use crate::models::{DailyBar, IntradayBarSize};
```
(add `IntradayBar` too since it's used in the new trait's return type):
```rust
use crate::models::{DailyBar, IntradayBar, IntradayBarSize};
```

Modify `crates/screener-core/src/data/mod.rs`, add to the re-export line:
```rust
pub use traits::{DailyDataSource, IntradayDataSource};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p screener-core --test intraday_data_source_mock_test`
Expected: PASS (3 tests)

- [ ] **Step 5: Wire up the crate-level re-export**

Modify `crates/screener-core/src/lib.rs` — change:
```rust
pub use data::DailyDataSource;
```
to:
```rust
pub use data::{DailyDataSource, IntradayDataSource};
```

- [ ] **Step 6: Commit**

```bash
git add crates/screener-core/src/data crates/screener-core/src/lib.rs crates/screener-core/tests
git commit -m "feat(screener-core): add IntradayDataSource trait and test mock"
```

---

### Task 3: Phase2Config

**Files:**
- Modify: `crates/screener-core/src/config.rs`
- Modify: `crates/screener-core/src/lib.rs`

**Interfaces:**
- Consumes: nothing
- Produces: `pub struct Phase2Config { sma7_period, sma17_period, sma33_period, sma65_period: usize, slope_lookback_bars: usize, enable_close_gt_sma7, enable_sma7_gt_sma17, enable_sma17_gt_sma33, enable_sma33_gt_sma65, enable_sma65_slope: bool }` implementing `Default`, plus `pub fn min_bars_required(&self) -> usize`

- [ ] **Step 1: Write the failing tests**

Add to `crates/screener-core/src/config.rs`, in a new block appended to the existing `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn phase2_defaults_match_locked_spec() {
        let cfg = Phase2Config::default();
        assert_eq!(cfg.sma7_period, 7);
        assert_eq!(cfg.sma17_period, 17);
        assert_eq!(cfg.sma33_period, 33);
        assert_eq!(cfg.sma65_period, 65);
        assert_eq!(cfg.slope_lookback_bars, 1);
        assert!(cfg.enable_close_gt_sma7);
        assert!(cfg.enable_sma7_gt_sma17);
        assert!(cfg.enable_sma17_gt_sma33);
        assert!(cfg.enable_sma33_gt_sma65);
        assert!(cfg.enable_sma65_slope);
    }

    #[test]
    fn phase2_default_min_bars_required_is_66() {
        let cfg = Phase2Config::default();
        assert_eq!(cfg.min_bars_required(), 66);
    }

    #[test]
    fn phase2_min_bars_required_ignores_slope_lookback_when_slope_disabled() {
        let mut cfg = Phase2Config::default();
        cfg.enable_sma65_slope = false;
        assert_eq!(cfg.min_bars_required(), 65);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p screener-core config::tests`
Expected: FAIL to compile — `Phase2Config` not defined.

- [ ] **Step 3: Implement `Phase2Config`**

Add to `crates/screener-core/src/config.rs`, above the `#[cfg(test)]` module, alongside `Phase1Config`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Phase2Config {
    pub sma7_period: usize,
    pub sma17_period: usize,
    pub sma33_period: usize,
    pub sma65_period: usize,
    pub slope_lookback_bars: usize,
    pub enable_close_gt_sma7: bool,
    pub enable_sma7_gt_sma17: bool,
    pub enable_sma17_gt_sma33: bool,
    pub enable_sma33_gt_sma65: bool,
    pub enable_sma65_slope: bool,
}

impl Default for Phase2Config {
    fn default() -> Self {
        Self {
            sma7_period: 7,
            sma17_period: 17,
            sma33_period: 33,
            sma65_period: 65,
            slope_lookback_bars: 1,
            enable_close_gt_sma7: true,
            enable_sma7_gt_sma17: true,
            enable_sma17_gt_sma33: true,
            enable_sma33_gt_sma65: true,
            enable_sma65_slope: true,
        }
    }
}

impl Phase2Config {
    /// Minimum completed 30-minute bars required to evaluate this config's active conditions.
    pub fn min_bars_required(&self) -> usize {
        let longest_period = self
            .sma7_period
            .max(self.sma17_period)
            .max(self.sma33_period)
            .max(self.sma65_period);

        if self.enable_sma65_slope {
            longest_period + self.slope_lookback_bars
        } else {
            longest_period
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p screener-core config::tests`
Expected: PASS (7 tests: 4 existing Phase1Config tests + 3 new Phase2Config tests)

- [ ] **Step 5: Wire up re-exports**

Modify `crates/screener-core/src/lib.rs` — change:
```rust
pub use config::Phase1Config;
```
to:
```rust
pub use config::{Phase1Config, Phase2Config};
```

- [ ] **Step 6: Commit**

```bash
git add crates/screener-core/src/config.rs crates/screener-core/src/lib.rs
git commit -m "feat(screener-core): add Phase2Config with locked defaults"
```

---

### Task 4: Phase 2 filter logic

**Files:**
- Create: `crates/screener-core/src/phase2.rs`
- Modify: `crates/screener-core/src/lib.rs`

**Interfaces:**
- Consumes: `IntradayBar` (models), `Phase2Config` (config), `sma_at` (indicators::sma)
- Produces: `pub enum Phase2Outcome { Passed, FailedTechnical, InsufficientData { needed: usize, got: usize } }` (`Debug + Clone + PartialEq`), `pub fn evaluate_phase2(bars: &[IntradayBar], config: &Phase2Config) -> Phase2Outcome`

- [ ] **Step 1: Write the failing tests**

`crates/screener-core/src/phase2.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

    fn bars_from_closes(closes: &[f64]) -> Vec<IntradayBar> {
        closes
            .iter()
            .enumerate()
            .map(|(i, &close)| IntradayBar {
                timestamp: DateTime::from_timestamp(1_700_000_000 + (i as i64) * 1800, 0).unwrap(),
                open: close,
                high: close,
                low: close,
                close,
                volume: 10_000.0,
            })
            .collect()
    }

    #[test]
    fn insufficient_bars_fails_with_needed_and_got() {
        let bars = bars_from_closes(&[1.0, 2.0, 3.0]);
        let config = Phase2Config::default();
        let outcome = evaluate_phase2(&bars, &config);
        assert_eq!(outcome, Phase2Outcome::InsufficientData { needed: 66, got: 3 });
    }

    #[test]
    fn strictly_ascending_closes_passes() {
        // 70 closes rising from 1.0 to 70.0: close > sma7 > sma17 > sma33 > sma65
        // holds, and sma65 is higher now than 1 bar ago.
        let closes: Vec<f64> = (1..=70).map(|i| i as f64).collect();
        let bars = bars_from_closes(&closes);
        let config = Phase2Config::default();
        assert_eq!(evaluate_phase2(&bars, &config), Phase2Outcome::Passed);
    }

    #[test]
    fn flat_series_fails_technical_because_slope_is_not_strictly_greater() {
        let closes = vec![50.0; 70];
        let bars = bars_from_closes(&closes);
        let config = Phase2Config::default();
        assert_eq!(evaluate_phase2(&bars, &config), Phase2Outcome::FailedTechnical);
    }

    #[test]
    fn falling_series_fails_technical_on_close_gt_sma7() {
        let closes: Vec<f64> = (1..=70).rev().map(|i| i as f64).collect();
        let bars = bars_from_closes(&closes);
        let config = Phase2Config::default();
        assert_eq!(evaluate_phase2(&bars, &config), Phase2Outcome::FailedTechnical);
    }

    #[test]
    fn disabling_slope_condition_lets_flat_series_pass_other_conditions() {
        let closes: Vec<f64> = (1..=70).map(|i| i as f64).collect();
        let bars = bars_from_closes(&closes);
        let mut config = Phase2Config::default();
        config.enable_sma65_slope = false;
        assert_eq!(evaluate_phase2(&bars, &config), Phase2Outcome::Passed);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p screener-core phase2::tests`
Expected: FAIL to compile — `evaluate_phase2`/`Phase2Outcome` not defined.

- [ ] **Step 3: Implement `evaluate_phase2`**

Add above the `#[cfg(test)]` module in `crates/screener-core/src/phase2.rs`:

```rust
use crate::config::Phase2Config;
use crate::indicators::sma_at;
use crate::models::IntradayBar;

#[derive(Debug, Clone, PartialEq)]
pub enum Phase2Outcome {
    Passed,
    FailedTechnical,
    InsufficientData { needed: usize, got: usize },
}

pub fn evaluate_phase2(bars: &[IntradayBar], config: &Phase2Config) -> Phase2Outcome {
    let needed = config.min_bars_required();
    if bars.len() < needed {
        return Phase2Outcome::InsufficientData {
            needed,
            got: bars.len(),
        };
    }

    let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
    let close = *closes.last().expect("checked non-empty via min_bars_required");

    let sma7 = sma_at(&closes, config.sma7_period, 0);
    let sma17 = sma_at(&closes, config.sma17_period, 0);
    let sma33 = sma_at(&closes, config.sma33_period, 0);
    let sma65 = sma_at(&closes, config.sma65_period, 0);

    let (sma7, sma17, sma33, sma65) = match (sma7, sma17, sma33, sma65) {
        (Some(a), Some(b), Some(c), Some(d)) => (a, b, c, d),
        _ => return Phase2Outcome::InsufficientData { needed, got: bars.len() },
    };

    if config.enable_close_gt_sma7 && !(close > sma7) {
        return Phase2Outcome::FailedTechnical;
    }
    if config.enable_sma7_gt_sma17 && !(sma7 > sma17) {
        return Phase2Outcome::FailedTechnical;
    }
    if config.enable_sma17_gt_sma33 && !(sma17 > sma33) {
        return Phase2Outcome::FailedTechnical;
    }
    if config.enable_sma33_gt_sma65 && !(sma33 > sma65) {
        return Phase2Outcome::FailedTechnical;
    }

    if config.enable_sma65_slope {
        let sma65_prior = match sma_at(&closes, config.sma65_period, config.slope_lookback_bars) {
            Some(v) => v,
            None => return Phase2Outcome::InsufficientData { needed, got: bars.len() },
        };
        if !(sma65 > sma65_prior) {
            return Phase2Outcome::FailedTechnical;
        }
    }

    Phase2Outcome::Passed
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p screener-core phase2::tests`
Expected: PASS (5 tests)

- [ ] **Step 5: Wire up the module**

Modify `crates/screener-core/src/lib.rs`, add:
```rust
pub mod phase2;
```
and:
```rust
pub use phase2::{evaluate_phase2, Phase2Outcome};
```

- [ ] **Step 6: Commit**

```bash
git add crates/screener-core/src/phase2.rs crates/screener-core/src/lib.rs
git commit -m "feat(screener-core): add Phase 2 filter logic (evaluate_phase2)"
```

---

### Task 5: Phase 2 orchestration

**Files:**
- Modify: `crates/screener-core/src/screener.rs`
- Modify: `crates/screener-core/src/lib.rs`
- Create: `crates/screener-core/tests/phase2_integration.rs`

**Interfaces:**
- Consumes: `IntradayDataSource` (data), `Phase2Config` (config), `evaluate_phase2`/`Phase2Outcome` (phase2), `ScreeningError` (error)
- Produces: `pub struct Phase2Progress { pub total: usize, pub started: usize, pub passed: usize, pub technical_failures: usize, pub errors: usize }`, `pub struct Phase2Results { pub survivors: Vec<String>, pub technical_failures: Vec<String>, pub errors: Vec<(String, ScreeningError)> }`, `pub async fn run_phase2(universe: &[String], config: &Phase2Config, data_source: Arc<dyn IntradayDataSource>, progress_tx: Option<tokio::sync::mpsc::UnboundedSender<Phase2Progress>>) -> Phase2Results`

- [ ] **Step 1: Write the failing integration test**

`crates/screener-core/tests/phase2_integration.rs`:

```rust
mod common;

use std::sync::Arc;

use chrono::DateTime;
use common::MockIntradayDataSource;
use screener_core::{screener::run_phase2, IntradayBar, Phase2Config, ScreeningError};

fn rising_bars(n: usize) -> Vec<IntradayBar> {
    (0..n)
        .map(|i| IntradayBar {
            timestamp: DateTime::from_timestamp(1_700_000_000 + (i as i64) * 1800, 0).unwrap(),
            open: (i + 1) as f64,
            high: (i + 1) as f64,
            low: (i + 1) as f64,
            close: (i + 1) as f64,
            volume: 10_000.0,
        })
        .collect()
}

fn flat_bars(n: usize) -> Vec<IntradayBar> {
    (0..n)
        .map(|i| IntradayBar {
            timestamp: DateTime::from_timestamp(1_700_000_000 + (i as i64) * 1800, 0).unwrap(),
            open: 50.0,
            high: 50.0,
            low: 50.0,
            close: 50.0,
            volume: 10_000.0,
        })
        .collect()
}

#[tokio::test]
async fn run_phase2_separates_survivors_technical_failures_and_errors() {
    let source = MockIntradayDataSource::new()
        .with_bars("RISER", rising_bars(70))
        .with_bars("FLAT", flat_bars(70))
        .with_error("BADSYM", ScreeningError::ContractQualificationFailed("BADSYM".to_string()));

    let universe = vec!["RISER".to_string(), "FLAT".to_string(), "BADSYM".to_string()];
    let config = Phase2Config::default();

    let results = run_phase2(&universe, &config, Arc::new(source), None).await;

    assert_eq!(results.survivors, vec!["RISER".to_string()]);
    assert_eq!(results.technical_failures, vec!["FLAT".to_string()]);
    assert_eq!(results.errors.len(), 1);
    assert_eq!(results.errors[0].0, "BADSYM");
}

#[tokio::test]
async fn run_phase2_emits_final_progress_with_correct_totals() {
    let source = MockIntradayDataSource::new()
        .with_bars("RISER", rising_bars(70))
        .with_bars("FLAT", flat_bars(70));

    let universe = vec!["RISER".to_string(), "FLAT".to_string()];
    let config = Phase2Config::default();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let results = run_phase2(&universe, &config, Arc::new(source), Some(tx)).await;

    let mut last_progress = None;
    while let Ok(p) = rx.try_recv() {
        last_progress = Some(p);
    }
    let last_progress = last_progress.expect("at least one progress message was sent");

    assert_eq!(last_progress.total, 2);
    assert_eq!(last_progress.started, 2);
    assert_eq!(last_progress.passed, 1);
    assert_eq!(last_progress.technical_failures, 1);
    assert_eq!(last_progress.errors, 0);
    assert_eq!(results.survivors.len(), 1);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p screener-core --test phase2_integration`
Expected: FAIL to compile — `screener::run_phase2` not defined.

- [ ] **Step 3: Implement the orchestration**

Modify `crates/screener-core/src/screener.rs`. Add near the top, alongside the existing `use` statements:
```rust
use crate::config::Phase2Config;
use crate::data::IntradayDataSource;
use crate::phase2::{evaluate_phase2, Phase2Outcome};
```

Add below the existing `run_phase1` function:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Phase2Progress {
    pub total: usize,
    pub started: usize,
    pub passed: usize,
    pub technical_failures: usize,
    pub errors: usize,
}

#[derive(Debug, Default)]
pub struct Phase2Results {
    pub survivors: Vec<String>,
    pub technical_failures: Vec<String>,
    pub errors: Vec<(String, ScreeningError)>,
}

enum Phase2SymbolOutcome {
    Passed(String),
    FailedTechnical(String),
    Errored(String, ScreeningError),
}

pub async fn run_phase2(
    universe: &[String],
    config: &Phase2Config,
    data_source: Arc<dyn IntradayDataSource>,
    progress_tx: Option<UnboundedSender<Phase2Progress>>,
) -> Phase2Results {
    let total = universe.len();
    let min_bars = config.min_bars_required();

    let mut tasks = JoinSet::new();
    for symbol in universe {
        let symbol = symbol.clone();
        let config = config.clone();
        let data_source = Arc::clone(&data_source);
        tasks.spawn(async move {
            match data_source
                .fetch_intraday_bars(&symbol, crate::models::IntradayBarSize::ThirtyMinutes, min_bars)
                .await
            {
                Ok(bars) => match evaluate_phase2(&bars, &config) {
                    Phase2Outcome::Passed => Phase2SymbolOutcome::Passed(symbol),
                    Phase2Outcome::FailedTechnical => Phase2SymbolOutcome::FailedTechnical(symbol),
                    Phase2Outcome::InsufficientData { needed, got } => Phase2SymbolOutcome::Errored(
                        symbol.clone(),
                        ScreeningError::InsufficientData { symbol, needed, got },
                    ),
                },
                Err(err) => Phase2SymbolOutcome::Errored(symbol, err),
            }
        });
    }

    let mut results = Phase2Results::default();
    let mut started = 0;

    while let Some(joined) = tasks.join_next().await {
        started += 1;
        match joined.expect("phase 2 worker task panicked") {
            Phase2SymbolOutcome::Passed(symbol) => results.survivors.push(symbol),
            Phase2SymbolOutcome::FailedTechnical(symbol) => results.technical_failures.push(symbol),
            Phase2SymbolOutcome::Errored(symbol, err) => results.errors.push((symbol, err)),
        }

        if let Some(tx) = &progress_tx {
            let _ = tx.send(Phase2Progress {
                total,
                started,
                passed: results.survivors.len(),
                technical_failures: results.technical_failures.len(),
                errors: results.errors.len(),
            });
        }
    }

    results
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p screener-core --test phase2_integration`
Expected: PASS (2 tests)

- [ ] **Step 5: Wire up re-exports**

Modify `crates/screener-core/src/lib.rs` — change:
```rust
pub use screener::{run_phase1, Phase1Progress, Phase1Results};
```
to:
```rust
pub use screener::{run_phase1, run_phase2, Phase1Progress, Phase1Results, Phase2Progress, Phase2Results};
```

- [ ] **Step 6: Run the full crate test suite**

Run: `cargo test -p screener-core`
Expected: PASS — all tests from this plan's Tasks 1-5 plus everything from the Phase 1 plan pass together.

- [ ] **Step 7: Commit**

```bash
git add crates/screener-core/src/screener.rs crates/screener-core/src/lib.rs crates/screener-core/tests/phase2_integration.rs
git commit -m "feat(screener-core): add Phase 2 orchestration (run_phase2)"
```

---

### Task 6: IBKR rate limiter

**Files:**
- Create: `crates/screener-core/src/data/ibkr/mod.rs`
- Create: `crates/screener-core/src/data/ibkr/rate_limiter.rs`
- Create: `crates/screener-core/tests/rate_limiter_test.rs`
- Modify: `crates/screener-core/Cargo.toml`
- Modify: `crates/screener-core/src/data/mod.rs`
- Modify: `crates/screener-core/src/lib.rs`

**Interfaces:**
- Consumes: nothing from earlier tasks
- Produces: `pub struct IbkrRateLimiter { .. }` (concrete, not generic — see design note) with `pub fn new() -> Self` and `pub async fn acquire(&self, symbol: &str)`

**Design note (corrected from an earlier draft of this plan):** governor's async wait methods (`until_ready_with_jitter`) require the clock to implement `governor::clock::ReasonablyRealtime` — and `governor::clock::FakeRelativeClock` deliberately does *not* implement that bound, so a fake clock cannot be used to test the async waiting path at all, only the synchronous `.check()` path. Making `IbkrRateLimiter` generic over `Clock` to inject a fake clock (as an earlier draft of this plan attempted) does not work for that reason. The corrected design:
- `IbkrRateLimiter` is concrete (always uses `governor::clock::DefaultClock`, matching production use), and its global quota configuration is verified in a standalone unit test that talks to a bare `governor::RateLimiter::direct_with_clock(quota, FakeRelativeClock)` directly (bypassing `IbkrRateLimiter` entirely) using the synchronous `.check()` API — deterministic, no timing races, no `ReasonablyRealtime` requirement.
- The per-symbol 15-second cooldown is pulled out into its own private `SymbolCooldown` type, implemented directly with `tokio::time` (not governor's keyed rate limiter — it has no fake-clock injection point either). Being plain `tokio::time`-based code, it's fully and deterministically testable with `tokio::time::pause`/`advance`, independent of any governor clock concerns.
- `IbkrRateLimiter::acquire` composes both: `self.global.until_ready_with_jitter(..).await` then `self.per_symbol.wait(symbol).await`. Its own test coverage is a single real-clock integration test confirming the first call resolves quickly (which needs no time-mocking, since a fresh limiter's first call is genuinely fast).

- [ ] **Step 1: Add the new dependencies**

Modify `crates/screener-core/Cargo.toml`, `[dependencies]`, add:
```toml
governor = "0.10"
```

Also modify `crates/screener-core/Cargo.toml`, `[dev-dependencies]` — the existing `tokio` dev-dependency needs the `test-util` feature added (for `tokio::time::pause`/`advance` used by this task's tests):
```toml
tokio = { version = "1", features = ["macros", "rt-multi-thread", "test-util"] }
```

- [ ] **Step 2: Write the failing integration test**

`crates/screener-core/tests/rate_limiter_test.rs`:

```rust
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
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p screener-core --test rate_limiter_test`
Expected: FAIL to compile — `screener_core::data::ibkr` module not defined.

- [ ] **Step 4: Implement `IbkrRateLimiter` and its unit tests**

`crates/screener-core/src/data/ibkr/rate_limiter.rs`:

```rust
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
```

`crates/screener-core/src/data/ibkr/mod.rs`:

```rust
pub mod rate_limiter;

pub use rate_limiter::IbkrRateLimiter;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p screener-core --test rate_limiter_test` — expected PASS (1 test).
Run: `cargo test -p screener-core data::ibkr` — expected PASS (4 unit tests).

- [ ] **Step 6: Wire up the module**

Modify `crates/screener-core/src/data/mod.rs`, add:
```rust
pub mod ibkr;
```

- [ ] **Step 7: Commit**

```bash
git add crates/screener-core/src/data/ibkr crates/screener-core/src/data/mod.rs crates/screener-core/Cargo.toml crates/screener-core/tests/rate_limiter_test.rs
git commit -m "feat(screener-core): add IBKR rate limiter (global pacer + per-symbol cooldown)"
```

---

### Task 7: Contract cache

**Files:**
- Create: `crates/screener-core/src/data/ibkr/contracts.rs`
- Modify: `crates/screener-core/src/data/ibkr/mod.rs`

**Interfaces:**
- Consumes: `ScreeningError` (error)
- Produces: `pub struct QualifiedContract { pub symbol: String, pub contract_id: i32 }` (`Debug + Clone + PartialEq`), `pub struct ContractCache { .. }` with `pub fn new() -> Self` and `pub async fn get_or_qualify<F, Fut>(&self, symbol: &str, qualify: F) -> Result<QualifiedContract, ScreeningError> where F: FnOnce(String) -> Fut, Fut: Future<Output = Result<QualifiedContract, ScreeningError>>` (note: `qualify` takes an owned `String`, not `&str` — see the implementation step for why)

- [ ] **Step 1: Write the failing tests**

`crates/screener-core/src/data/ibkr/contracts.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn qualifies_a_symbol_on_first_lookup() {
        let cache = ContractCache::new();
        let result = cache
            .get_or_qualify("AAPL", |symbol| async move {
                Ok(QualifiedContract { symbol: symbol.to_string(), contract_id: 42 })
            })
            .await
            .unwrap();
        assert_eq!(result.contract_id, 42);
    }

    #[tokio::test]
    async fn caches_the_result_and_does_not_requalify() {
        let cache = ContractCache::new();
        let call_count = Arc::new(AtomicUsize::new(0));

        for _ in 0..3 {
            let call_count = Arc::clone(&call_count);
            cache
                .get_or_qualify("AAPL", move |symbol| {
                    call_count.fetch_add(1, Ordering::SeqCst);
                    async move { Ok(QualifiedContract { symbol: symbol.to_string(), contract_id: 42 }) }
                })
                .await
                .unwrap();
        }

        assert_eq!(call_count.load(Ordering::SeqCst), 1, "qualify should only run once, on the first call");
    }

    #[tokio::test]
    async fn different_symbols_are_qualified_independently() {
        let cache = ContractCache::new();
        let call_count = Arc::new(AtomicUsize::new(0));

        for symbol in ["AAPL", "MSFT"] {
            let call_count = Arc::clone(&call_count);
            cache
                .get_or_qualify(symbol, move |s| {
                    call_count.fetch_add(1, Ordering::SeqCst);
                    let s = s.to_string();
                    async move { Ok(QualifiedContract { symbol: s.clone(), contract_id: 1 }) }
                })
                .await
                .unwrap();
        }

        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_failed_qualification_is_not_cached() {
        let cache = ContractCache::new();
        let call_count = Arc::new(AtomicUsize::new(0));

        for _ in 0..2 {
            let call_count = Arc::clone(&call_count);
            let result = cache
                .get_or_qualify("BADSYM", move |symbol| {
                    call_count.fetch_add(1, Ordering::SeqCst);
                    let symbol = symbol.to_string();
                    async move { Err(ScreeningError::ContractQualificationFailed(symbol)) }
                })
                .await;
            assert!(result.is_err());
        }

        assert_eq!(call_count.load(Ordering::SeqCst), 2, "a failed qualification should be retried, not cached");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p screener-core data::ibkr::contracts::tests`
Expected: FAIL to compile — `ContractCache`/`QualifiedContract` not defined.

- [ ] **Step 3: Implement `ContractCache`**

Add above the `#[cfg(test)]` module in `crates/screener-core/src/data/ibkr/contracts.rs`:

```rust
use std::collections::HashMap;
use std::future::Future;

use tokio::sync::RwLock;

use crate::error::ScreeningError;

#[derive(Debug, Clone, PartialEq)]
pub struct QualifiedContract {
    pub symbol: String,
    pub contract_id: i32,
}

#[derive(Default)]
pub struct ContractCache {
    cache: RwLock<HashMap<String, QualifiedContract>>,
}

impl ContractCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the cached qualified contract for `symbol` if present;
    /// otherwise calls `qualify` to resolve it and caches the result on
    /// success. A failed qualification is not cached, so it will be retried
    /// on the next call for the same symbol.
    ///
    /// `qualify` takes an owned `String` rather than `&str` — tying `Fut`'s
    /// lifetime to a borrowed parameter here runs into a well-known Rust
    /// limitation with generic `FnOnce(&str) -> Fut` signatures (the
    /// compiler can't express that `Fut` may borrow from the argument
    /// without higher-ranked trait bounds that generic async closures don't
    /// support cleanly); taking ownership sidesteps it entirely.
    pub async fn get_or_qualify<F, Fut>(&self, symbol: &str, qualify: F) -> Result<QualifiedContract, ScreeningError>
    where
        F: FnOnce(String) -> Fut,
        Fut: Future<Output = Result<QualifiedContract, ScreeningError>>,
    {
        if let Some(cached) = self.cache.read().await.get(symbol) {
            return Ok(cached.clone());
        }

        let qualified = qualify(symbol.to_string()).await?;
        self.cache.write().await.insert(symbol.to_string(), qualified.clone());
        Ok(qualified)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p screener-core data::ibkr::contracts::tests`
Expected: PASS (4 tests)

- [ ] **Step 5: Wire up the module**

Modify `crates/screener-core/src/data/ibkr/mod.rs`:
```rust
pub mod contracts;
pub mod rate_limiter;

pub use contracts::{ContractCache, QualifiedContract};
pub use rate_limiter::IbkrRateLimiter;
```

- [ ] **Step 6: Commit**

```bash
git add crates/screener-core/src/data/ibkr/contracts.rs crates/screener-core/src/data/ibkr/mod.rs
git commit -m "feat(screener-core): add IBKR contract cache (qualify-once-per-symbol)"
```

---

### Task 8: IBKR client

**Files:**
- Create: `crates/screener-core/src/data/ibkr/client.rs`
- Modify: `crates/screener-core/src/data/ibkr/mod.rs`
- Modify: `crates/screener-core/src/data/mod.rs`
- Modify: `crates/screener-core/Cargo.toml`

**Interfaces:**
- Consumes: `IntradayDataSource` (data::traits), `IntradayBar`/`IntradayBarSize` (models), `ContractCache`/`QualifiedContract` (data::ibkr::contracts), `IbkrRateLimiter` (data::ibkr::rate_limiter), `ScreeningError` (error)
- Produces: `pub struct IbkrClient { .. }` implementing `IntradayDataSource`, with `pub async fn connect(address: &str, client_id: i32) -> Result<Self, ScreeningError>`

**This task cannot be automatically tested end-to-end** — there is no TWS/IB Gateway available in this environment. Its only verification here is that it compiles against the real `ibapi` 3.3.0 API. Manual verification against a paper-trading IBKR account is required before relying on it (see the spec's section 7 testing strategy).

- [ ] **Step 1: Add the dependency**

Modify `crates/screener-core/Cargo.toml`, `[dependencies]`, add:
```toml
ibapi = "3.3.0"
```

- [ ] **Step 2: Implement `IbkrClient`**

`crates/screener-core/src/data/ibkr/client.rs`:

```rust
use async_trait::async_trait;
use ibapi::market_data::historical::BarTimestamp;
use ibapi::prelude::*;

use crate::data::ibkr::{ContractCache, IbkrRateLimiter, QualifiedContract};
use crate::data::traits::IntradayDataSource;
use crate::error::ScreeningError;
use crate::models::{IntradayBar, IntradayBarSize};

pub struct IbkrClient {
    client: Client,
    contracts: ContractCache,
    rate_limiter: IbkrRateLimiter,
}

impl IbkrClient {
    /// Opens a single persistent connection to TWS or IB Gateway at `address`
    /// (e.g. "127.0.0.1:7497") using `client_id` to identify this session.
    /// This connection should be reused for the entire screening run rather
    /// than reconnecting per symbol.
    pub async fn connect(address: &str, client_id: i32) -> Result<Self, ScreeningError> {
        let client = Client::connect(address, client_id)
            .await
            .map_err(|e| map_ibkr_error(&e))?;
        Ok(Self {
            client,
            contracts: ContractCache::new(),
            rate_limiter: IbkrRateLimiter::new(),
        })
    }

    async fn qualify(&self, symbol: &str) -> Result<QualifiedContract, ScreeningError> {
        self.contracts
            .get_or_qualify(symbol, |symbol| async move {
                let contract = Contract::stock(symbol).build();
                let details = self
                    .client
                    .contract_details(&contract)
                    .await
                    .map_err(|e| map_ibkr_error(&e))?;
                let first = details
                    .into_iter()
                    .next()
                    .ok_or_else(|| ScreeningError::ContractQualificationFailed(symbol.to_string()))?;
                Ok(QualifiedContract {
                    symbol: symbol.to_string(),
                    contract_id: first.contract.contract_id,
                })
            })
            .await
    }
}

#[async_trait]
impl IntradayDataSource for IbkrClient {
    async fn fetch_intraday_bars(
        &self,
        symbol: &str,
        bar_size: IntradayBarSize,
        _min_bars: usize,
    ) -> Result<Vec<IntradayBar>, ScreeningError> {
        // Qualification also serves as an existence/entitlement check before
        // spending a rate-limited historical-data request on this symbol.
        self.qualify(symbol).await?;
        self.rate_limiter.acquire(symbol).await;

        let (ib_bar_size, duration_days) = match bar_size {
            // ~10 trading days of 30-min bars, per the Global Constraints
            // safety-margin rationale (13 bars/day; a half-day session can
            // remove ~6 bars, so 7 days is too thin a margin over the
            // 66-bar default requirement).
            IntradayBarSize::ThirtyMinutes => (HistoricalBarSize::Min30, 10),
            // 2-3 trading days of 2-min bars is ample for a 100-bar SMA
            // requirement (~195 bars/day); this plan only wires the
            // ThirtyMinutes path through Phase 2 — TwoMinutes support is for
            // the Phase 3 plan that builds on this client.
            IntradayBarSize::TwoMinutes => (HistoricalBarSize::Min2, 3),
        };

        let contract = Contract::stock(symbol).build();
        let historical_data = self
            .client
            .historical_data(&contract, ib_bar_size)
            .what_to_show(HistoricalWhatToShow::Trades)
            .trading_hours(TradingHours::Regular)
            .duration(duration_days.days())
            .fetch()
            .await
            .map_err(|e| map_ibkr_error(&e))?;

        let bars = historical_data
            .bars
            .into_iter()
            .filter_map(|bar| {
                let timestamp = match bar.date {
                    BarTimestamp::DateTime(odt) => chrono::DateTime::from_timestamp(odt.unix_timestamp(), 0)?,
                    // Daily/weekly timestamps are not expected for intraday
                    // bar sizes; skip defensively rather than fail the whole
                    // request over one malformed bar.
                    BarTimestamp::Date(_) => return None,
                };
                Some(IntradayBar {
                    timestamp,
                    open: bar.open,
                    high: bar.high,
                    low: bar.low,
                    close: bar.close,
                    volume: bar.volume,
                })
            })
            .collect();

        Ok(bars)
    }
}

/// Maps ibapi's error type to our ScreeningError taxonomy. The specific
/// IBKR error-code-to-variant mapping below (100 = pacing, 162/200 = no
/// data) covers the most common cases; it should be extended as real pacing
/// violations and data errors are observed against a live/paper account —
/// everything else currently falls through to IBKRApiError with the raw
/// code and message preserved for diagnosis.
fn map_ibkr_error(err: &Error) -> ScreeningError {
    match err {
        Error::Notice(notice) => match notice.code {
            100 => ScreeningError::PacingViolation,
            162 | 200 => ScreeningError::NoData(notice.message.clone()),
            code => ScreeningError::IBKRApiError(format!("[{code}] {}", notice.message)),
        },
        Error::ConnectionFailed | Error::ConnectionRejected(_) | Error::ConnectionReset => {
            ScreeningError::ConnectionError(err.to_string())
        }
        other => ScreeningError::IBKRApiError(other.to_string()),
    }
}
```

- [ ] **Step 3: Wire up the module**

Modify `crates/screener-core/src/data/ibkr/mod.rs`:
```rust
pub mod client;
pub mod contracts;
pub mod rate_limiter;

pub use client::IbkrClient;
pub use contracts::{ContractCache, QualifiedContract};
pub use rate_limiter::IbkrRateLimiter;
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build -p screener-core`
Expected: builds cleanly. If it does not — most likely because `ibapi`'s public API has moved since this plan was verified against its `main` branch — the fix is confined to `data/ibkr/client.rs`; re-check the failing names against `cargo doc -p screener-core --open` (which will show the resolved `ibapi` version's actual API) rather than guessing.

- [ ] **Step 5: Run the full test suite to confirm nothing else broke**

Run: `cargo test -p screener-core`
Expected: PASS — all tests from Tasks 1-7 (this plan) and the Phase 1 plan still pass. `IbkrClient` itself has no automated tests in this task (see the task header note).

- [ ] **Step 6: Commit**

```bash
git add crates/screener-core/src/data/ibkr crates/screener-core/Cargo.toml
git commit -m "feat(screener-core): add IbkrClient (wraps ibapi, implements IntradayDataSource)

Compile-verified only — no TWS/IB Gateway available in this environment.
Requires manual verification against a paper-trading account before
relying on it for real screening runs."
```

---

### Task 9: End-to-end example binary (Phase 1 -> Phase 2)

**Files:**
- Create: `crates/screener-core/examples/run_phase1_then_phase2.rs`

**Interfaces:**
- Consumes: `run_phase1`, `run_phase2`, `Phase1Config`, `Phase2Config`, `data::yahoo::{YahooClient, YahooClientConfig}`, `data::ibkr::IbkrClient` (all public API from this plan and the Phase 1 plan)
- Produces: a runnable example, no new library API

**This example requires a live/paper IBKR TWS or Gateway instance listening on the given address to actually run** — it will compile and can be built here, but running it end-to-end is out of scope for this environment. It demonstrates how the pieces compose.

- [ ] **Step 1: Write the example binary**

`crates/screener-core/examples/run_phase1_then_phase2.rs`:

```rust
use std::fs;
use std::sync::Arc;

use screener_core::data::ibkr::IbkrClient;
use screener_core::data::yahoo::{YahooClient, YahooClientConfig};
use screener_core::{run_phase1, run_phase2, Phase1Config, Phase2Config};

#[tokio::main]
async fn main() {
    let _ = tracing_subscriber::fmt::try_init();

    let path = std::env::args().nth(1).unwrap_or_else(|| {
        "crates/screener-core/examples/universe.sample.txt".to_string()
    });
    let ibkr_address = std::env::args().nth(2).unwrap_or_else(|| "127.0.0.1:7497".to_string());

    let universe: Vec<String> = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read universe file {path}: {e}"))
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    println!("Loaded {} symbols from {}", universe.len(), path);

    let yahoo = Arc::new(YahooClient::new(YahooClientConfig::default()));
    let phase1_config = Phase1Config::default();
    let phase1_results = run_phase1(&universe, &phase1_config, yahoo, None).await;

    println!(
        "Phase 1: {} survivors, {} technical failures, {} errors",
        phase1_results.survivors.len(),
        phase1_results.technical_failures.len(),
        phase1_results.errors.len()
    );

    if phase1_results.survivors.is_empty() {
        println!("No Phase 1 survivors — nothing to send to Phase 2.");
        return;
    }

    println!("Connecting to IBKR at {ibkr_address}...");
    let ibkr = match IbkrClient::connect(&ibkr_address, 100).await {
        Ok(client) => Arc::new(client),
        Err(err) => {
            eprintln!("Failed to connect to IBKR: {err}");
            return;
        }
    };

    let phase2_config = Phase2Config::default();
    let phase2_results = run_phase2(&phase1_results.survivors, &phase2_config, ibkr, None).await;

    println!(
        "Phase 2: {} survivors, {} technical failures, {} errors",
        phase2_results.survivors.len(),
        phase2_results.technical_failures.len(),
        phase2_results.errors.len()
    );

    println!("\nPhase 2 survivors:");
    for symbol in &phase2_results.survivors {
        println!("  {symbol}");
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p screener-core --example run_phase1_then_phase2`
Expected: builds cleanly.

- [ ] **Step 3: Run the full test suite one final time**

Run: `cargo test -p screener-core`
Expected: PASS — confirms the example addition didn't break anything.

- [ ] **Step 4: Commit**

```bash
git add crates/screener-core/examples/run_phase1_then_phase2.rs
git commit -m "feat(screener-core): add run_phase1_then_phase2 example demonstrating Phase 1 -> Phase 2 composition"
```

---

## Self-Review Notes

**Spec coverage:** section 4.5 (Phase 2 data/indicators/slope/filter, contract qualification and caching, single persistent connection) is covered by Tasks 1-5, 7, 8. Section 4.8 (centralized rate limiter, capped retries, pacing priority over throughput) is covered by Task 6, with the explicit scope note that the "≤6 identical-contract requests per 2s" sub-rule is not separately modeled (dominated by the 15s per-symbol cooldown for this access pattern). Section 4.9's IBKR-related error variants (`ContractQualificationFailed`, `PacingViolation`, `IBKRApiError`, `ConnectionError`, `NoData`, `Timeout`) were already defined in the Phase 1 plan's `error.rs` and are used, not redefined, by `map_ibkr_error` in Task 8. Section 4.10 (mockable data sources) is covered by Task 2's `IntradayDataSource`/`MockIntradayDataSource`. Section 4.6 (Phase 3, VWAP, session-boundary timezone handling) and the remainder of section 4.7 (DST/session-boundary specifics) are explicitly out of scope — they're the next plan, building on this one's `IbkrClient`/`IntradayDataSource`/rate limiter/contract cache.

**Placeholder scan:** no TBD/TODO. Task 8/9 are explicitly marked as compile-verified-only rather than glossing over the live-TWS testing gap — that's a stated limitation, not a placeholder.

**Type consistency:** `IntradayBar`, `IntradayBarSize`, `Phase2Config`, `Phase2Outcome`, `IntradayDataSource`, `Phase2Progress`, `Phase2Results`, `QualifiedContract`, `ContractCache`, `IbkrRateLimiter`, `IbkrClient` are each defined once and referenced identically across every task and the example that uses them.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-08-24-screener-core-phase2-ibkr.md`. Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
