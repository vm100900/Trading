use std::sync::Arc;

use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinSet;

use crate::config::{Phase1Config, Phase2Config, Phase3Config};
use crate::data::{DailyDataSource, IntradayDataSource};
use crate::error::ScreeningError;
use crate::phase1::{evaluate_phase1, Phase1Outcome};
use crate::phase2::{evaluate_phase2, Phase2Outcome};
use crate::phase3::{evaluate_phase3, Phase3Outcome};

#[derive(Debug, Clone, PartialEq)]
pub struct Phase1Progress {
    pub total: usize,
    pub started: usize,
    pub passed: usize,
    pub technical_failures: usize,
    pub errors: usize,
}

#[derive(Debug, Default)]
pub struct Phase1Results {
    pub survivors: Vec<String>,
    pub technical_failures: Vec<String>,
    pub errors: Vec<(String, ScreeningError)>,
}

enum SymbolOutcome {
    Passed(String),
    FailedTechnical(String),
    Errored(String, ScreeningError),
}

pub async fn run_phase1(
    universe: &[String],
    config: &Phase1Config,
    data_source: Arc<dyn DailyDataSource>,
    progress_tx: Option<UnboundedSender<Phase1Progress>>,
) -> Phase1Results {
    let total = universe.len();
    let min_bars = config.min_bars_required();

    let mut tasks = JoinSet::new();
    for symbol in universe {
        let symbol = symbol.clone();
        let config = config.clone();
        let data_source = Arc::clone(&data_source);
        tasks.spawn(async move {
            match data_source.fetch_daily_bars(&symbol, min_bars).await {
                Ok(bars) => match evaluate_phase1(&bars, &config) {
                    Phase1Outcome::Passed => SymbolOutcome::Passed(symbol),
                    Phase1Outcome::FailedTechnical => SymbolOutcome::FailedTechnical(symbol),
                    Phase1Outcome::InsufficientData { needed, got } => SymbolOutcome::Errored(
                        symbol.clone(),
                        ScreeningError::InsufficientData { symbol, needed, got },
                    ),
                },
                Err(err) => SymbolOutcome::Errored(symbol, err),
            }
        });
    }

    let mut results = Phase1Results::default();
    let mut started = 0;

    while let Some(joined) = tasks.join_next().await {
        started += 1;
        match joined.expect("phase 1 worker task panicked") {
            SymbolOutcome::Passed(symbol) => results.survivors.push(symbol),
            SymbolOutcome::FailedTechnical(symbol) => results.technical_failures.push(symbol),
            SymbolOutcome::Errored(symbol, err) => results.errors.push((symbol, err)),
        }

        if let Some(tx) = &progress_tx {
            let _ = tx.send(Phase1Progress {
                total,
                started,
                passed: results.survivors.len(),
                technical_failures: results.technical_failures.len(),
                errors: results.errors.len(),
            });
        }
    }

    results
}

#[derive(Debug, Clone, PartialEq)]
pub struct Phase2Progress {
    pub total: usize,
    pub started: usize,
    pub passed: usize,
    pub technical_failures: usize,
    pub errors: usize,
}

#[derive(Debug, Default)]
pub struct Phase2Results {
    pub survivors: Vec<String>,
    pub technical_failures: Vec<String>,
    pub errors: Vec<(String, ScreeningError)>,
}

enum Phase2SymbolOutcome {
    Passed(String),
    FailedTechnical(String),
    Errored(String, ScreeningError),
}

