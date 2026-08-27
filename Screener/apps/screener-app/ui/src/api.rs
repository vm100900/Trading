use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::dto::{FilterConfigDto, RunRecord};

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    Message(String),
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
}

#[derive(Debug, Clone)]
pub struct ApiClient {
    http: reqwest::Client,
    base_url: String,
}

impl ApiClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self { http: reqwest::Client::new(), base_url: base_url.into() }
    }

    async fn send(
        &self,
        method: reqwest::Method,
        path: &str,
        token: Option<&str>,
        body: Option<&(impl Serialize + ?Sized)>,
    ) -> Result<String, ApiError> {
        let mut req = self.http.request(method, format!("{}{}", self.base_url, path));
        if let Some(token) = token {
            req = req.bearer_auth(token);
        }
        if let Some(body) = body {
            req = req.json(body);
        }
        let response = req.send().await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            let message = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
                .unwrap_or_else(|| format!("HTTP {status}"));
            return Err(ApiError::Message(message));
        }
        Ok(text)
    }

    async fn request_json<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        token: Option<&str>,
        body: Option<&(impl Serialize + ?Sized)>,
    ) -> Result<T, ApiError> {
        let text = self.send(method, path, token, body).await?;
        serde_json::from_str(&text).map_err(|e| ApiError::Message(e.to_string()))
    }

    pub async fn get_filter_config(&self, token: &str) -> Result<FilterConfigDto, ApiError> {
        self.request_json(reqwest::Method::GET, "/filter-config", Some(token), None::<&()>).await
    }

    pub async fn put_filter_config(&self, token: &str, config: &FilterConfigDto) -> Result<FilterConfigDto, ApiError> {
        self.request_json(reqwest::Method::PUT, "/filter-config", Some(token), Some(config)).await
    }

    pub async fn trigger_run(&self, token: &str) -> Result<RunRecord, ApiError> {
        self.request_json(reqwest::Method::POST, "/runs", Some(token), None::<&()>).await
    }

    pub async fn get_run(&self, token: &str, run_id: &str) -> Result<RunRecord, ApiError> {
        self.request_json(reqwest::Method::GET, &format!("/runs/{run_id}"), Some(token), None::<&()>).await
    }

    pub async fn list_runs(&self, token: &str) -> Result<Vec<RunRecord>, ApiError> {
        self.request_json(reqwest::Method::GET, "/runs", Some(token), None::<&()>).await
    }

    pub async fn get_health(&self) -> Result<(), ApiError> {
        self.send(reqwest::Method::GET, "/health", None, None::<&()>).await.map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn get_filter_config_sends_bearer_token_and_returns_parsed_json() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/filter-config"))
            .and(header("authorization", "Bearer secret-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(FilterConfigDto::default()))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let config = client.get_filter_config("secret-token").await.unwrap();
        assert_eq!(config, FilterConfigDto::default());
    }

    #[tokio::test]
    async fn put_filter_config_sends_put_with_json_body() {
        let server = MockServer::start().await;
        let mut updated = FilterConfigDto::default();
        updated.phase3.enable_vwap = false;
        Mock::given(method("PUT"))
            .and(path("/filter-config"))
            .respond_with(ResponseTemplate::new(200).set_body_json(updated.clone()))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let result = client.put_filter_config("secret-token", &updated).await.unwrap();
        assert_eq!(result, updated);
    }

    #[tokio::test]
    async fn trigger_run_posts_to_runs_and_returns_the_record() {
        let server = MockServer::start().await;
        let record = RunRecord {
            id: "run-1".to_string(),
            status: crate::dto::RunStatus::Running,
            started_at: "2026-08-24T00:00:00Z".to_string(),
            finished_at: None,
            final_watchlist: vec![],
            error_message: None,
        };
        Mock::given(method("POST"))
            .and(path("/runs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&record))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let result = client.trigger_run("secret-token").await.unwrap();
        assert_eq!(result.id, "run-1");
    }

    #[tokio::test]
    async fn trigger_run_surfaces_the_server_error_message_on_409() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/runs"))
            .respond_with(
                ResponseTemplate::new(409)
                    .set_body_json(serde_json::json!({ "error": "a run is already in progress" })),
            )
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let err = client.trigger_run("secret-token").await.unwrap_err();
        assert_eq!(err.to_string(), "a run is already in progress");
    }

    #[tokio::test]
    async fn get_run_fetches_runs_by_id() {
        let server = MockServer::start().await;
        let record = RunRecord {
            id: "run-1".to_string(),
            status: crate::dto::RunStatus::Completed,
            started_at: "2026-08-24T00:00:00Z".to_string(),
            finished_at: Some("2026-08-24T00:05:00Z".to_string()),
            final_watchlist: vec!["AAPL".to_string()],
            error_message: None,
        };
        Mock::given(method("GET"))
            .and(path("/runs/run-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&record))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let result = client.get_run("secret-token", "run-1").await.unwrap();
        assert_eq!(result.status, crate::dto::RunStatus::Completed);
    }

    #[tokio::test]
    async fn list_runs_fetches_runs_and_returns_an_array() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/runs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let result = client.list_runs("secret-token").await.unwrap();
        assert_eq!(result.len(), 0);
    }

    #[tokio::test]
    async fn get_health_sends_no_authorization_header() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/health"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        client.get_health().await.unwrap();

        let requests = server.received_requests().await.unwrap();
        assert!(requests[0].headers.get("authorization").is_none());
    }
}
