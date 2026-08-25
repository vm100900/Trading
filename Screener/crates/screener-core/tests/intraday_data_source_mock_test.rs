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
