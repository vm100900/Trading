use std::sync::Arc;
use std::time::Duration;

use screener_service::config::{FilterConfigDto, InMemoryFilterConfigStore};
use screener_service::engine::{ProgressEvent, ScreeningEngine};
use screener_service::runs::InMemoryRunStore;
use screener_service::{build_router, AppState};
use tokio::sync::broadcast;

#[tokio::main]
async fn main() {
    let _ = tracing_subscriber::fmt::try_init();

    let auth_token = std::env::var("SCREENER_AUTH_TOKEN")
        .unwrap_or_else(|_| panic!("SCREENER_AUTH_TOKEN environment variable must be set"));

    // `RealScreeningEngine` needs a live IBKR connection (`IbkrClient::connect`)
    // and a universe list, neither of which this binary sources yet (deployment
    // config and universe management are separate future plans). Default is a
    // stub whose `POST /runs` fails fast; set `SCREENER_DEMO_ENGINE=1` for a
    // fake engine that streams a scripted Phase 1 -> 2 -> 3 -> Complete run, so
    // the API and the app's Live Run screen can be exercised end to end without
    // Yahoo/IBKR.
    let engine: Arc<dyn ScreeningEngine> = if std::env::var("SCREENER_DEMO_ENGINE").as_deref() == Ok("1")
    {
        tracing::warn!("SCREENER_DEMO_ENGINE=1 — using the scripted DemoScreeningEngine, not real screening");
        Arc::new(DemoScreeningEngine)
    } else {
        Arc::new(NotYetConfiguredEngine)
    };

    let state = AppState {
        auth_token,
        filter_config_store: Arc::new(InMemoryFilterConfigStore::new()),
        run_store: Arc::new(InMemoryRunStore::new()),
        engine,
        progress_channels: Default::default(),
    };

    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    tracing::info!("screener-service listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

struct NotYetConfiguredEngine;

#[async_trait::async_trait]
impl ScreeningEngine for NotYetConfiguredEngine {
    async fn run(
        &self,
        _config: FilterConfigDto,
        _progress_tx: broadcast::Sender<ProgressEvent>,
    ) -> Result<Vec<String>, String> {
        Err("no ScreeningEngine configured — wire RealScreeningEngine with a live IbkrClient before triggering runs".to_string())
    }
}

/// A fake engine that streams a scripted Phase 1 -> 2 -> 3 progression and
/// finishes with a small watchlist. Enabled by `SCREENER_DEMO_ENGINE=1`; used
/// only for local/manual end-to-end checks of the API and the app.
struct DemoScreeningEngine;

#[async_trait::async_trait]
impl ScreeningEngine for DemoScreeningEngine {
    async fn run(
        &self,
        _config: FilterConfigDto,
        progress_tx: broadcast::Sender<ProgressEvent>,
    ) -> Result<Vec<String>, String> {
        let tick = Duration::from_millis(600);

        // Phase 1: 500 -> 40 survivors
        for started in [100, 250, 400, 500] {
            let _ = progress_tx.send(ProgressEvent::Phase1 {
                total: 500,
                started,
                passed: started / 12,
                technical_failures: started - started / 12,
                errors: 0,
            });
            tokio::time::sleep(tick).await;
        }

        // Phase 2: 40 -> 12 survivors
        for started in [10, 25, 40] {
            let _ = progress_tx.send(ProgressEvent::Phase2 {
                total: 40,
                started,
                passed: (started as f64 * 0.3).round() as usize,
                technical_failures: started - (started as f64 * 0.3).round() as usize,
                errors: 0,
            });
            tokio::time::sleep(tick).await;
        }

        // Phase 3: 12 -> 4 survivors
        for started in [5, 12] {
            let _ = progress_tx.send(ProgressEvent::Phase3 {
                total: 12,
                started,
                passed: started / 3,
                technical_failures: started - started / 3,
                errors: 0,
            });
            tokio::time::sleep(tick).await;
        }

        Ok(vec![
            "AAPL".to_string(),
            "MSFT".to_string(),
            "NVDA".to_string(),
            "AMD".to_string(),
        ])
    }
}
