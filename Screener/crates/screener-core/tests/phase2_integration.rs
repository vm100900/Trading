mod common;

use std::sync::Arc;

use chrono::DateTime;
use common::MockIntradayDataSource;
use screener_core::{screener::run_phase2, IntradayBar, Phase2Config, ScreeningError};

fn rising_bars(n: usize) -> Vec<IntradayBar> {
    (0..n)
        .map(|i| IntradayBar {
            timestamp: DateTime::from_timestamp(1_700_000_000 + (i as i64) * 1800, 0).unwrap(),
            open: (i + 1) as f64,
            high: (i + 1) as f64,
            low: (i + 1) as f64,
            close: (i + 1) as f64,
            volume: 10_000.0,
        })
        .collect()
}

fn flat_bars(n: usize) -> Vec<IntradayBar> {
    (0..n)
        .map(|i| IntradayBar {
            timestamp: DateTime::from_timestamp(1_700_000_000 + (i as i64) * 1800, 0).unwrap(),
            open: 50.0,
            high: 50.0,
            low: 50.0,
            close: 50.0,
            volume: 10_000.0,
        })
        .collect()
}

#[tokio::test]
async fn run_phase2_separates_survivors_technical_failures_and_errors() {
    let source = MockIntradayDataSource::new()
        .with_bars("RISER", rising_bars(70))
        .with_bars("FLAT", flat_bars(70))
        .with_error("BADSYM", ScreeningError::ContractQualificationFailed("BADSYM".to_string()));

    let universe = vec!["RISER".to_string(), "FLAT".to_string(), "BADSYM".to_string()];
    let config = Phase2Config::default();

    let results = run_phase2(&universe, &config, Arc::new(source), None).await;

    assert_eq!(results.survivors, vec!["RISER".to_string()]);
    assert_eq!(results.technical_failures, vec!["FLAT".to_string()]);
    assert_eq!(results.errors.len(), 1);
    assert_eq!(results.errors[0].0, "BADSYM");
}

#[tokio::test]
async fn run_phase2_emits_final_progress_with_correct_totals() {
    let source = MockIntradayDataSource::new()
        .with_bars("RISER", rising_bars(70))
        .with_bars("FLAT", flat_bars(70));

    let universe = vec!["RISER".to_string(), "FLAT".to_string()];
    let config = Phase2Config::default();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let results = run_phase2(&universe, &config, Arc::new(source), Some(tx)).await;

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
