mod common;

use std::sync::Arc;

use chrono::NaiveDate;
use common::MockIntradayDataSource;
use screener_core::{screener::run_phase3, IntradayBar, Phase3Config};

fn session_bars_from_closes(closes: &[f64]) -> Vec<IntradayBar> {
    let start = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap().and_hms_opt(14, 30, 0).unwrap().and_utc();
    closes
        .iter()
        .enumerate()
        .map(|(i, &close)| IntradayBar {
            timestamp: start + chrono::Duration::minutes(2 * i as i64),
            open: close,
            high: close,
            low: close,
            close,
            volume: 1_000.0,
        })
        .collect()
}

#[tokio::test]
async fn run_phase3_separates_survivors_and_technical_failures() {
    let rising: Vec<f64> = (1..=100).map(|i| i as f64).collect();
    let flat = vec![50.0; 100];

    let source = MockIntradayDataSource::new()
        .with_bars("RISER", session_bars_from_closes(&rising))
        .with_bars("FLAT", session_bars_from_closes(&flat));

    let universe = vec!["RISER".to_string(), "FLAT".to_string()];
    let config = Phase3Config::default();

    let results = run_phase3(&universe, &config, Arc::new(source), None).await;

    assert_eq!(results.survivors, vec!["RISER".to_string()]);
    assert_eq!(results.technical_failures, vec!["FLAT".to_string()]);
}

#[tokio::test]
async fn run_phase3_emits_final_progress_with_correct_totals() {
    let rising: Vec<f64> = (1..=100).map(|i| i as f64).collect();
    let flat = vec![50.0; 100];

    let source = MockIntradayDataSource::new()
        .with_bars("RISER", session_bars_from_closes(&rising))
        .with_bars("FLAT", session_bars_from_closes(&flat));

    let universe = vec!["RISER".to_string(), "FLAT".to_string()];
    let config = Phase3Config::default();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let results = run_phase3(&universe, &config, Arc::new(source), Some(tx)).await;

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
