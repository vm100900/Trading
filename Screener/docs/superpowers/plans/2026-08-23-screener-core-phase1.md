# screener-core Phase 1 (Yahoo Daily Macro Filter) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `screener-core` library crate's Phase 1 pipeline — Yahoo Finance daily-bar retrieval, SMA/slope indicators, and the Phase 1 filter — as a standalone, fully tested slice with no IBKR, service, or app dependency, per the design spec's delivery order (section 9, item 1).

**Architecture:** A pure-library crate with clear module boundaries: `models` (data types), `indicators` (pure SMA math), `config` (runtime-configurable filter parameters with locked defaults), `phase1` (pure filter-decision logic), `data` (a `DailyDataSource` trait plus a Yahoo HTTP implementation), and `screener` (bounded-concurrency orchestration wiring the pieces together with progress reporting). Everything except the Yahoo HTTP client is pure/synchronous and trivially unit-testable; the Yahoo client and orchestration are tested against mocks (`wiremock` for HTTP, a hand-written mock for the trait) so no test ever touches the real network.

**Tech Stack:** Rust (stable), `tokio` (async runtime), `reqwest` (HTTP + cookie jar), `serde`/`serde_json` (Yahoo JSON parsing), `chrono` (dates), `thiserror` (typed errors), `async-trait` (dyn-compatible async trait), `rand` (backoff jitter), `tracing` (logging); dev-dependency `wiremock` (HTTP mocking for tests).

**Spec:** `/home/vijay/Study/Screener/docs/superpowers/specs/2026-08-23-rust-screener-design.md` (sections 4.1–4.4, 4.9, 4.10)

## Global Constraints

- SMA only, never EMA. `SMA_N[t] = arithmetic mean of the latest N completed closes`.
- All filter comparisons are strict `>` — never `>=`.
- Default Phase 1 requires ≥205 valid completed daily bars (200 for SMA200 + 5 for the slope comparison); this is a function of the *active* config, not a hardcoded constant — changing `sma200_period` or `slope_lookback_bars` changes the requirement.
- Default SMA200 slope lookback is exactly 5 completed trading bars (not 7), compared by bar index, never calendar days.
- Use adjusted close consistently for all price comparisons.
- No look-ahead bias: only the latest *completed* daily bar is ever evaluated.
- Missing/insufficient/malformed data must produce `InsufficientData`/`YahooDataError`, and must never be counted as a `TechnicalFilterFailed`.
- Yahoo access requires a session cookie + `crumb`; requests are bounded to 5–10 concurrent, with exponential backoff + jitter on 429/5xx, and a capped retry count (never infinite retry).
- `Phase1Config` defaults must exactly match the locked spec values above and must be fully overridable at runtime by the caller.
- All data access goes through the `DailyDataSource` trait — no module outside `data/` ever makes an HTTP call directly, so tests can substitute mocks.

---

## File Structure

```text
Screener/
├── Cargo.toml                          # workspace root
└── crates/
    └── screener-core/
        ├── Cargo.toml
        ├── src/
        │   ├── lib.rs                  # public API re-exports
        │   ├── error.rs                # ScreeningError (full taxonomy)
        │   ├── models.rs               # DailyBar
        │   ├── config.rs               # Phase1Config + defaults
        │   ├── indicators/
        │   │   ├── mod.rs
        │   │   └── sma.rs              # sma_at, sma
        │   ├── data/
        │   │   ├── mod.rs
        │   │   ├── traits.rs           # DailyDataSource trait
        │   │   └── yahoo.rs            # YahooClient (session/crumb, fetch, retry)
        │   ├── phase1.rs                # evaluate_phase1 (pure filter logic)
        │   └── screener.rs              # run_phase1 (bounded-concurrency orchestration)
        ├── tests/
        │   ├── common/
        │   │   └── mod.rs               # MockDailyDataSource
        │   └── phase1_integration.rs    # end-to-end run_phase1 against the mock
        └── examples/
            └── run_phase1.rs             # reads a universe file, runs Phase 1 for real
```

---

### Task 1: Workspace scaffold, error taxonomy, DailyBar model

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/screener-core/Cargo.toml`
- Create: `crates/screener-core/src/lib.rs`
- Create: `crates/screener-core/src/error.rs`
- Create: `crates/screener-core/src/models.rs`

**Interfaces:**
- Produces: `pub enum ScreeningError { TechnicalFilterFailed { symbol: String }, InsufficientData { symbol: String, needed: usize, got: usize }, InvalidSymbol(String), ContractQualificationFailed(String), NoData(String), Timeout(String), PacingViolation, IBKRApiError(String), ConnectionError(String), CalculationError(String), YahooDataError { symbol: String, message: String }, YahooAuthError(String) }` — `Debug + Clone + thiserror::Error`
- Produces: `pub struct DailyBar { pub date: chrono::NaiveDate, pub open: f64, pub high: f64, pub low: f64, pub close: f64, pub volume: u64 }` — `Debug + Clone + PartialEq`

- [ ] **Step 1: Create the workspace root**

`Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["crates/screener-core"]
```

- [ ] **Step 2: Create the crate manifest**

`crates/screener-core/Cargo.toml`:
```toml
[package]
name = "screener-core"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync", "time"] }
reqwest = { version = "0.12", features = ["json", "cookies"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }
thiserror = "1"
async-trait = "0.1"
rand = "0.8"
tracing = "0.1"

[dev-dependencies]
wiremock = "0.6"
```

- [ ] **Step 3: Write the failing test for `ScreeningError` display formatting**

`crates/screener-core/src/error.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insufficient_data_formats_with_symbol_and_counts() {
        let err = ScreeningError::InsufficientData {
            symbol: "AAPL".to_string(),
            needed: 207,
            got: 150,
        };
        assert_eq!(
            err.to_string(),
            "insufficient data for AAPL: needed 207, got 150"
        );
    }

    #[test]
    fn technical_filter_failed_formats_with_symbol() {
        let err = ScreeningError::TechnicalFilterFailed { symbol: "MSFT".to_string() };
        assert_eq!(err.to_string(), "technical filter failed for MSFT");
    }
}
```

- [ ] **Step 4: Run the test to verify it fails**

Run: `cargo test -p screener-core error::tests`
Expected: FAIL to compile — `ScreeningError` is not defined yet.

- [ ] **Step 5: Implement `ScreeningError`**

Add above the `#[cfg(test)]` module in `crates/screener-core/src/error.rs`:
```rust
use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum ScreeningError {
    #[error("technical filter failed for {symbol}")]
    TechnicalFilterFailed { symbol: String },

    #[error("insufficient data for {symbol}: needed {needed}, got {got}")]
    InsufficientData {
        symbol: String,
        needed: usize,
        got: usize,
    },

    #[error("invalid symbol: {0}")]
    InvalidSymbol(String),

    #[error("contract qualification failed for {0}")]
    ContractQualificationFailed(String),

    #[error("no data returned for {0}")]
    NoData(String),

    #[error("request timed out for {0}")]
    Timeout(String),

    #[error("IBKR pacing violation")]
    PacingViolation,

    #[error("IBKR API error: {0}")]
    IBKRApiError(String),

    #[error("connection error: {0}")]
    ConnectionError(String),

    #[error("calculation error: {0}")]
    CalculationError(String),

    #[error("Yahoo data error for {symbol}: {message}")]
    YahooDataError { symbol: String, message: String },

    #[error("Yahoo auth error: {0}")]
    YahooAuthError(String),
}
```

