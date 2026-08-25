use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;

use crate::AppState;

pub async fn require_bearer_token(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let provided = request
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));

    match provided {
        Some(token) if token == state.auth_token => Ok(next.run(request).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::middleware;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    use crate::auth::require_bearer_token;
    use crate::AppState;

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

    fn protected_app() -> Router {
        let state = AppState {
            auth_token: "correct-token".to_string(),
            filter_config_store: std::sync::Arc::new(crate::config::InMemoryFilterConfigStore::new()),
            run_store: std::sync::Arc::new(crate::runs::InMemoryRunStore::new()),
            engine: std::sync::Arc::new(NoOpEngine),
            progress_channels: Default::default(),
        };
        Router::new()
            .route("/protected", get(|| async { "ok" }))
            .route_layer(middleware::from_fn_with_state(state.clone(), require_bearer_token))
            .with_state(state)
    }

    #[tokio::test]
    async fn missing_authorization_header_is_rejected() {
        let response = protected_app()
            .oneshot(Request::get("/protected").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn wrong_token_is_rejected() {
        let response = protected_app()
            .oneshot(
                Request::get("/protected")
                    .header("authorization", "Bearer wrong-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn correct_token_is_accepted() {
        let response = protected_app()
            .oneshot(
                Request::get("/protected")
                    .header("authorization", "Bearer correct-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
