use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::broadcast;

use screener_core::data::ibkr::IbkrClient;
use screener_core::data::yahoo::YahooClient;
use screener_core::{run_phase1, run_phase2, run_phase3};

use crate::config::FilterConfigDto;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "phase")]
pub enum ProgressEvent {
    Phase1 { total: usize, started: usize, passed: usize, technical_failures: usize, errors: usize },
    Phase2 { total: usize, started: usize, passed: usize, technical_failures: usize, errors: usize },
    Phase3 { total: usize, started: usize, passed: usize, technical_failures: usize, errors: usize },
    Complete { final_watchlist: Vec<String> },
    Failed { message: String },
}

#[async_trait]
pub trait ScreeningEngine: Send + Sync {
    /// Runs the full Phase 1 -> 2 -> 3 pipeline, emitting progress on
    /// `progress_tx` as it goes, and returns the final watchlist on
    /// success or an error message on failure.
    async fn run(&self, config: FilterConfigDto, progress_tx: broadcast::Sender<ProgressEvent>) -> Result<Vec<String>, String>;
}

pub struct RealScreeningEngine {
    yahoo: Arc<YahooClient>,
    ibkr: Arc<IbkrClient>,
    universe: Vec<String>,
}

impl RealScreeningEngine {
    /// `universe` is a static symbol list for now — real universe
    /// management (S&P 500 / Nasdaq scraping, refreshable via the API) is
    /// a separate future plan.
    pub fn new(yahoo: Arc<YahooClient>, ibkr: Arc<IbkrClient>, universe: Vec<String>) -> Self {
        Self { yahoo, ibkr, universe }
    }

    async fn relay_phase1(rx: &mut tokio::sync::mpsc::UnboundedReceiver<screener_core::Phase1Progress>, tx: &broadcast::Sender<ProgressEvent>) {
        while let Some(p) = rx.recv().await {
            let _ = tx.send(ProgressEvent::Phase1 {
                total: p.total,
                started: p.started,
                passed: p.passed,
                technical_failures: p.technical_failures,
                errors: p.errors,
            });
        }
    }

    async fn relay_phase2(rx: &mut tokio::sync::mpsc::UnboundedReceiver<screener_core::Phase2Progress>, tx: &broadcast::Sender<ProgressEvent>) {
        while let Some(p) = rx.recv().await {
            let _ = tx.send(ProgressEvent::Phase2 {
                total: p.total,
                started: p.started,
                passed: p.passed,
                technical_failures: p.technical_failures,
                errors: p.errors,
            });
        }
    }

    async fn relay_phase3(rx: &mut tokio::sync::mpsc::UnboundedReceiver<screener_core::Phase3Progress>, tx: &broadcast::Sender<ProgressEvent>) {
        while let Some(p) = rx.recv().await {
            let _ = tx.send(ProgressEvent::Phase3 {
                total: p.total,
                started: p.started,
                passed: p.passed,
                technical_failures: p.technical_failures,
                errors: p.errors,
            });
        }
    }
}

#[async_trait]
impl ScreeningEngine for RealScreeningEngine {
    async fn run(&self, config: FilterConfigDto, progress_tx: broadcast::Sender<ProgressEvent>) -> Result<Vec<String>, String> {
        let phase1_config: screener_core::Phase1Config = config.phase1.into();
        let phase2_config: screener_core::Phase2Config = config.phase2.into();
        let phase3_config: screener_core::Phase3Config = config.phase3.into();

        let (p1_tx, mut p1_rx) = tokio::sync::mpsc::unbounded_channel();
        let relay_tx = progress_tx.clone();
        let relay1 = tokio::spawn(async move { Self::relay_phase1(&mut p1_rx, &relay_tx).await });
        let phase1_results = run_phase1(&self.universe, &phase1_config, Arc::clone(&self.yahoo) as _, Some(p1_tx)).await;
        let _ = relay1.await;

        if phase1_results.survivors.is_empty() {
            return Ok(Vec::new());
        }

        let (p2_tx, mut p2_rx) = tokio::sync::mpsc::unbounded_channel();
        let relay_tx = progress_tx.clone();
        let relay2 = tokio::spawn(async move { Self::relay_phase2(&mut p2_rx, &relay_tx).await });
        let phase2_results = run_phase2(&phase1_results.survivors, &phase2_config, Arc::clone(&self.ibkr) as _, Some(p2_tx)).await;
        let _ = relay2.await;

        if phase2_results.survivors.is_empty() {
            return Ok(Vec::new());
        }

        let (p3_tx, mut p3_rx) = tokio::sync::mpsc::unbounded_channel();
        let relay_tx = progress_tx.clone();
        let relay3 = tokio::spawn(async move { Self::relay_phase3(&mut p3_rx, &relay_tx).await });
        let phase3_results = run_phase3(&phase2_results.survivors, &phase3_config, Arc::clone(&self.ibkr) as _, Some(p3_tx)).await;
        let _ = relay3.await;

        Ok(phase3_results.survivors)
    }
}
