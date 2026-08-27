use std::sync::Arc;
use std::time::Duration;

use screener_core::data::ibkr::IbkrClient;
use screener_core::data::yahoo::{YahooClient, YahooClientConfig};
use screener_service::config::{FilterConfigDto, InMemoryFilterConfigStore};
use screener_service::engine::{ProgressEvent, RealScreeningEngine, ScreeningEngine};
use screener_service::runs::InMemoryRunStore;
use screener_service::{build_router, AppState};
use tokio::sync::broadcast;

#[tokio::main]
async fn main() {
    let _ = tracing_subscriber::fmt::try_init();

    let auth_token = std::env::var("SCREENER_AUTH_TOKEN")
        .unwrap_or_else(|_| panic!("SCREENER_AUTH_TOKEN environment variable must be set"));
    let bind = std::env::var("SCREENER_BIND").unwrap_or_else(|_| "0.0.0.0:8080".to_string());

    let engine = build_engine().await;

    let state = AppState {
        auth_token,
        filter_config_store: Arc::new(InMemoryFilterConfigStore::new()),
        run_store: Arc::new(InMemoryRunStore::new()),
        engine,
        progress_channels: Default::default(),
    };

    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(&bind).await.unwrap();
    tracing::info!("screener-service listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

/// Engine selection, in priority order:
///
/// * `SCREENER_DEMO_ENGINE=1` — a scripted fake run (no Yahoo/IBKR); for local
///   and CI end-to-end checks of the API and the app.
/// * `SCREENER_IBKR_ADDR` set — the real Phase 1->2->3 pipeline. Reads:
///     - `SCREENER_IBKR_ADDR`      e.g. `127.0.0.1:4002` (paper) / `:4001` (live)
///     - `SCREENER_IBKR_CLIENT_ID` integer, default `11`
///     - `SCREENER_UNIVERSE_FILE`  path to a newline-separated symbol list
///   The IBKR connection is opened fresh for each run (see
///   `ReconnectingRealEngine`) so IB Gateway's nightly restart can't leave the
///   service holding a dead socket.
/// * otherwise — a stub whose `POST /runs` fails fast.
async fn build_engine() -> Arc<dyn ScreeningEngine> {
    if std::env::var("SCREENER_DEMO_ENGINE").as_deref() == Ok("1") {
        tracing::warn!("SCREENER_DEMO_ENGINE=1 — scripted DemoScreeningEngine, not real screening");
        return Arc::new(DemoScreeningEngine);
    }

    let Ok(ibkr_addr) = std::env::var("SCREENER_IBKR_ADDR") else {
        tracing::warn!(
            "no SCREENER_IBKR_ADDR set — POST /runs will fail until a real engine is configured"
        );
        return Arc::new(NotYetConfiguredEngine);
    };

    let client_id: i32 = std::env::var("SCREENER_IBKR_CLIENT_ID")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(11);

    let universe_file = std::env::var("SCREENER_UNIVERSE_FILE")
        .expect("SCREENER_UNIVERSE_FILE must be set when SCREENER_IBKR_ADDR is set");
    let universe = load_universe(&universe_file);
    tracing::info!(
        "real engine: IBKR {ibkr_addr} (client_id {client_id}), {} symbols from {universe_file}",
        universe.len()
    );

    // Fail fast on obvious misconfiguration: try one connection at startup so a
    // bad address / down Gateway surfaces in the logs immediately rather than
    // only on the first run. The connection is dropped again right away — each
    // run opens its own.
    match IbkrClient::connect(&ibkr_addr, client_id).await {
        Ok(_) => tracing::info!("IBKR reachable at {ibkr_addr}"),
        Err(e) => tracing::error!("IBKR not reachable at {ibkr_addr} yet: {e} (runs will retry)"),
    }

    Arc::new(ReconnectingRealEngine {
        yahoo: Arc::new(YahooClient::new(YahooClientConfig::default())),
        ibkr_addr,
        client_id,
        universe,
    })
}

fn load_universe(path: &str) -> Vec<String> {
    let contents = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("could not read SCREENER_UNIVERSE_FILE {path}: {e}"));
    let symbols: Vec<String> = contents
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim().to_uppercase())
        .filter(|l| !l.is_empty())
        .collect();
    if symbols.is_empty() {
        panic!("SCREENER_UNIVERSE_FILE {path} contained no symbols");
    }
    symbols
}

/// Wraps `RealScreeningEngine`, opening a fresh `IbkrClient` connection for
/// every run. One connection per screening run (not per symbol) still satisfies
/// screener-core's "one persistent connection per run" rule, and it means IB
/// Gateway's forced daily restart never leaves this process wedged.
struct ReconnectingRealEngine {
    yahoo: Arc<YahooClient>,
    ibkr_addr: String,
    client_id: i32,
    universe: Vec<String>,
}

#[async_trait::async_trait]
impl ScreeningEngine for ReconnectingRealEngine {
    async fn run(
        &self,
        config: FilterConfigDto,
        progress_tx: broadcast::Sender<ProgressEvent>,
    ) -> Result<Vec<String>, String> {
        let ibkr = IbkrClient::connect(&self.ibkr_addr, self.client_id)
            .await
            .map_err(|e| format!("IBKR connect ({}) failed: {e}", self.ibkr_addr))?;
        let engine = RealScreeningEngine::new(
            Arc::clone(&self.yahoo),
            Arc::new(ibkr),
            self.universe.clone(),
        );
        engine.run(config, progress_tx).await
    }
}

struct NotYetConfiguredEngine;

#[async_trait::async_trait]
impl ScreeningEngine for NotYetConfiguredEngine {
    async fn run(
        &self,
        _config: FilterConfigDto,
        _progress_tx: broadcast::Sender<ProgressEvent>,
    ) -> Result<Vec<String>, String> {
        Err("no ScreeningEngine configured — set SCREENER_IBKR_ADDR (+ SCREENER_UNIVERSE_FILE) or SCREENER_DEMO_ENGINE=1".to_string())
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
