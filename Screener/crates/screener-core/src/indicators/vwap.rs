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
