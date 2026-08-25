mod common;

use std::sync::Arc;

use chrono::NaiveDate;
use common::MockDailyDataSource;
use screener_core::{screener::run_phase1, DailyBar, Phase1Config, ScreeningError};

fn rising_bars(n: usize) -> Vec<DailyBar> {
    (0..n)
        .map(|i| DailyBar {
            date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap() + chrono::Duration::days(i as i64),
            open: (i + 1) as f64,
            high: (i + 1) as f64,
            low: (i + 1) as f64,
            close: (i + 1) as f64,
            volume: 1_000_000,
        })
        .collect()
}

fn flat_bars(n: usize) -> Vec<DailyBar> {
    (0..n)
        .map(|i| DailyBar {
            date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap() + chrono::Duration::days(i as i64),
            open: 100.0,
            high: 100.0,
            low: 100.0,
            close: 100.0,
            volume: 1_000_000,
        })
        .collect()
}

#[tokio::test]
async fn run_phase1_separates_survivors_technical_failures_and_errors() {
    let source = MockDailyDataSource::new()
        .with_bars("RISER", rising_bars(210))
        .with_bars("FLAT", flat_bars(210))
        .with_error("BADSYM", ScreeningError::InvalidSymbol("BADSYM".to_string()));

    let universe = vec!["RISER".to_string(), "FLAT".to_string(), "BADSYM".to_string()];
    let config = Phase1Config::default();

    let results = run_phase1(&universe, &config, Arc::new(source), None).await;

    assert_eq!(results.survivors, vec!["RISER".to_string()]);
    assert_eq!(results.technical_failures, vec!["FLAT".to_string()]);
    assert_eq!(results.errors.len(), 1);
    assert_eq!(results.errors[0].0, "BADSYM");
}

#[tokio::test]
async fn run_phase1_emits_final_progress_with_correct_totals() {
    let source = MockDailyDataSource::new()
        .with_bars("RISER", rising_bars(210))
        .with_bars("FLAT", flat_bars(210));

    let universe = vec!["RISER".to_string(), "FLAT".to_string()];
    let config = Phase1Config::default();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let results = run_phase1(&universe, &config, Arc::new(source), Some(tx)).await;

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
