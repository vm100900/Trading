# screener-core Phase 3 (2-Minute IBKR + Current-Session VWAP Filter) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the `screener-core` engine with the Phase 3 filter — SMA20/50/100 plus a current-session (09:30 America/New_York reset) VWAP condition on 2-minute IBKR bars — the final stage of the sequential screening pipeline.

**Architecture:** This plan is intentionally small because Phase 3 reuses everything IBKR-related from the Phase 2 plan without changes: `IbkrClient` already implements `IntradayDataSource` for `IntradayBarSize::TwoMinutes` (wired in the Phase 2 plan's Task 8), and the rate limiter/contract cache need no Phase-3-specific work. What's new is purely domain logic: a session-boundary calculation (`session.rs`, using `chrono-tz` for DST-correct America/New_York conversion), a VWAP indicator (`indicators/vwap.rs`), `Phase3Config`, `evaluate_phase3`, and `run_phase3` orchestration — mirroring the exact shape of `evaluate_phase1`/`run_phase1` and `evaluate_phase2`/`run_phase2`.

**Tech Stack:** `chrono-tz` (new dependency, for DST-correct America/New_York conversion — verified against its real API before writing this plan, not from memory); everything else is already in the crate (`chrono`, `tokio`, `async-trait`, existing `IntradayDataSource`/`IbkrClient`).

**Spec:** `/home/vijay/Study/Screener/docs/superpowers/specs/2026-08-23-rust-screener-design.md` (section 4.6 Phase 3; the timezone/session-boundary portion of section 4.7 that Phase 1 and Phase 2 explicitly deferred here)

**Builds on:** `/home/vijay/Study/Screener/docs/superpowers/plans/2026-08-23-screener-core-phase1.md` and `/home/vijay/Study/Screener/docs/superpowers/plans/2026-08-24-screener-core-phase2-ibkr.md`, both implemented and committed on `master` in `/home/vijay/Study/Screener/crates/screener-core`. This plan assumes: `models.rs` (`IntradayBar`), `indicators/sma.rs` (`sma_at`), `data/traits.rs` (`IntradayDataSource`), `data/ibkr/client.rs` (`IbkrClient`, which already handles `IntradayBarSize::TwoMinutes`), `screener.rs` (`run_phase2` as the pattern to mirror), `tests/common/mod.rs` (`MockIntradayDataSource`), all re-exported from `lib.rs`.

## Note on `chrono-tz`

`chrono_tz::America::New_York` and the `TimeZone::from_local_datetime` → `LocalResult<DateTime<Tz>>` pattern used in Task 1 were verified directly against `chrono-tz` 0.10's docs at plan-writing time. The 2026 US DST transition dates used in Task 1's tests (spring forward 2026-03-08, fall back 2026-11-01 — both computed programmatically from the "2nd Sunday of March" / "1st Sunday of November" rule, not guessed) are real calendar facts, not something that needs runtime verification the way `ibapi`'s API did.

## Global Constraints

- Phase 3 defaults: SMA20, SMA50, SMA100 — no slope condition (unlike Phase 1/2).
- Default Phase 3 requires ≥100 valid completed 2-minute bars (just `sma100_period` — no slope lookback to add, unlike Phase 1/2).
- All filter comparisons are strict `>` — never `>=`.
- VWAP resets at 09:30 America/New_York and only includes bars from the current trading session — bars from a prior session must never contribute, even if they're part of the same fetched bar history.
- VWAP uses each bar's typical price `(high + low + close) / 3`, volume-weighted.
- If cumulative session volume is zero or invalid, no signal is generated — this is distinct from both `FailedTechnical` and `InsufficientData` (there's enough data; the *volume* specifically can't support a VWAP).
- The price for every Phase 3 comparison is the latest **completed** 2-minute close — structurally guaranteed here since IBKR's historical data endpoint only ever returns completed bars (same reasoning as Phase 2).
- All session-boundary timezone math is explicit `America/New_York`, never OS local time, and must be DST-correct.
- Only Phase 2 survivors may reach Phase 3 — enforced by the caller passing that survivor list as `run_phase3`'s universe, same pattern as `run_phase1`/`run_phase2`.

---

## File Structure

```text
crates/screener-core/src/
├── session.rs                    # CREATE: session_start_utc (DST-correct 09:30 ET boundary)
├── config.rs                      # MODIFY: add Phase3Config
├── indicators/
│   ├── mod.rs                      # MODIFY: add vwap re-export
│   └── vwap.rs                      # CREATE: calculate_vwap
├── phase3.rs                        # CREATE: evaluate_phase3 (mirrors phase1.rs/phase2.rs)
├── screener.rs                       # MODIFY: add run_phase3
└── lib.rs                             # MODIFY: wire up new modules/re-exports

crates/screener-core/tests/
└── phase3_integration.rs           # CREATE: run_phase3 end-to-end against the mock

crates/screener-core/examples/
├── run_phase1_then_phase2.rs       # DELETE: superseded by run_all_phases.rs
└── run_all_phases.rs                # CREATE: Phase 1 -> 2 -> 3 against real Yahoo + IBKR

crates/screener-core/Cargo.toml     # MODIFY: add chrono-tz
```

---

### Task 1: Session boundary computation

**Files:**
- Create: `crates/screener-core/src/session.rs`
- Modify: `crates/screener-core/src/lib.rs`
- Modify: `crates/screener-core/Cargo.toml`

**Interfaces:**
- Consumes: nothing from earlier tasks (pure `chrono`/`chrono-tz` math)
- Produces: `pub fn session_start_utc(at: chrono::DateTime<chrono::Utc>) -> Option<chrono::DateTime<chrono::Utc>>` — returns the UTC instant for 09:30 America/New_York on the trading day containing `at`

- [ ] **Step 1: Add the dependency**

Modify `crates/screener-core/Cargo.toml`, `[dependencies]`, add:
```toml
chrono-tz = "0.10"
```

- [ ] **Step 2: Write the failing tests**

`crates/screener-core/src/session.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn utc(y: i32, m: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, mi, 0)
            .unwrap()
            .and_utc()
    }

    #[test]
    fn session_start_in_winter_est_is_1430_utc() {
        // 2026-01-15 is well before the 2026-03-08 US spring-forward
        // transition, so EST (UTC-5) applies: 09:30 ET = 14:30 UTC.
        let at = utc(2026, 1, 15, 20, 0);
        let start = session_start_utc(at).unwrap();
        assert_eq!(start, utc(2026, 1, 15, 14, 30));
    }

    #[test]
    fn session_start_in_summer_edt_is_1330_utc() {
        // 2026-06-15 is well after spring-forward and before fall-back, so
        // EDT (UTC-4) applies: 09:30 ET = 13:30 UTC.
        let at = utc(2026, 6, 15, 20, 0);
        let start = session_start_utc(at).unwrap();
        assert_eq!(start, utc(2026, 6, 15, 13, 30));
    }

    #[test]
    fn session_start_the_day_after_spring_forward_uses_edt() {
        // US DST began 2026-03-08 at 02:00 local; 2026-03-09 is fully EDT.
        let at = utc(2026, 3, 9, 20, 0);
        let start = session_start_utc(at).unwrap();
        assert_eq!(start, utc(2026, 3, 9, 13, 30));
    }

    #[test]
    fn session_start_the_day_after_fall_back_uses_est() {
        // US DST ended 2026-11-01 at 02:00 local; 2026-11-02 is fully EST.
        let at = utc(2026, 11, 2, 20, 0);
        let start = session_start_utc(at).unwrap();
        assert_eq!(start, utc(2026, 11, 2, 14, 30));
    }

    #[test]
    fn session_start_is_the_same_regardless_of_time_of_day_within_the_session() {
        let morning = utc(2026, 1, 15, 15, 0);
        let afternoon = utc(2026, 1, 15, 20, 0);
        assert_eq!(session_start_utc(morning), session_start_utc(afternoon));
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p screener-core session::tests`
Expected: FAIL to compile — `session_start_utc` not defined.

- [ ] **Step 4: Implement `session_start_utc`**

Add above the `#[cfg(test)]` module in `crates/screener-core/src/session.rs`:

```rust
use chrono::{DateTime, TimeZone, Utc};
use chrono_tz::America::New_York;

/// Returns the UTC instant corresponding to 09:30 America/New_York on the
/// trading day containing `at` (also a UTC instant). Returns `None` only in
/// the practically-impossible case that 09:30 local time doesn't resolve to
/// a single unambiguous instant — US DST transitions happen at 02:00 local
/// time, nowhere near 09:30, so this should never actually occur.
pub fn session_start_utc(at: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let local = at.with_timezone(&New_York);
    let trading_date = local.date_naive();
    let session_start_naive = trading_date.and_hms_opt(9, 30, 0)?;
    let session_start_local = New_York.from_local_datetime(&session_start_naive).single()?;
    Some(session_start_local.with_timezone(&Utc))
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p screener-core session::tests`
Expected: PASS (5 tests)

- [ ] **Step 6: Wire up the module**

Modify `crates/screener-core/src/lib.rs`, add:
```rust
pub mod session;
```
and:
```rust
pub use session::session_start_utc;
```

- [ ] **Step 7: Commit**

```bash
git add crates/screener-core/src/session.rs crates/screener-core/src/lib.rs crates/screener-core/Cargo.toml
git commit -m "feat(screener-core): add DST-correct America/New_York session-boundary computation"
```

---

### Task 2: VWAP indicator

**Files:**
- Create: `crates/screener-core/src/indicators/vwap.rs`
- Modify: `crates/screener-core/src/indicators/mod.rs`

**Interfaces:**
- Consumes: `IntradayBar` (models)
- Produces: `pub fn calculate_vwap<'a>(bars: impl IntoIterator<Item = &'a IntradayBar>) -> Option<f64>`

- [ ] **Step 1: Write the failing tests**

`crates/screener-core/src/indicators/vwap.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

    fn bar(high: f64, low: f64, close: f64, volume: f64) -> IntradayBar {
        IntradayBar {
            timestamp: DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            open: close,
            high,
            low,
            close,
            volume,
        }
    }

    #[test]
    fn vwap_is_volume_weighted_average_of_typical_prices() {
        // bar 1: typical = (11+9+10)/3 = 10.0, volume 100 -> pv 1000
        // bar 2: typical = (21+19+20)/3 = 20.0, volume 300 -> pv 6000
        // vwap = (1000+6000)/(100+300) = 17.5
        let bars = vec![bar(11.0, 9.0, 10.0, 100.0), bar(21.0, 19.0, 20.0, 300.0)];
        let vwap = calculate_vwap(bars.iter()).unwrap();
        assert!((vwap - 17.5).abs() < 1e-9);
    }

    #[test]
    fn vwap_is_none_for_empty_bars() {
        let bars: Vec<IntradayBar> = vec![];
        assert_eq!(calculate_vwap(bars.iter()), None);
    }

    #[test]
    fn vwap_is_none_when_cumulative_volume_is_zero() {
        let bars = vec![bar(11.0, 9.0, 10.0, 0.0), bar(21.0, 19.0, 20.0, 0.0)];
        assert_eq!(calculate_vwap(bars.iter()), None);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p screener-core indicators::vwap::tests`
Expected: FAIL to compile — `calculate_vwap` not defined.

- [ ] **Step 3: Implement `calculate_vwap`**

Add above the `#[cfg(test)]` module in `crates/screener-core/src/indicators/vwap.rs`:

```rust
use crate::models::IntradayBar;

/// Volume-weighted average price over the given bars, using each bar's
/// typical price `(high + low + close) / 3`. Returns `None` if cumulative
/// volume is zero or negative — callers must not generate a signal from a
/// `None` result.
pub fn calculate_vwap<'a>(bars: impl IntoIterator<Item = &'a IntradayBar>) -> Option<f64> {
    let mut cumulative_pv = 0.0;
    let mut cumulative_volume = 0.0;
    for bar in bars {
        let typical_price = (bar.high + bar.low + bar.close) / 3.0;
        cumulative_pv += typical_price * bar.volume;
        cumulative_volume += bar.volume;
    }
    if cumulative_volume <= 0.0 {
        None
    } else {
        Some(cumulative_pv / cumulative_volume)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p screener-core indicators::vwap::tests`
Expected: PASS (3 tests)

- [ ] **Step 5: Wire up the module**

Modify `crates/screener-core/src/indicators/mod.rs`, add:
```rust
pub mod vwap;
```
and:
```rust
pub use vwap::calculate_vwap;
```

- [ ] **Step 6: Commit**

```bash
git add crates/screener-core/src/indicators
git commit -m "feat(screener-core): add VWAP indicator (calculate_vwap)"
```

---

### Task 3: Phase3Config

**Files:**
- Modify: `crates/screener-core/src/config.rs`
- Modify: `crates/screener-core/src/lib.rs`

**Interfaces:**
- Consumes: nothing
- Produces: `pub struct Phase3Config { sma20_period, sma50_period, sma100_period: usize, enable_close_gt_sma20, enable_sma20_gt_sma50, enable_sma50_gt_sma100, enable_vwap: bool }` implementing `Default`, plus `pub fn min_bars_required(&self) -> usize`

- [ ] **Step 1: Write the failing tests**

Add to `crates/screener-core/src/config.rs`, appended inside the existing `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn phase3_defaults_match_locked_spec() {
        let cfg = Phase3Config::default();
        assert_eq!(cfg.sma20_period, 20);
        assert_eq!(cfg.sma50_period, 50);
        assert_eq!(cfg.sma100_period, 100);
        assert!(cfg.enable_close_gt_sma20);
        assert!(cfg.enable_sma20_gt_sma50);
        assert!(cfg.enable_sma50_gt_sma100);
        assert!(cfg.enable_vwap);
    }

    #[test]
    fn phase3_default_min_bars_required_is_100() {
        let cfg = Phase3Config::default();
        assert_eq!(cfg.min_bars_required(), 100);
    }

    #[test]
    fn phase3_min_bars_required_tracks_custom_periods() {
        let mut cfg = Phase3Config::default();
        cfg.sma100_period = 150;
        assert_eq!(cfg.min_bars_required(), 150);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p screener-core config::tests`
Expected: FAIL to compile — `Phase3Config` not defined.

- [ ] **Step 3: Implement `Phase3Config`**

Add to `crates/screener-core/src/config.rs`, above the `#[cfg(test)]` module, alongside `Phase1Config`/`Phase2Config`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Phase3Config {
    pub sma20_period: usize,
    pub sma50_period: usize,
    pub sma100_period: usize,
    pub enable_close_gt_sma20: bool,
    pub enable_sma20_gt_sma50: bool,
    pub enable_sma50_gt_sma100: bool,
    pub enable_vwap: bool,
}

impl Default for Phase3Config {
    fn default() -> Self {
        Self {
            sma20_period: 20,
            sma50_period: 50,
            sma100_period: 100,
            enable_close_gt_sma20: true,
            enable_sma20_gt_sma50: true,
            enable_sma50_gt_sma100: true,
            enable_vwap: true,
        }
    }
}

impl Phase3Config {
    /// Minimum completed 2-minute bars required to evaluate this config's active conditions.
    /// Unlike Phase1Config/Phase2Config there is no slope lookback to add — Phase 3 has no
    /// slope condition.
    pub fn min_bars_required(&self) -> usize {
        self.sma20_period.max(self.sma50_period).max(self.sma100_period)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p screener-core config::tests`
Expected: PASS (10 tests: 4 Phase1Config + 3 Phase2Config + 3 Phase3Config)

- [ ] **Step 5: Wire up re-exports**

Modify `crates/screener-core/src/lib.rs` — change:
```rust
pub use config::{Phase1Config, Phase2Config};
```
to:
```rust
pub use config::{Phase1Config, Phase2Config, Phase3Config};
```

- [ ] **Step 6: Commit**

```bash
git add crates/screener-core/src/config.rs crates/screener-core/src/lib.rs
git commit -m "feat(screener-core): add Phase3Config with locked defaults"
```

---

### Task 4: Phase 3 filter logic

**Files:**
- Create: `crates/screener-core/src/phase3.rs`
- Modify: `crates/screener-core/src/lib.rs`

**Interfaces:**
- Consumes: `IntradayBar` (models), `Phase3Config` (config), `sma_at` (indicators::sma), `calculate_vwap` (indicators::vwap), `session_start_utc` (session)
- Produces: `pub enum Phase3Outcome { Passed, FailedTechnical, InsufficientData { needed: usize, got: usize }, NoVwapSignal }` (`Debug + Clone + PartialEq`), `pub fn evaluate_phase3(bars: &[IntradayBar], config: &Phase3Config) -> Phase3Outcome`

- [ ] **Step 1: Write the failing tests**

`crates/screener-core/src/phase3.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, NaiveDate};

    fn bars_from_closes(closes: &[f64]) -> Vec<IntradayBar> {
        closes
            .iter()
            .enumerate()
            .map(|(i, &close)| IntradayBar {
                timestamp: DateTime::from_timestamp(1_700_000_000 + (i as i64) * 120, 0).unwrap(),
                open: close,
                high: close,
                low: close,
                close,
                volume: 1_000.0,
            })
            .collect()
    }

    /// 100 bars, 2-minute spacing, starting 08:00 ET (13:00 UTC) on
    /// 2026-01-15 (EST). The first 45 bars (08:00-09:28 ET) are PRE-session;
    /// the remaining 55 (09:30 ET onward, session boundary 14:30 UTC) are
    /// the current session. Closes ascend 1.0..=100.0 across ALL bars so
    /// the SMA-stack conditions pass regardless of the session split.
    /// Pre-session bars carry a deliberately huge typical price (10,000)
    /// and volume (1,000,000) so that if the session-reset filter is
    /// broken and they leak into VWAP, the test fails loudly instead of
    /// silently passing for the wrong reason.
    fn bars_with_session_boundary() -> Vec<IntradayBar> {
        let start = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap().and_hms_opt(13, 0, 0).unwrap().and_utc();
        let session_boundary = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap().and_hms_opt(14, 30, 0).unwrap().and_utc();

        (0..100i64)
            .map(|i| {
                let timestamp = start + chrono::Duration::minutes(2 * i);
                let close = (i + 1) as f64;
                if timestamp < session_boundary {
                    IntradayBar { timestamp, open: 10_000.0, high: 10_000.0, low: 10_000.0, close, volume: 1_000_000.0 }
                } else {
                    IntradayBar { timestamp, open: close, high: close, low: close, close, volume: 1_000.0 }
                }
            })
            .collect()
    }

    #[test]
    fn insufficient_bars_fails_with_needed_and_got() {
        let bars = bars_from_closes(&[1.0, 2.0, 3.0]);
        let config = Phase3Config::default();
        assert_eq!(evaluate_phase3(&bars, &config), Phase3Outcome::InsufficientData { needed: 100, got: 3 });
    }

    #[test]
    fn flat_series_fails_technical_when_vwap_disabled() {
        let closes = vec![50.0; 100];
        let bars = bars_from_closes(&closes);
        let mut config = Phase3Config::default();
        config.enable_vwap = false;
        assert_eq!(evaluate_phase3(&bars, &config), Phase3Outcome::FailedTechnical);
    }

    #[test]
    fn strictly_ascending_closes_with_vwap_disabled_passes() {
        let closes: Vec<f64> = (1..=100).map(|i| i as f64).collect();
        let bars = bars_from_closes(&closes);
        let mut config = Phase3Config::default();
        config.enable_vwap = false;
        assert_eq!(evaluate_phase3(&bars, &config), Phase3Outcome::Passed);
    }

    #[test]
    fn vwap_condition_only_considers_current_session_bars() {
        // If session-reset were broken (pre-session bars leaking into
        // VWAP), the huge pre-session price/volume would push VWAP far
        // above the final close (100.0), flipping this to FailedTechnical.
        let bars = bars_with_session_boundary();
        let config = Phase3Config::default();
        assert_eq!(evaluate_phase3(&bars, &config), Phase3Outcome::Passed);
    }

    #[test]
    fn zero_session_volume_yields_no_vwap_signal() {
        let start = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap().and_hms_opt(14, 30, 0).unwrap().and_utc();
        let bars: Vec<IntradayBar> = (0..100i64)
            .map(|i| {
                let close = (i + 1) as f64;
                IntradayBar {
                    timestamp: start + chrono::Duration::minutes(2 * i),
                    open: close,
                    high: close,
                    low: close,
                    close,
                    volume: 0.0,
                }
            })
            .collect();
        let config = Phase3Config::default();
        assert_eq!(evaluate_phase3(&bars, &config), Phase3Outcome::NoVwapSignal);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p screener-core phase3::tests`
Expected: FAIL to compile — `evaluate_phase3`/`Phase3Outcome` not defined.

- [ ] **Step 3: Implement `evaluate_phase3`**

Add above the `#[cfg(test)]` module in `crates/screener-core/src/phase3.rs`:

```rust
use crate::config::Phase3Config;
use crate::indicators::sma::sma_at;
use crate::indicators::vwap::calculate_vwap;
use crate::models::IntradayBar;
use crate::session::session_start_utc;

#[derive(Debug, Clone, PartialEq)]
pub enum Phase3Outcome {
    Passed,
    FailedTechnical,
    InsufficientData { needed: usize, got: usize },
    /// Cumulative current-session volume was zero or invalid — per spec,
    /// this must not generate a signal, and is distinct from both a
    /// technical failure and insufficient data (there IS enough bar data;
    /// specifically the volume can't support a VWAP).
    NoVwapSignal,
}

pub fn evaluate_phase3(bars: &[IntradayBar], config: &Phase3Config) -> Phase3Outcome {
    let needed = config.min_bars_required();
    if bars.len() < needed {
        return Phase3Outcome::InsufficientData {
            needed,
            got: bars.len(),
        };
    }

    let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
    let close = *closes.last().expect("checked non-empty via min_bars_required");

    let sma20 = sma_at(&closes, config.sma20_period, 0);
    let sma50 = sma_at(&closes, config.sma50_period, 0);
    let sma100 = sma_at(&closes, config.sma100_period, 0);

    let (sma20, sma50, sma100) = match (sma20, sma50, sma100) {
        (Some(a), Some(b), Some(c)) => (a, b, c),
        _ => return Phase3Outcome::InsufficientData { needed, got: bars.len() },
    };

    if config.enable_close_gt_sma20 && !(close > sma20) {
        return Phase3Outcome::FailedTechnical;
    }
    if config.enable_sma20_gt_sma50 && !(sma20 > sma50) {
        return Phase3Outcome::FailedTechnical;
    }
    if config.enable_sma50_gt_sma100 && !(sma50 > sma100) {
        return Phase3Outcome::FailedTechnical;
    }

    if config.enable_vwap {
        let last_bar = bars.last().expect("checked non-empty via min_bars_required");
        let Some(session_start) = session_start_utc(last_bar.timestamp) else {
            return Phase3Outcome::NoVwapSignal;
        };
        let session_bars = bars.iter().filter(|b| b.timestamp >= session_start);
        let vwap = match calculate_vwap(session_bars) {
            Some(v) => v,
            None => return Phase3Outcome::NoVwapSignal,
        };
        if !(close > vwap) {
            return Phase3Outcome::FailedTechnical;
        }
    }

    Phase3Outcome::Passed
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p screener-core phase3::tests`
Expected: PASS (5 tests)

- [ ] **Step 5: Wire up the module**

Modify `crates/screener-core/src/lib.rs`, add:
```rust
pub mod phase3;
```
and:
```rust
pub use phase3::{evaluate_phase3, Phase3Outcome};
```

- [ ] **Step 6: Commit**

```bash
git add crates/screener-core/src/phase3.rs crates/screener-core/src/lib.rs
git commit -m "feat(screener-core): add Phase 3 filter logic (evaluate_phase3)"
```

---

### Task 5: Phase 3 orchestration

**Files:**
- Modify: `crates/screener-core/src/screener.rs`
- Modify: `crates/screener-core/src/lib.rs`
- Create: `crates/screener-core/tests/phase3_integration.rs`

**Interfaces:**
- Consumes: `IntradayDataSource` (data), `Phase3Config` (config), `evaluate_phase3`/`Phase3Outcome` (phase3), `ScreeningError` (error)
- Produces: `pub struct Phase3Progress { pub total: usize, pub started: usize, pub passed: usize, pub technical_failures: usize, pub errors: usize }`, `pub struct Phase3Results { pub survivors: Vec<String>, pub technical_failures: Vec<String>, pub errors: Vec<(String, ScreeningError)> }`, `pub async fn run_phase3(universe: &[String], config: &Phase3Config, data_source: Arc<dyn IntradayDataSource>, progress_tx: Option<tokio::sync::mpsc::UnboundedSender<Phase3Progress>>) -> Phase3Results`

- [ ] **Step 1: Write the failing integration test**

`crates/screener-core/tests/phase3_integration.rs`:

```rust
mod common;

use std::sync::Arc;

use chrono::NaiveDate;
use common::MockIntradayDataSource;
use screener_core::{screener::run_phase3, IntradayBar, Phase3Config};

fn session_bars_from_closes(closes: &[f64]) -> Vec<IntradayBar> {
    let start = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap().and_hms_opt(14, 30, 0).unwrap().and_utc();
    closes
        .iter()
        .enumerate()
        .map(|(i, &close)| IntradayBar {
            timestamp: start + chrono::Duration::minutes(2 * i as i64),
            open: close,
            high: close,
            low: close,
            close,
            volume: 1_000.0,
        })
        .collect()
}

#[tokio::test]
async fn run_phase3_separates_survivors_and_technical_failures() {
    let rising: Vec<f64> = (1..=100).map(|i| i as f64).collect();
    let flat = vec![50.0; 100];

    let source = MockIntradayDataSource::new()
        .with_bars("RISER", session_bars_from_closes(&rising))
        .with_bars("FLAT", session_bars_from_closes(&flat));

    let universe = vec!["RISER".to_string(), "FLAT".to_string()];
    let config = Phase3Config::default();

    let results = run_phase3(&universe, &config, Arc::new(source), None).await;

    assert_eq!(results.survivors, vec!["RISER".to_string()]);
    assert_eq!(results.technical_failures, vec!["FLAT".to_string()]);
}

#[tokio::test]
async fn run_phase3_emits_final_progress_with_correct_totals() {
    let rising: Vec<f64> = (1..=100).map(|i| i as f64).collect();
    let flat = vec![50.0; 100];

    let source = MockIntradayDataSource::new()
        .with_bars("RISER", session_bars_from_closes(&rising))
        .with_bars("FLAT", session_bars_from_closes(&flat));

    let universe = vec!["RISER".to_string(), "FLAT".to_string()];
    let config = Phase3Config::default();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let results = run_phase3(&universe, &config, Arc::new(source), Some(tx)).await;

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

Run: `cargo test -p screener-core --test phase3_integration`
Expected: FAIL to compile — `screener::run_phase3` not defined.

- [ ] **Step 3: Implement the orchestration**

Modify `crates/screener-core/src/screener.rs`. Update the imports near the top:
```rust
use crate::config::{Phase1Config, Phase2Config, Phase3Config};
use crate::data::{DailyDataSource, IntradayDataSource};
use crate::error::ScreeningError;
use crate::phase1::{evaluate_phase1, Phase1Outcome};
use crate::phase2::{evaluate_phase2, Phase2Outcome};
use crate::phase3::{evaluate_phase3, Phase3Outcome};
```

Add at the end of the file, below `run_phase2`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Phase3Progress {
    pub total: usize,
    pub started: usize,
    pub passed: usize,
    pub technical_failures: usize,
    pub errors: usize,
}

#[derive(Debug, Default)]
pub struct Phase3Results {
    pub survivors: Vec<String>,
    pub technical_failures: Vec<String>,
    pub errors: Vec<(String, ScreeningError)>,
}

enum Phase3SymbolOutcome {
    Passed(String),
    FailedTechnical(String),
    Errored(String, ScreeningError),
}

pub async fn run_phase3(
    universe: &[String],
    config: &Phase3Config,
    data_source: Arc<dyn IntradayDataSource>,
    progress_tx: Option<UnboundedSender<Phase3Progress>>,
) -> Phase3Results {
    let total = universe.len();
    let min_bars = config.min_bars_required();

    let mut tasks = JoinSet::new();
    for symbol in universe {
        let symbol = symbol.clone();
        let config = config.clone();
        let data_source = Arc::clone(&data_source);
        tasks.spawn(async move {
            match data_source
                .fetch_intraday_bars(&symbol, crate::models::IntradayBarSize::TwoMinutes, min_bars)
                .await
            {
                Ok(bars) => match evaluate_phase3(&bars, &config) {
                    Phase3Outcome::Passed => Phase3SymbolOutcome::Passed(symbol),
                    Phase3Outcome::FailedTechnical => Phase3SymbolOutcome::FailedTechnical(symbol),
                    Phase3Outcome::InsufficientData { needed, got } => Phase3SymbolOutcome::Errored(
                        symbol.clone(),
                        ScreeningError::InsufficientData { symbol, needed, got },
                    ),
                    Phase3Outcome::NoVwapSignal => Phase3SymbolOutcome::Errored(
                        symbol.clone(),
                        ScreeningError::CalculationError(format!(
                            "no VWAP signal for {symbol}: zero or invalid session volume"
                        )),
                    ),
                },
                Err(err) => Phase3SymbolOutcome::Errored(symbol, err),
            }
        });
    }

    let mut results = Phase3Results::default();
    let mut started = 0;

    while let Some(joined) = tasks.join_next().await {
        started += 1;
        match joined.expect("phase 3 worker task panicked") {
            Phase3SymbolOutcome::Passed(symbol) => results.survivors.push(symbol),
            Phase3SymbolOutcome::FailedTechnical(symbol) => results.technical_failures.push(symbol),
            Phase3SymbolOutcome::Errored(symbol, err) => results.errors.push((symbol, err)),
        }

        if let Some(tx) = &progress_tx {
            let _ = tx.send(Phase3Progress {
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

Run: `cargo test -p screener-core --test phase3_integration`
Expected: PASS (2 tests)

- [ ] **Step 5: Wire up re-exports**

Modify `crates/screener-core/src/lib.rs` — change:
```rust
pub use screener::{run_phase1, run_phase2, Phase1Progress, Phase1Results, Phase2Progress, Phase2Results};
```
to:
```rust
pub use screener::{
    run_phase1, run_phase2, run_phase3, Phase1Progress, Phase1Results, Phase2Progress, Phase2Results,
    Phase3Progress, Phase3Results,
};
```

- [ ] **Step 6: Run the full crate test suite**

Run: `cargo test -p screener-core`
Expected: PASS — every test from this plan plus the Phase 1 and Phase 2 plans passes together.

- [ ] **Step 7: Commit**

```bash
git add crates/screener-core/src/screener.rs crates/screener-core/src/lib.rs crates/screener-core/tests/phase3_integration.rs
git commit -m "feat(screener-core): add Phase 3 orchestration (run_phase3)"
```

---

### Task 6: End-to-end example binary (Phase 1 -> 2 -> 3)

**Files:**
- Delete: `crates/screener-core/examples/run_phase1_then_phase2.rs`
- Create: `crates/screener-core/examples/run_all_phases.rs`

**Interfaces:**
- Consumes: `run_phase1`, `run_phase2`, `run_phase3`, `Phase1Config`, `Phase2Config`, `Phase3Config`, `data::yahoo::{YahooClient, YahooClientConfig}`, `data::ibkr::IbkrClient` (all public API from this plan and the prior two)
- Produces: a runnable example, no new library API

**This example requires a live/paper IBKR TWS or Gateway instance to actually run end-to-end** — it compiles and can be built here, but running it is out of scope for this environment, same caveat as the Phase 2 plan's example.

- [ ] **Step 1: Remove the superseded example**

```bash
git rm crates/screener-core/examples/run_phase1_then_phase2.rs
```

- [ ] **Step 2: Write the example binary**

`crates/screener-core/examples/run_all_phases.rs`:

```rust
use std::fs;
use std::sync::Arc;

use screener_core::data::ibkr::IbkrClient;
use screener_core::data::yahoo::{YahooClient, YahooClientConfig};
use screener_core::{run_phase1, run_phase2, run_phase3, Phase1Config, Phase2Config, Phase3Config};

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
    let phase1_results = run_phase1(&universe, &Phase1Config::default(), yahoo, None).await;
    println!(
        "Phase 1: {} survivors, {} technical failures, {} errors",
        phase1_results.survivors.len(),
        phase1_results.technical_failures.len(),
        phase1_results.errors.len()
    );
    if phase1_results.survivors.is_empty() {
        println!("No Phase 1 survivors — stopping.");
        return;
    }

    println!("Connecting to IBKR at {ibkr_address}...");
    let ibkr: Arc<IbkrClient> = match IbkrClient::connect(&ibkr_address, 100).await {
        Ok(client) => Arc::new(client),
        Err(err) => {
            eprintln!("Failed to connect to IBKR: {err}");
            return;
        }
    };

    let phase2_results = run_phase2(&phase1_results.survivors, &Phase2Config::default(), Arc::clone(&ibkr) as _, None).await;
    println!(
        "Phase 2: {} survivors, {} technical failures, {} errors",
        phase2_results.survivors.len(),
        phase2_results.technical_failures.len(),
        phase2_results.errors.len()
    );
    if phase2_results.survivors.is_empty() {
        println!("No Phase 2 survivors — stopping.");
        return;
    }

    let phase3_results = run_phase3(&phase2_results.survivors, &Phase3Config::default(), ibkr, None).await;
    println!(
        "Phase 3: {} survivors, {} technical failures, {} errors",
        phase3_results.survivors.len(),
        phase3_results.technical_failures.len(),
        phase3_results.errors.len()
    );

    println!("\nFinal watchlist:");
    for symbol in &phase3_results.survivors {
        println!("  {symbol}");
    }
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p screener-core --example run_all_phases`
Expected: builds cleanly. Note `Arc::clone(&ibkr) as _` — `run_phase2` and `run_phase3` both need `Arc<dyn IntradayDataSource>`, and `ibkr` is `Arc<IbkrClient>`; the `as _` coerces it to the trait-object `Arc`. If this specific coercion syntax doesn't compile against the resolved toolchain, the fix is a local, mechanical one (e.g. `Arc::clone(&ibkr) as Arc<dyn screener_core::IntradayDataSource>` with the full type spelled out) — it does not affect any other task's code.

- [ ] **Step 4: Run the full test suite one final time**

Run: `cargo test -p screener-core`
Expected: PASS — confirms the example addition/removal didn't break anything.

- [ ] **Step 5: Commit**

```bash
git add crates/screener-core/examples
git commit -m "feat(screener-core): add run_all_phases example (Phase 1 -> 2 -> 3), superseding run_phase1_then_phase2"
```

---

## Self-Review Notes

**Spec coverage:** section 4.6 (Phase 3 data/indicators/filter, current-session VWAP with 09:30 America/New_York reset, typical-price/volume-weighted formula, no-signal-on-zero-volume) is covered by Tasks 1, 2, 4, 5. The timezone/session-boundary portion of section 4.7 that Phase 1 and Phase 2 deferred is covered by Task 1's DST-correctness tests against real 2026 transition dates. Section 4.10 (mockable data sources) needs no new work — `run_phase3` reuses the existing `IntradayDataSource`/`MockIntradayDataSource` from the Phase 2 plan unchanged. This plan completes `screener-core` per the spec's delivery order item 2; `screener-service` and `screener-app` remain separate future plans.

**Placeholder scan:** no TBD/TODO. Task 6's Step 3 has an honest, narrow caveat about one specific line's exact syntax (the `Arc` trait-object coercion) rather than glossing over it — this is a stated, scoped risk, not a placeholder.

**Type consistency:** `Phase3Config`, `Phase3Outcome`, `evaluate_phase3`, `Phase3Progress`, `Phase3Results`, `run_phase3`, `session_start_utc`, `calculate_vwap` are each defined once and referenced identically across every task and the example that uses them.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-08-24-screener-core-phase3-vwap.md`. Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
