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
}