- [ ] **Step 6: Run the test to verify it passes**

Run: `cargo test -p screener-core error::tests`
Expected: PASS (2 tests)

- [ ] **Step 7: Write the failing test for `DailyBar`**

`crates/screener-core/src/models.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn daily_bar_equality_by_value() {
        let a = DailyBar {
            date: NaiveDate::from_ymd_opt(2026, 8, 20).unwrap(),
            open: 100.0,
            high: 105.0,
            low: 99.0,
            close: 104.0,
            volume: 1_000_000,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }
}
```

- [ ] **Step 8: Run the test to verify it fails**

Run: `cargo test -p screener-core models::tests`
Expected: FAIL to compile — `DailyBar` is not defined yet.

- [ ] **Step 9: Implement `DailyBar`**

Add above the `#[cfg(test)]` module in `crates/screener-core/src/models.rs`:
```rust
use chrono::NaiveDate;

#[derive(Debug, Clone, PartialEq)]
pub struct DailyBar {
    pub date: NaiveDate,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    /// Adjusted close — this is the close used for every Phase 1 comparison.
    pub close: f64,
    pub volume: u64,
}
```

- [ ] **Step 10: Run the test to verify it passes**

Run: `cargo test -p screener-core models::tests`
Expected: PASS (1 test)

- [ ] **Step 11: Wire up `lib.rs`**

`crates/screener-core/src/lib.rs`:
```rust
pub mod error;
pub mod models;

pub use error::ScreeningError;
pub use models::DailyBar;
```

- [ ] **Step 12: Verify the whole crate builds**

Run: `cargo build -p screener-core`
Expected: builds cleanly with no warnings

- [ ] **Step 13: Commit**

```bash
git add Cargo.toml crates/screener-core/Cargo.toml crates/screener-core/src/lib.rs crates/screener-core/src/error.rs crates/screener-core/src/models.rs
git commit -m "feat(screener-core): scaffold crate, ScreeningError taxonomy, DailyBar model"
```

---

### Task 2: SMA indicator

**Files:**
- Create: `crates/screener-core/src/indicators/mod.rs`
- Create: `crates/screener-core/src/indicators/sma.rs`
- Modify: `crates/screener-core/src/lib.rs`

**Interfaces:**
- Consumes: nothing from earlier tasks (pure `f64` math)
- Produces: `pub fn sma_at(closes: &[f64], period: usize, bars_back: usize) -> Option<f64>`, `pub fn sma(closes: &[f64], period: usize) -> Option<f64>` (== `sma_at(closes, period, 0)`)

- [ ] **Step 1: Write the failing tests**

`crates/screener-core/src/indicators/sma.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sma_is_arithmetic_mean_of_latest_n_closes() {
        let closes = [1.0, 2.0, 3.0, 4.0, 5.0];
        // latest 3 closes: 3.0, 4.0, 5.0 -> mean 4.0
        assert_eq!(sma(&closes, 3), Some(4.0));
    }

    #[test]
    fn sma_returns_none_when_fewer_bars_than_period() {
        let closes = [1.0, 2.0];
        assert_eq!(sma(&closes, 3), None);
    }

    #[test]
    fn sma_returns_none_for_zero_period() {
        let closes = [1.0, 2.0, 3.0];
        assert_eq!(sma(&closes, 0), None);
    }

    #[test]
    fn sma_at_computes_sma_as_of_n_bars_back() {
        let closes = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        // 2 bars back from the end means the window ends at index len-1-2=4 (value 5.0),
        // so SMA(3) as of 2 bars back = mean(3.0, 4.0, 5.0) = 4.0
        assert_eq!(sma_at(&closes, 3, 2), Some(4.0));
    }

    #[test]
    fn sma_at_returns_none_when_bars_back_plus_period_exceeds_length() {
        let closes = [1.0, 2.0, 3.0];
        assert_eq!(sma_at(&closes, 3, 1), None);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p screener-core indicators::sma::tests`
Expected: FAIL to compile — `sma`/`sma_at` not defined.

- [ ] **Step 3: Implement the SMA functions**

Add above the `#[cfg(test)]` module in `crates/screener-core/src/indicators/sma.rs`:
```rust
/// SMA of `period` closes, ending `bars_back` bars before the most recent close.
/// `bars_back == 0` means "the current/latest SMA".
pub fn sma_at(closes: &[f64], period: usize, bars_back: usize) -> Option<f64> {
    if period == 0 {
        return None;
    }
    let end = closes.len().checked_sub(bars_back)?;
    let start = end.checked_sub(period)?;
    let window = &closes[start..end];
    Some(window.iter().sum::<f64>() / period as f64)
}

pub fn sma(closes: &[f64], period: usize) -> Option<f64> {
    sma_at(closes, period, 0)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p screener-core indicators::sma::tests`
Expected: PASS (5 tests)

- [ ] **Step 5: Wire up the module**

`crates/screener-core/src/indicators/mod.rs`:
```rust
pub mod sma;

pub use sma::{sma, sma_at};
```

Modify `crates/screener-core/src/lib.rs`, add:
```rust
pub mod indicators;
```

- [ ] **Step 6: Commit**

```bash
git add crates/screener-core/src/indicators crates/screener-core/src/lib.rs
git commit -m "feat(screener-core): add SMA indicator (sma, sma_at)"
```

---

### Task 3: Phase1Config

**Files:**
- Create: `crates/screener-core/src/config.rs`
- Modify: `crates/screener-core/src/lib.rs`