pub async fn run_phase2(
    universe: &[String],
    config: &Phase2Config,
    data_source: Arc<dyn IntradayDataSource>,
    progress_tx: Option<UnboundedSender<Phase2Progress>>,
) -> Phase2Results {
    let total = universe.len();
    let min_bars = config.min_bars_required();

    let mut tasks = JoinSet::new();
    for symbol in universe {
        let symbol = symbol.clone();
        let config = config.clone();
        let data_source = Arc::clone(&data_source);
        tasks.spawn(async move {
            match data_source
                .fetch_intraday_bars(&symbol, crate::models::IntradayBarSize::ThirtyMinutes, min_bars)
                .await
            {
                Ok(bars) => match evaluate_phase2(&bars, &config) {
                    Phase2Outcome::Passed => Phase2SymbolOutcome::Passed(symbol),
                    Phase2Outcome::FailedTechnical => Phase2SymbolOutcome::FailedTechnical(symbol),
                    Phase2Outcome::InsufficientData { needed, got } => Phase2SymbolOutcome::Errored(
                        symbol.clone(),
                        ScreeningError::InsufficientData { symbol, needed, got },
                    ),
                },
                Err(err) => Phase2SymbolOutcome::Errored(symbol, err),
            }
        });
    }

    let mut results = Phase2Results::default();
    let mut started = 0;

    while let Some(joined) = tasks.join_next().await {
        started += 1;
        match joined.expect("phase 2 worker task panicked") {
            Phase2SymbolOutcome::Passed(symbol) => results.survivors.push(symbol),
            Phase2SymbolOutcome::FailedTechnical(symbol) => results.technical_failures.push(symbol),
            Phase2SymbolOutcome::Errored(symbol, err) => results.errors.push((symbol, err)),
        }

        if let Some(tx) = &progress_tx {
            let _ = tx.send(Phase2Progress {
                total,
                started,
                passed: results.survivors.len(),
                technical_failures: results.technical_failures.len(),
                errors: results.errors.len(),
            });
        }
    }

    results
}

#[derive(Debug, Clone, PartialEq)]
pub struct Phase3Progress {
    pub total: usize,
    pub started: usize,
    pub passed: usize,
    pub technical_failures: usize,
    pub errors: usize,
}

#[derive(Debug, Default)]
pub struct Phase3Results {
    pub survivors: Vec<String>,
    pub technical_failures: Vec<String>,
    pub errors: Vec<(String, ScreeningError)>,
}

enum Phase3SymbolOutcome {
    Passed(String),
    FailedTechnical(String),
    Errored(String, ScreeningError),
}

pub async fn run_phase3(
    universe: &[String],
    config: &Phase3Config,
    data_source: Arc<dyn IntradayDataSource>,
    progress_tx: Option<UnboundedSender<Phase3Progress>>,
) -> Phase3Results {
    let total = universe.len();
    let min_bars = config.min_bars_required();

    let mut tasks = JoinSet::new();
    for symbol in universe {
        let symbol = symbol.clone();
        let config = config.clone();
        let data_source = Arc::clone(&data_source);
        tasks.spawn(async move {
            match data_source
                .fetch_intraday_bars(&symbol, crate::models::IntradayBarSize::TwoMinutes, min_bars)
                .await
            {
                Ok(bars) => match evaluate_phase3(&bars, &config) {
                    Phase3Outcome::Passed => Phase3SymbolOutcome::Passed(symbol),
                    Phase3Outcome::FailedTechnical => Phase3SymbolOutcome::FailedTechnical(symbol),
                    Phase3Outcome::InsufficientData { needed, got } => Phase3SymbolOutcome::Errored(
                        symbol.clone(),
                        ScreeningError::InsufficientData { symbol, needed, got },
                    ),
                    Phase3Outcome::NoVwapSignal => Phase3SymbolOutcome::Errored(
                        symbol.clone(),
                        ScreeningError::CalculationError(format!(
                            "no VWAP signal for {symbol}: zero or invalid session volume"
                        )),
                    ),
                },
                Err(err) => Phase3SymbolOutcome::Errored(symbol, err),
            }
        });
    }

    let mut results = Phase3Results::default();
    let mut started = 0;

    while let Some(joined) = tasks.join_next().await {
        started += 1;
        match joined.expect("phase 3 worker task panicked") {
            Phase3SymbolOutcome::Passed(symbol) => results.survivors.push(symbol),
            Phase3SymbolOutcome::FailedTechnical(symbol) => results.technical_failures.push(symbol),
            Phase3SymbolOutcome::Errored(symbol, err) => results.errors.push((symbol, err)),
        }

        if let Some(tx) = &progress_tx {
            let _ = tx.send(Phase3Progress {
                total,
                started,
                passed: results.survivors.len(),
                technical_failures: results.technical_failures.len(),
                errors: results.errors.len(),
            });
        }
    }

    results
}
