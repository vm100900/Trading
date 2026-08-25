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
