use async_trait::async_trait;
use screener_service::config::FilterConfigDto;
use screener_service::engine::{ProgressEvent, ScreeningEngine};
use tokio::sync::broadcast;

pub struct FakeScreeningEngine {
    pub final_watchlist: Vec<String>,
}

#[async_trait]
impl ScreeningEngine for FakeScreeningEngine {
    async fn run(&self, _config: FilterConfigDto, progress_tx: broadcast::Sender<ProgressEvent>) -> Result<Vec<String>, String> {
        let _ = progress_tx.send(ProgressEvent::Phase1 { total: 1, started: 1, passed: 1, technical_failures: 0, errors: 0 });
        let _ = progress_tx.send(ProgressEvent::Phase2 { total: 1, started: 1, passed: 1, technical_failures: 0, errors: 0 });
        let _ = progress_tx.send(ProgressEvent::Phase3 { total: 1, started: 1, passed: 1, technical_failures: 0, errors: 0 });
        Ok(self.final_watchlist.clone())
    }
}
