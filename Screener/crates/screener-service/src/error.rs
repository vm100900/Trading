use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, Clone, thiserror::Error)]
pub enum ServiceError {
    #[error("a run is already in progress")]
    RunAlreadyInProgress,
    #[error("run not found")]
    RunNotFound,
    #[error("unauthorized")]
    Unauthorized,
}

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        let status = match self {
            ServiceError::RunAlreadyInProgress => StatusCode::CONFLICT,
            ServiceError::RunNotFound => StatusCode::NOT_FOUND,
            ServiceError::Unauthorized => StatusCode::UNAUTHORIZED,
        };
        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;
    use axum::http::StatusCode;

    #[tokio::test]
    async fn run_already_in_progress_maps_to_409() {
        let response = ServiceError::RunAlreadyInProgress.into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn run_not_found_maps_to_404() {
        let response = ServiceError::RunNotFound.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn unauthorized_maps_to_401() {
        let response = ServiceError::Unauthorized.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