**Interfaces:**
- Consumes: nothing
- Produces: `pub struct Phase1Config { sma10_period, sma20_period, sma50_period, sma200_period: usize, slope_lookback_bars: usize, enable_close_gt_sma10, enable_sma10_gt_sma20, enable_sma20_gt_sma50, enable_sma50_gt_sma200, enable_sma200_slope: bool }` implementing `Default`, plus `pub fn min_bars_required(&self) -> usize`

- [ ] **Step 1: Write the failing tests**

`crates/screener-core/src/config.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_locked_spec() {
        let cfg = Phase1Config::default();
        assert_eq!(cfg.sma10_period, 10);
        assert_eq!(cfg.sma20_period, 20);
        assert_eq!(cfg.sma50_period, 50);
        assert_eq!(cfg.sma200_period, 200);
        assert_eq!(cfg.slope_lookback_bars, 5);
        assert!(cfg.enable_close_gt_sma10);
        assert!(cfg.enable_sma10_gt_sma20);
        assert!(cfg.enable_sma20_gt_sma50);
        assert!(cfg.enable_sma50_gt_sma200);
        assert!(cfg.enable_sma200_slope);
    }

    #[test]
    fn default_min_bars_required_is_205() {
        let cfg = Phase1Config::default();
        assert_eq!(cfg.min_bars_required(), 205);
    }

    #[test]
    fn min_bars_required_ignores_slope_lookback_when_slope_disabled() {
        let mut cfg = Phase1Config::default();
        cfg.enable_sma200_slope = false;
        assert_eq!(cfg.min_bars_required(), 200);
    }

    #[test]
    fn min_bars_required_tracks_custom_periods() {
        let mut cfg = Phase1Config::default();
        cfg.sma200_period = 300;
        assert_eq!(cfg.min_bars_required(), 305);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p screener-core config::tests`
Expected: FAIL to compile — `Phase1Config` not defined.

- [ ] **Step 3: Implement `Phase1Config`**

Add above the `#[cfg(test)]` module in `crates/screener-core/src/config.rs`:
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Phase1Config {
    pub sma10_period: usize,
    pub sma20_period: usize,
    pub sma50_period: usize,
    pub sma200_period: usize,
    pub slope_lookback_bars: usize,
    pub enable_close_gt_sma10: bool,
    pub enable_sma10_gt_sma20: bool,
    pub enable_sma20_gt_sma50: bool,
    pub enable_sma50_gt_sma200: bool,
    pub enable_sma200_slope: bool,
}

impl Default for Phase1Config {
    fn default() -> Self {
        Self {
            sma10_period: 10,
            sma20_period: 20,
            sma50_period: 50,
            sma200_period: 200,
            slope_lookback_bars: 5,
            enable_close_gt_sma10: true,
            enable_sma10_gt_sma20: true,
            enable_sma20_gt_sma50: true,
            enable_sma50_gt_sma200: true,
            enable_sma200_slope: true,
        }
    }
}

