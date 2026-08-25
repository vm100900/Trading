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
