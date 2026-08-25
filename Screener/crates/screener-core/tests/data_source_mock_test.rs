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
