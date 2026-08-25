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
