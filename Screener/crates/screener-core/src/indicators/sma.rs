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