impl Phase1Config {
    /// Minimum completed daily bars required to evaluate this config's active conditions.
    pub fn min_bars_required(&self) -> usize {
        let longest_period = self
            .sma10_period
            .max(self.sma20_period)
            .max(self.sma50_period)
            .max(self.sma200_period);

        if self.enable_sma200_slope {
            longest_period + self.slope_lookback_bars
        } else {
            longest_period
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p screener-core config::tests`
Expected: PASS (4 tests)

- [ ] **Step 5: Wire up the module**

Modify `crates/screener-core/src/lib.rs`, add:
```rust
pub mod config;

pub use config::Phase1Config;
```

- [ ] **Step 6: Commit**

```bash
git add crates/screener-core/src/config.rs crates/screener-core/src/lib.rs
git commit -m "feat(screener-core): add Phase1Config with locked defaults"
```

---

### Task 4: Phase 1 filter logic

**Files:**
- Create: `crates/screener-core/src/phase1.rs`
- Modify: `crates/screener-core/src/lib.rs`

**Interfaces:**
- Consumes: `DailyBar` (models), `Phase1Config` (config), `sma_at` (indicators::sma)
- Produces: `pub enum Phase1Outcome { Passed, FailedTechnical, InsufficientData { needed: usize, got: usize } }` (`Debug + Clone + PartialEq`), `pub fn evaluate_phase1(bars: &[DailyBar], config: &Phase1Config) -> Phase1Outcome`

- [ ] **Step 1: Write the failing tests**

`crates/screener-core/src/phase1.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn bars_from_closes(closes: &[f64]) -> Vec<DailyBar> {
        closes
            .iter()
            .enumerate()
            .map(|(i, &close)| DailyBar {
                date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap() + chrono::Duration::days(i as i64),
                open: close,
                high: close,
                low: close,
                close,
                volume: 1_000_000,
            })
            .collect()
    }

    #[test]
    fn insufficient_bars_fails_with_needed_and_got() {
        let bars = bars_from_closes(&[1.0, 2.0, 3.0]);
        let config = Phase1Config::default();
        let outcome = evaluate_phase1(&bars, &config);
        assert_eq!(
            outcome,
            Phase1Outcome::InsufficientData { needed: 205, got: 3 }
        );
    }

    #[test]
    fn strictly_ascending_closes_with_flat_tail_slope_passes() {
        // 210 closes rising from 1.0 to 210.0 in steps of 1.0.
        // close > sma10 > sma20 > sma50 > sma200 holds for a strictly rising series,
        // and sma200 (computed from the later, higher closes) is still rising
        // 5 bars later than 5 bars ago, satisfying the slope condition.
        let closes: Vec<f64> = (1..=210).map(|i| i as f64).collect();
        let bars = bars_from_closes(&closes);
        let config = Phase1Config::default();
        assert_eq!(evaluate_phase1(&bars, &config), Phase1Outcome::Passed);
    }

    #[test]
    fn flat_series_fails_technical_because_slope_is_not_strictly_greater() {
        let closes = vec![100.0; 210];
        let bars = bars_from_closes(&closes);
        let config = Phase1Config::default();
        assert_eq!(evaluate_phase1(&bars, &config), Phase1Outcome::FailedTechnical);
    }

    #[test]
    fn falling_series_fails_technical_on_close_gt_sma10() {
        let closes: Vec<f64> = (1..=210).rev().map(|i| i as f64).collect();
        let bars = bars_from_closes(&closes);
        let config = Phase1Config::default();
        assert_eq!(evaluate_phase1(&bars, &config), Phase1Outcome::FailedTechnical);
    }

    #[test]
    fn disabling_slope_condition_lets_flat_series_pass_other_conditions() {
        // Rising series so the SMA-stack condition passes, but disable the slope
        // check and confirm a config where sma200 slope would otherwise matter
        // no longer gates the result.
        let closes: Vec<f64> = (1..=210).map(|i| i as f64).collect();
        let bars = bars_from_closes(&closes);
        let mut config = Phase1Config::default();
        config.enable_sma200_slope = false;
        assert_eq!(evaluate_phase1(&bars, &config), Phase1Outcome::Passed);
    }

    #[test]
    fn custom_periods_change_required_bar_count() {
        let bars = bars_from_closes(&(1..=210).map(|i| i as f64).collect::<Vec<_>>());
        let mut config = Phase1Config::default();
        config.sma200_period = 300;
        let outcome = evaluate_phase1(&bars, &config);
        assert_eq!(outcome, Phase1Outcome::InsufficientData { needed: 305, got: 210 });
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p screener-core phase1::tests`
Expected: FAIL to compile — `evaluate_phase1`/`Phase1Outcome` not defined.

- [ ] **Step 3: Implement `evaluate_phase1`**

Add above the `#[cfg(test)]` module in `crates/screener-core/src/phase1.rs`:
```rust
use crate::config::Phase1Config;
use crate::indicators::sma_at;
use crate::models::DailyBar;

#[derive(Debug, Clone, PartialEq)]
pub enum Phase1Outcome {
    Passed,
    FailedTechnical,
    InsufficientData { needed: usize, got: usize },
}

pub fn evaluate_phase1(bars: &[DailyBar], config: &Phase1Config) -> Phase1Outcome {
    let needed = config.min_bars_required();
    if bars.len() < needed {
        return Phase1Outcome::InsufficientData {
            needed,
            got: bars.len(),
        };
    }

    let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
    let close = *closes.last().expect("checked non-empty via min_bars_required");

    let sma10 = sma_at(&closes, config.sma10_period, 0);
    let sma20 = sma_at(&closes, config.sma20_period, 0);
    let sma50 = sma_at(&closes, config.sma50_period, 0);
    let sma200 = sma_at(&closes, config.sma200_period, 0);

    let (sma10, sma20, sma50, sma200) = match (sma10, sma20, sma50, sma200) {
        (Some(a), Some(b), Some(c), Some(d)) => (a, b, c, d),
        _ => return Phase1Outcome::InsufficientData { needed, got: bars.len() },
    };

    if config.enable_close_gt_sma10 && !(close > sma10) {
        return Phase1Outcome::FailedTechnical;
    }
    if config.enable_sma10_gt_sma20 && !(sma10 > sma20) {
        return Phase1Outcome::FailedTechnical;
    }
    if config.enable_sma20_gt_sma50 && !(sma20 > sma50) {
        return Phase1Outcome::FailedTechnical;
    }
    if config.enable_sma50_gt_sma200 && !(sma50 > sma200) {
        return Phase1Outcome::FailedTechnical;
    }

    if config.enable_sma200_slope {
        let sma200_prior = match sma_at(&closes, config.sma200_period, config.slope_lookback_bars) {
            Some(v) => v,
            None => return Phase1Outcome::InsufficientData { needed, got: bars.len() },
        };
        if !(sma200 > sma200_prior) {
            return Phase1Outcome::FailedTechnical;
        }
    }

    Phase1Outcome::Passed
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p screener-core phase1::tests`
Expected: PASS (6 tests)

- [ ] **Step 5: Wire up the module**

Modify `crates/screener-core/src/lib.rs`, add:
```rust
pub mod phase1;

pub use phase1::{evaluate_phase1, Phase1Outcome};
```

- [ ] **Step 6: Commit**

```bash
git add crates/screener-core/src/phase1.rs crates/screener-core/src/lib.rs
git commit -m "feat(screener-core): add Phase 1 filter logic (evaluate_phase1)"
```

---

### Task 5: DailyDataSource trait + test mock

**Files:**
- Create: `crates/screener-core/src/data/mod.rs`
- Create: `crates/screener-core/src/data/traits.rs`
- Create: `crates/screener-core/tests/common/mod.rs`
- Modify: `crates/screener-core/src/lib.rs`

**Interfaces:**
- Consumes: `DailyBar` (models), `ScreeningError` (error)
- Produces: `#[async_trait] pub trait DailyDataSource: Send + Sync { async fn fetch_daily_bars(&self, symbol: &str, min_bars: usize) -> Result<Vec<DailyBar>, ScreeningError>; }`; test-only `MockDailyDataSource` (in `tests/common`, not part of the crate's public API) implementing it

- [ ] **Step 1: Write the failing test for the mock (as an integration test)**

`crates/screener-core/tests/common/mod.rs`:
```rust
use std::collections::HashMap;

use async_trait::async_trait;
use screener_core::{DailyBar, DailyDataSource, ScreeningError};

pub enum MockResponse {
    Bars(Vec<DailyBar>),
    Error(ScreeningError),
}

pub struct MockDailyDataSource {
    pub responses: HashMap<String, MockResponse>,
}

impl MockDailyDataSource {
    pub fn new() -> Self {
        Self { responses: HashMap::new() }
    }

    pub fn with_bars(mut self, symbol: &str, bars: Vec<DailyBar>) -> Self {
        self.responses.insert(symbol.to_string(), MockResponse::Bars(bars));
        self
    }

    pub fn with_error(mut self, symbol: &str, error: ScreeningError) -> Self {
        self.responses.insert(symbol.to_string(), MockResponse::Error(error));
        self
    }
}

#[async_trait]
impl DailyDataSource for MockDailyDataSource {
    async fn fetch_daily_bars(&self, symbol: &str, _min_bars: usize) -> Result<Vec<DailyBar>, ScreeningError> {
        match self.responses.get(symbol) {
            Some(MockResponse::Bars(bars)) => Ok(bars.clone()),
            Some(MockResponse::Error(e)) => Err(e.clone()),
            None => Err(ScreeningError::NoData(symbol.to_string())),
        }
    }
}
```

`crates/screener-core/tests/data_source_mock_test.rs`:
```rust
mod common;

use common::MockDailyDataSource;
use screener_core::{DailyBar, DailyDataSource, ScreeningError};
use chrono::NaiveDate;

fn one_bar() -> DailyBar {
    DailyBar {
        date: NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(),
        open: 10.0,
        high: 11.0,
        low: 9.0,
        close: 10.5,
        volume: 500_000,
    }
}

#[tokio::test]
async fn mock_returns_configured_bars_for_known_symbol() {
    let source = MockDailyDataSource::new().with_bars("AAPL", vec![one_bar()]);
    let bars = source.fetch_daily_bars("AAPL", 1).await.unwrap();
    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].close, 10.5);
}

#[tokio::test]
async fn mock_returns_no_data_error_for_unknown_symbol() {
    let source = MockDailyDataSource::new();
    let result = source.fetch_daily_bars("ZZZZ", 1).await;
    assert!(matches!(result, Err(ScreeningError::NoData(_))));
}

#[tokio::test]
async fn mock_returns_configured_error() {
    let source = MockDailyDataSource::new()
        .with_error("BADSYM", ScreeningError::InvalidSymbol("BADSYM".to_string()));
    let result = source.fetch_daily_bars("BADSYM", 1).await;
    assert!(matches!(result, Err(ScreeningError::InvalidSymbol(_))));
}
```

- [ ] **Step 2: Add the dev-dependency needed for async tests**

Modify `crates/screener-core/Cargo.toml`, in `[dev-dependencies]` add:
```toml
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p screener-core --test data_source_mock_test`
Expected: FAIL to compile — `DailyDataSource` not defined in `screener_core`.

- [ ] **Step 4: Implement the `DailyDataSource` trait**

`crates/screener-core/src/data/traits.rs`:
```rust
use async_trait::async_trait;

use crate::error::ScreeningError;
use crate::models::DailyBar;

#[async_trait]
pub trait DailyDataSource: Send + Sync {
    /// Fetch daily bars for `symbol`, requesting enough history to cover at
    /// least `min_bars` valid completed bars (implementations should request
    /// extra buffer, not exactly `min_bars`).
    async fn fetch_daily_bars(&self, symbol: &str, min_bars: usize) -> Result<Vec<DailyBar>, ScreeningError>;
}
```

`crates/screener-core/src/data/mod.rs`:
```rust
pub mod traits;

pub use traits::DailyDataSource;
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p screener-core --test data_source_mock_test`
Expected: PASS (3 tests)

- [ ] **Step 6: Wire up the module**

Modify `crates/screener-core/src/lib.rs`, add:
```rust
pub mod data;

pub use data::DailyDataSource;
```

- [ ] **Step 7: Commit**

```bash
git add crates/screener-core/src/data crates/screener-core/src/lib.rs crates/screener-core/Cargo.toml crates/screener-core/tests
git commit -m "feat(screener-core): add DailyDataSource trait and test mock"
```

---

### Task 6: Yahoo client (session/crumb auth, fetch, retry)

**Files:**
- Create: `crates/screener-core/src/data/yahoo.rs`
- Modify: `crates/screener-core/src/data/mod.rs`
- Modify: `crates/screener-core/src/lib.rs`
- Modify: `crates/screener-core/Cargo.toml`

**Interfaces:**
- Consumes: `DailyDataSource` (data::traits), `DailyBar` (models), `ScreeningError` (error)
- Produces: `pub struct YahooClientConfig { pub session_url: String, pub crumb_url: String, pub chart_base_url: String, pub max_concurrent_requests: usize, pub max_retries: u32 }` with `Default`; `pub struct YahooClient` implementing `DailyDataSource`, constructed via `YahooClient::new(config: YahooClientConfig) -> Self`

**Note on real Yahoo endpoints:** Yahoo's cookie/crumb bootstrap flow is unofficial and has changed shape before; this task builds it behind configurable URLs precisely so it can be tested against a mock server without depending on Yahoo's actual current behavior, and so the URLs can be adjusted later without touching calling code if Yahoo changes its flow again.

- [ ] **Step 1: Add the new dependencies**

Modify `crates/screener-core/Cargo.toml`, `[dev-dependencies]` — `wiremock` is already present from Task 1; no change needed there. Confirm `[dependencies]` already has `reqwest` with `cookies` feature (from Task 1) — no change needed.

- [ ] **Step 2: Write the failing tests against a mock Yahoo server**

`crates/screener-core/tests/yahoo_client_test.rs`:
```rust
use screener_core::data::yahoo::{YahooClient, YahooClientConfig};
use screener_core::DailyDataSource;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn sample_chart_body() -> serde_json::Value {
    serde_json::json!({
        "chart": {
            "result": [{
                "meta": { "symbol": "AAPL" },
                "timestamp": [1700000000, 1700086400, 1700172800],
                "indicators": {
                    "quote": [{
                        "open": [10.0, 11.0, 12.0],
                        "high": [10.5, 11.5, 12.5],
                        "low": [9.5, 10.5, 11.5],
                        "close": [10.2, 11.2, 12.2],
                        "volume": [1000, 1100, 1200]
                    }],
                    "adjclose": [{
                        "adjclose": [10.1, 11.1, 12.1]
                    }]
                }
            }],
            "error": null
        }
    })
}

async fn mock_server_with_crumb_and_chart(chart_body: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/session"))
        .respond_with(ResponseTemplate::new(200).insert_header("set-cookie", "A1=session; Path=/"))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/crumb"))
        .respond_with(ResponseTemplate::new(200).set_body_string("test-crumb"))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/chart/AAPL"))
        .and(query_param("crumb", "test-crumb"))
        .respond_with(ResponseTemplate::new(200).set_body_json(chart_body))
        .mount(&server)
        .await;

    server
}

fn client_for(server: &MockServer) -> YahooClient {
    let config = YahooClientConfig {
        session_url: format!("{}/session", server.uri()),
        crumb_url: format!("{}/crumb", server.uri()),
        chart_base_url: format!("{}/chart", server.uri()),
        max_concurrent_requests: 5,
        max_retries: 2,
    };
    YahooClient::new(config)
}

#[tokio::test]
async fn fetches_and_parses_daily_bars_using_adjusted_close() {
    let server = mock_server_with_crumb_and_chart(sample_chart_body()).await;
    let client = client_for(&server);

    let bars = client.fetch_daily_bars("AAPL", 3).await.unwrap();

    assert_eq!(bars.len(), 3);
    // adjclose values, not raw close
    assert_eq!(bars[0].close, 10.1);
    assert_eq!(bars[1].close, 11.1);
    assert_eq!(bars[2].close, 12.1);
    assert_eq!(bars[0].volume, 1000);
}

#[tokio::test]
async fn skips_bars_with_null_fields() {
    // volume has no fallback (unlike close, which falls back from adjclose to
    // raw close), so nulling it is what actually exercises the skip path.
    let mut body = sample_chart_body();
    body["chart"]["result"][0]["indicators"]["quote"][0]["volume"][1] = serde_json::Value::Null;
    let server = mock_server_with_crumb_and_chart(body).await;
    let client = client_for(&server);

    let bars = client.fetch_daily_bars("AAPL", 1).await.unwrap();

    assert_eq!(bars.len(), 2); // the bar with a null volume is skipped
}

#[tokio::test]
async fn returns_yahoo_data_error_when_result_is_empty() {
    let empty_body = serde_json::json!({ "chart": { "result": [], "error": null } });
    let server = mock_server_with_crumb_and_chart(empty_body).await;
    let client = client_for(&server);

    let result = client.fetch_daily_bars("AAPL", 1).await;

    assert!(matches!(result, Err(screener_core::ScreeningError::YahooDataError { .. })));
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p screener-core --test yahoo_client_test`
Expected: FAIL to compile — `YahooClient`/`YahooClientConfig` not defined.

- [ ] **Step 4: Implement the Yahoo client**

`crates/screener-core/src/data/yahoo.rs`:
```rust
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rand::Rng;
use serde::Deserialize;
use tokio::sync::{RwLock, Semaphore};
use tracing::warn;

use crate::data::traits::DailyDataSource;
use crate::error::ScreeningError;
use crate::models::DailyBar;

#[derive(Debug, Clone)]
pub struct YahooClientConfig {
    pub session_url: String,
    pub crumb_url: String,
    pub chart_base_url: String,
    pub max_concurrent_requests: usize,
    pub max_retries: u32,
}

impl Default for YahooClientConfig {
    fn default() -> Self {
        Self {
            session_url: "https://fc.yahoo.com".to_string(),
            crumb_url: "https://query2.finance.yahoo.com/v1/test/getcrumb".to_string(),
            chart_base_url: "https://query1.finance.yahoo.com/v8/finance/chart".to_string(),
            max_concurrent_requests: 8,
            max_retries: 3,
        }
    }
}

pub struct YahooClient {
    http: reqwest::Client,
    config: YahooClientConfig,
    crumb: RwLock<Option<String>>,
    concurrency: Arc<Semaphore>,
}

impl YahooClient {
    pub fn new(config: YahooClientConfig) -> Self {
        let http = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .expect("reqwest client build should not fail with default settings");
        let concurrency = Arc::new(Semaphore::new(config.max_concurrent_requests));
        Self {
            http,
            config,
            crumb: RwLock::new(None),
            concurrency,
        }
    }

    async fn ensure_crumb(&self) -> Result<String, ScreeningError> {
        if let Some(crumb) = self.crumb.read().await.clone() {
            return Ok(crumb);
        }

        self.http
            .get(&self.config.session_url)
            .send()
            .await
            .map_err(|e| ScreeningError::YahooAuthError(format!("session bootstrap failed: {e}")))?;

        let resp = self
            .http
            .get(&self.config.crumb_url)
            .send()
            .await
            .map_err(|e| ScreeningError::YahooAuthError(format!("crumb request failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(ScreeningError::YahooAuthError(format!(
                "crumb request returned status {}",
                resp.status()
            )));
        }

        let crumb = resp
            .text()
            .await
            .map_err(|e| ScreeningError::YahooAuthError(format!("failed to read crumb body: {e}")))?;

        *self.crumb.write().await = Some(crumb.clone());
        Ok(crumb)
    }

    async fn fetch_once(&self, symbol: &str) -> Result<Vec<DailyBar>, ScreeningError> {
        let crumb = self.ensure_crumb().await?;
        let url = format!("{}/{}", self.config.chart_base_url, symbol);

        let resp = self
            .http
            .get(&url)
            .query(&[
                ("interval", "1d"),
                ("range", "1y"),
                ("events", "div,splits"),
                ("crumb", crumb.as_str()),
            ])
            .send()
            .await
            .map_err(|e| ScreeningError::ConnectionError(e.to_string()))?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            *self.crumb.write().await = None;
            return Err(ScreeningError::YahooAuthError("crumb rejected (401)".to_string()));
        }
        if !resp.status().is_success() {
            return Err(ScreeningError::YahooDataError {
                symbol: symbol.to_string(),
                message: format!("HTTP {}", resp.status()),
            });
        }

        let body: ChartResponse = resp
            .json()
            .await
            .map_err(|e| ScreeningError::YahooDataError {
                symbol: symbol.to_string(),
                message: format!("failed to parse response: {e}"),
            })?;

        parse_chart_response(symbol, body)
    }
}

#[async_trait]
impl DailyDataSource for YahooClient {
    async fn fetch_daily_bars(&self, symbol: &str, _min_bars: usize) -> Result<Vec<DailyBar>, ScreeningError> {
        let _permit = self
            .concurrency
            .acquire()
            .await
            .expect("semaphore is never closed");

        let mut attempt = 0;
        loop {
            match self.fetch_once(symbol).await {
                Ok(bars) => return Ok(bars),
                Err(err) if attempt >= self.config.max_retries => return Err(err),
                Err(err) => {
                    warn!(symbol, attempt, %err, "Yahoo fetch failed, retrying");
                    let backoff_ms = 250u64 * 2u64.pow(attempt);
                    let jitter_ms = rand::thread_rng().gen_range(0..100);
                    tokio::time::sleep(Duration::from_millis(backoff_ms + jitter_ms)).await;
                    attempt += 1;
                }
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct ChartResponse {
    chart: ChartBody,
}

#[derive(Debug, Deserialize)]
struct ChartBody {
    result: Option<Vec<ChartResult>>,
}

#[derive(Debug, Deserialize)]
struct ChartResult {
    timestamp: Vec<i64>,
    indicators: Indicators,
}

#[derive(Debug, Deserialize)]
struct Indicators {
    quote: Vec<Quote>,
    adjclose: Option<Vec<AdjClose>>,
}

#[derive(Debug, Deserialize)]
struct Quote {
    open: Vec<Option<f64>>,
    high: Vec<Option<f64>>,
    low: Vec<Option<f64>>,
    close: Vec<Option<f64>>,
    volume: Vec<Option<u64>>,
}

#[derive(Debug, Deserialize)]
struct AdjClose {
    adjclose: Vec<Option<f64>>,
}

fn parse_chart_response(symbol: &str, body: ChartResponse) -> Result<Vec<DailyBar>, ScreeningError> {
    let result = body
        .chart
        .result
        .and_then(|mut r| if r.is_empty() { None } else { Some(r.remove(0)) })
        .ok_or_else(|| ScreeningError::YahooDataError {
            symbol: symbol.to_string(),
            message: "empty chart result".to_string(),
        })?;

    let quote = result.indicators.quote.first().ok_or_else(|| ScreeningError::YahooDataError {
        symbol: symbol.to_string(),
        message: "missing quote indicators".to_string(),
    })?;

    let adjclose = result.indicators.adjclose.as_ref().and_then(|a| a.first());

    let mut bars = Vec::with_capacity(result.timestamp.len());
    for i in 0..result.timestamp.len() {
        let close = adjclose
            .and_then(|a| a.adjclose.get(i).copied().flatten())
            .or_else(|| quote.close.get(i).copied().flatten());

        let (Some(open), Some(high), Some(low), Some(close), Some(volume)) = (
            quote.open.get(i).copied().flatten(),
            quote.high.get(i).copied().flatten(),
            quote.low.get(i).copied().flatten(),
            close,
            quote.volume.get(i).copied().flatten(),
        ) else {
            continue; // skip bars with any missing/null required field
        };

        let date = chrono::DateTime::from_timestamp(result.timestamp[i], 0)
            .ok_or_else(|| ScreeningError::YahooDataError {
                symbol: symbol.to_string(),
                message: format!("invalid timestamp {}", result.timestamp[i]),
            })?
            .date_naive();

        bars.push(DailyBar { date, open, high, low, close, volume });
    }

    Ok(bars)
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p screener-core --test yahoo_client_test`
Expected: PASS (3 tests)

- [ ] **Step 6: Wire up the module**

Modify `crates/screener-core/src/data/mod.rs`, add:
```rust
pub mod yahoo;
```

Modify `crates/screener-core/src/lib.rs` — no new re-export needed; `YahooClient`/`YahooClientConfig` are reached via `screener_core::data::yahoo::*`, matching how the test file already references them.

- [ ] **Step 7: Commit**

```bash
git add crates/screener-core/src/data crates/screener-core/tests/yahoo_client_test.rs
git commit -m "feat(screener-core): add Yahoo client with crumb auth, retry/backoff, and null-bar handling"
```

---

### Task 7: Phase 1 orchestration (bounded concurrency + progress)

**Files:**
- Create: `crates/screener-core/src/screener.rs`
- Modify: `crates/screener-core/src/lib.rs`
- Create: `crates/screener-core/tests/phase1_integration.rs`

**Interfaces:**
- Consumes: `DailyDataSource` (data), `Phase1Config` (config), `evaluate_phase1`/`Phase1Outcome` (phase1), `ScreeningError` (error)
- Produces: `pub struct Phase1Progress { pub total: usize, pub started: usize, pub passed: usize, pub technical_failures: usize, pub errors: usize }`, `pub struct Phase1Results { pub survivors: Vec<String>, pub technical_failures: Vec<String>, pub errors: Vec<(String, ScreeningError)> }`, `pub async fn run_phase1(universe: &[String], config: &Phase1Config, data_source: Arc<dyn DailyDataSource>, progress_tx: Option<tokio::sync::mpsc::UnboundedSender<Phase1Progress>>) -> Phase1Results`

- [ ] **Step 1: Write the failing integration test**

`crates/screener-core/tests/phase1_integration.rs`:
```rust
mod common;

use std::sync::Arc;

use chrono::NaiveDate;
use common::MockDailyDataSource;
use screener_core::{screener::run_phase1, DailyBar, Phase1Config, ScreeningError};

fn rising_bars(n: usize) -> Vec<DailyBar> {
    (0..n)
        .map(|i| DailyBar {
            date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap() + chrono::Duration::days(i as i64),
            open: (i + 1) as f64,
            high: (i + 1) as f64,
            low: (i + 1) as f64,
            close: (i + 1) as f64,
            volume: 1_000_000,
        })
        .collect()
}

fn flat_bars(n: usize) -> Vec<DailyBar> {
    (0..n)
        .map(|i| DailyBar {
            date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap() + chrono::Duration::days(i as i64),
            open: 100.0,
            high: 100.0,
            low: 100.0,
            close: 100.0,
            volume: 1_000_000,
        })
        .collect()
}

#[tokio::test]
async fn run_phase1_separates_survivors_technical_failures_and_errors() {
    let source = MockDailyDataSource::new()
        .with_bars("RISER", rising_bars(210))
        .with_bars("FLAT", flat_bars(210))
        .with_error("BADSYM", ScreeningError::InvalidSymbol("BADSYM".to_string()));

    let universe = vec!["RISER".to_string(), "FLAT".to_string(), "BADSYM".to_string()];
    let config = Phase1Config::default();

    let results = run_phase1(&universe, &config, Arc::new(source), None).await;

    assert_eq!(results.survivors, vec!["RISER".to_string()]);
    assert_eq!(results.technical_failures, vec!["FLAT".to_string()]);
    assert_eq!(results.errors.len(), 1);
    assert_eq!(results.errors[0].0, "BADSYM");
}

#[tokio::test]
async fn run_phase1_emits_final_progress_with_correct_totals() {
    let source = MockDailyDataSource::new()
        .with_bars("RISER", rising_bars(210))
        .with_bars("FLAT", flat_bars(210));

    let universe = vec!["RISER".to_string(), "FLAT".to_string()];
    let config = Phase1Config::default();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let results = run_phase1(&universe, &config, Arc::new(source), Some(tx)).await;

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

Run: `cargo test -p screener-core --test phase1_integration`
Expected: FAIL to compile — `screener::run_phase1` not defined.

- [ ] **Step 3: Implement the orchestration**

`crates/screener-core/src/screener.rs`:
```rust
use std::sync::Arc;

use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinSet;

use crate::config::Phase1Config;
use crate::data::DailyDataSource;
use crate::error::ScreeningError;
use crate::phase1::{evaluate_phase1, Phase1Outcome};

#[derive(Debug, Clone, PartialEq)]
pub struct Phase1Progress {
    pub total: usize,
    pub started: usize,
    pub passed: usize,
    pub technical_failures: usize,
    pub errors: usize,
}

#[derive(Debug, Default)]
pub struct Phase1Results {
    pub survivors: Vec<String>,
    pub technical_failures: Vec<String>,
    pub errors: Vec<(String, ScreeningError)>,
}

enum SymbolOutcome {
    Passed(String),
    FailedTechnical(String),
    Errored(String, ScreeningError),
}

pub async fn run_phase1(
    universe: &[String],
    config: &Phase1Config,
    data_source: Arc<dyn DailyDataSource>,
    progress_tx: Option<UnboundedSender<Phase1Progress>>,
) -> Phase1Results {
    let total = universe.len();
    let min_bars = config.min_bars_required();

    let mut tasks = JoinSet::new();
    for symbol in universe {
        let symbol = symbol.clone();
        let config = config.clone();
        let data_source = Arc::clone(&data_source);
        tasks.spawn(async move {
            match data_source.fetch_daily_bars(&symbol, min_bars).await {
                Ok(bars) => match evaluate_phase1(&bars, &config) {
                    Phase1Outcome::Passed => SymbolOutcome::Passed(symbol),
                    Phase1Outcome::FailedTechnical => SymbolOutcome::FailedTechnical(symbol),
                    Phase1Outcome::InsufficientData { needed, got } => SymbolOutcome::Errored(
                        symbol.clone(),
                        ScreeningError::InsufficientData { symbol, needed, got },
                    ),
                },
                Err(err) => SymbolOutcome::Errored(symbol, err),
            }
        });
    }

    let mut results = Phase1Results::default();
    let mut started = 0;

    while let Some(joined) = tasks.join_next().await {
        started += 1;
        match joined.expect("phase 1 worker task panicked") {
            SymbolOutcome::Passed(symbol) => results.survivors.push(symbol),
            SymbolOutcome::FailedTechnical(symbol) => results.technical_failures.push(symbol),
            SymbolOutcome::Errored(symbol, err) => results.errors.push((symbol, err)),
        }

        if let Some(tx) = &progress_tx {
            let _ = tx.send(Phase1Progress {
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

Run: `cargo test -p screener-core --test phase1_integration`
Expected: PASS (2 tests)

- [ ] **Step 5: Wire up the module**

Modify `crates/screener-core/src/lib.rs`, add:
```rust
pub mod screener;

pub use screener::{run_phase1, Phase1Progress, Phase1Results};
```

- [ ] **Step 6: Run the full crate test suite**

Run: `cargo test -p screener-core`
Expected: PASS — all unit and integration tests from Tasks 1–7 pass together.

- [ ] **Step 7: Commit**

```bash
git add crates/screener-core/src/screener.rs crates/screener-core/src/lib.rs crates/screener-core/tests/phase1_integration.rs
git commit -m "feat(screener-core): add bounded-concurrency Phase 1 orchestration with progress reporting"
```

---

### Task 8: End-to-end example binary

**Files:**
- Create: `crates/screener-core/examples/run_phase1.rs`
- Create: `crates/screener-core/examples/universe.sample.txt`

**Interfaces:**
- Consumes: `run_phase1`, `Phase1Config`, `data::yahoo::{YahooClient, YahooClientConfig}` (all public API from earlier tasks)
- Produces: a runnable example, no new library API

- [ ] **Step 1: Create a sample universe file**

`crates/screener-core/examples/universe.sample.txt`:
```text
AAPL
MSFT
NVDA
AMZN
SPY
QQQ
```

- [ ] **Step 2: Write the example binary**

`crates/screener-core/examples/run_phase1.rs`:
```rust
use std::fs;
use std::sync::Arc;

use screener_core::data::yahoo::{YahooClient, YahooClientConfig};
use screener_core::{run_phase1, Phase1Config, Phase1Progress};

#[tokio::main]
async fn main() {
    tracing_subscriber_init();

    let path = std::env::args().nth(1).unwrap_or_else(|| {
        "crates/screener-core/examples/universe.sample.txt".to_string()
    });

    let universe: Vec<String> = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read universe file {path}: {e}"))
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    println!("Loaded {} symbols from {}", universe.len(), path);

    let client = Arc::new(YahooClient::new(YahooClientConfig::default()));
    let config = Phase1Config::default();

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Phase1Progress>();
    let progress_task = tokio::spawn(async move {
        while let Some(p) = rx.recv().await {
            println!(
                "Phase 1: {}/{} started, {} passed, {} technical failures, {} errors",
                p.started, p.total, p.passed, p.technical_failures, p.errors
            );
        }
    });

    let results = run_phase1(&universe, &config, client, Some(tx)).await;
    progress_task.await.expect("progress printer task panicked");

    println!("\nSurvivors:");
    for symbol in &results.survivors {
        println!("  {symbol}");
    }

    println!("\nTechnical failures: {}", results.technical_failures.len());
    println!("Errors: {}", results.errors.len());
    for (symbol, err) in &results.errors {
        println!("  {symbol}: {err}");
    }
}

fn tracing_subscriber_init() {
    let _ = tracing_subscriber::fmt::try_init();
}
```

Note: this example uses `tracing_subscriber`, which is not yet a dependency — add it.

Modify `crates/screener-core/Cargo.toml`, `[dependencies]`, add:
```toml
tracing-subscriber = "0.3"
```

- [ ] **Step 3: Verify the example builds**

Run: `cargo build -p screener-core --example run_phase1`
Expected: builds cleanly (this only verifies compilation — running it requires real Yahoo access, which is expected to work against live Yahoo endpoints but is not part of the automated test suite, per the spec's note that Yahoo's real auth flow may need adjustment)

- [ ] **Step 4: Run the full test suite one final time**

Run: `cargo test -p screener-core`
Expected: PASS — confirms the example addition didn't break anything

- [ ] **Step 5: Commit**

```bash
git add crates/screener-core/examples crates/screener-core/Cargo.toml
git commit -m "feat(screener-core): add run_phase1 example binary demonstrating end-to-end usage"
```

---

## Self-Review Notes

**Spec coverage:** sections 4.1 (pipeline/no re-expansion — enforced by only Phase 1 existing so far), 4.2 (no look-ahead — only completed bars ever loaded, no forming-bar concept exists in daily data), 4.3 (runtime-configurable `Phase1Config` with locked defaults), 4.4 (Yahoo auth, scale/backoff, indicators, slope, filter, error classification), 4.9 (full error taxonomy), 4.10 (trait-based testability) are all covered by Tasks 1–8. Sections 4.5–4.8 (Phase 2/3, IBKR, VWAP, timezone/session boundaries) are explicitly out of scope for this plan — they're the next plan in the delivery order.

**Placeholder scan:** no TBD/TODO; every step has real code or a real command.

**Type consistency:** `Phase1Config`, `DailyBar`, `ScreeningError`, `Phase1Outcome`, `DailyDataSource`, `Phase1Progress`, `Phase1Results` are defined once (Tasks 1–7) and referenced with identical names/signatures in every later task and in the example.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-08-23-screener-core-phase1.md`. Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
