use std::sync::Arc;

use screener_service::config::InMemoryFilterConfigStore;
use screener_service::runs::InMemoryRunStore;
use screener_service::{build_router, AppState};

#[tokio::main]
async fn main() {
    let _ = tracing_subscriber::fmt::try_init();

    let auth_token = std::env::var("SCREENER_AUTH_TOKEN")
        .unwrap_or_else(|_| panic!("SCREENER_AUTH_TOKEN environment variable must be set"));

    // NOTE: this wires an in-memory FilterConfigStore/RunStore and has no
    // ScreeningEngine configured yet — RealScreeningEngine needs a live
    // IBKR connection (IbkrClient::connect) and a universe list, neither of
    // which this binary sources yet (deployment config and universe
    // management are separate future plans). This binary is runnable and
    // serves /health, /filter-config, and run history, but POST /runs will
    // need a real engine wired in before it does anything useful.
    let state = AppState {
        auth_token,
        filter_config_store: Arc::new(InMemoryFilterConfigStore::new()),
        run_store: Arc::new(InMemoryRunStore::new()),
        engine: Arc::new(NotYetConfiguredEngine),
        progress_channels: Default::default(),
    };

    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    tracing::info!("screener-service listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

struct NotYetConfiguredEngine;

#[async_trait::async_trait]
impl screener_service::engine::ScreeningEngine for NotYetConfiguredEngine {
    async fn run(
        &self,
        _config: screener_service::config::FilterConfigDto,
        _progress_tx: tokio::sync::broadcast::Sender<screener_service::engine::ProgressEvent>,
    ) -> Result<Vec<String>, String> {
        Err("no ScreeningEngine configured — wire RealScreeningEngine with a live IbkrClient before triggering runs".to_string())
    }
}
