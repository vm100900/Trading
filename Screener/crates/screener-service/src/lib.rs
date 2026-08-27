pub mod auth;
pub mod config;
pub mod engine;
pub mod error;
pub mod routes;
pub mod runs;

use axum::http::StatusCode;
use axum::routing::get;
use axum::Router;

#[derive(Clone)]
pub struct AppState {
    pub auth_token: String,
    pub filter_config_store: std::sync::Arc<dyn crate::config::FilterConfigStore>,
    pub run_store: std::sync::Arc<dyn crate::runs::RunStore>,
    pub engine: std::sync::Arc<dyn crate::engine::ScreeningEngine>,
    pub progress_channels: crate::routes::ProgressChannels,
}

async fn health() -> StatusCode {
    StatusCode::OK
}

pub fn build_router(state: AppState) -> Router {
    use axum::middleware;
    use axum::routing::{get as get_route, post};
    use tower_http::cors::CorsLayer;

    let protected = Router::new()
        .route(
            "/filter-config",
            get_route(config::get_filter_config).put(config::put_filter_config),
        )
        .route("/runs", post(routes::trigger_run).get(routes::list_runs))
        .route("/runs/{id}", get_route(routes::get_run))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth::require_bearer_token));

    Router::new()
        .route("/health", get(health))
        .route("/runs/{id}/stream", get_route(routes::run_stream))
        .merge(protected)
        .with_state(state)
        // The app's frontend is WASM (`reqwest` -> browser fetch) served from
        // a `tauri://`/`http://…localhost` origin, so every REST call is
        // cross-origin and subject to CORS. Auth is by bearer token (a
        // header, not a cookie), so a wildcard policy is safe here and keeps
        // the webview origin — which isn't stable — from mattering.
        .layer(CorsLayer::permissive())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    struct NoOpEngine;

    #[async_trait::async_trait]
    impl crate::engine::ScreeningEngine for NoOpEngine {
        async fn run(
            &self,
            _config: crate::config::FilterConfigDto,
            _progress_tx: tokio::sync::broadcast::Sender<crate::engine::ProgressEvent>,
        ) -> Result<Vec<String>, String> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn health_route_returns_200() {
        let state = AppState {
            auth_token: "test-token".to_string(),
            filter_config_store: std::sync::Arc::new(crate::config::InMemoryFilterConfigStore::new()),
            run_store: std::sync::Arc::new(crate::runs::InMemoryRunStore::new()),
            engine: std::sync::Arc::new(NoOpEngine),
            progress_channels: Default::default(),
        };
        let app = build_router(state);

        let response = app
            .oneshot(Request::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
