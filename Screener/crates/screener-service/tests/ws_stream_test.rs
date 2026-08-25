mod common;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::FakeScreeningEngine;
use futures_util::StreamExt;
use http_body_util::BodyExt;
use screener_service::config::InMemoryFilterConfigStore;
use screener_service::runs::InMemoryRunStore;
use screener_service::{build_router, AppState};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tower::ServiceExt;

fn authorized_ws_request(url: &str) -> tokio_tungstenite::tungstenite::handshake::client::Request {
    let mut request = url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert("authorization", "Bearer test-token".parse().unwrap());
    request
}

#[tokio::test]
async fn streams_progress_events_and_closes_after_completion() {
    let state = AppState {
        auth_token: "test-token".to_string(),
        filter_config_store: Arc::new(InMemoryFilterConfigStore::new()),
        run_store: Arc::new(InMemoryRunStore::new()),
        engine: Arc::new(FakeScreeningEngine { final_watchlist: vec!["AAPL".to_string()] }),
        progress_channels: Default::default(),
    };
    let app = build_router(state);

    // Trigger a run over the in-process router first, to get a real run id
    // and populate the progress-channel registry.
    let trigger_response = app
        .clone()
        .oneshot(
            Request::post("/runs")
                .header("authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = trigger_response.into_body().collect().await.unwrap().to_bytes();
    let record: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id = record["id"].as_str().unwrap().to_string();

    // Serve the same router on a real ephemeral TCP port so we can connect
    // an actual WebSocket client to it.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let url = format!("ws://{addr}/runs/{run_id}/stream");
    let (mut ws_stream, response) = connect_async(authorized_ws_request(&url)).await.unwrap();
    assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);

    let mut saw_complete = false;
    while let Some(Ok(msg)) = ws_stream.next().await {
        if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
            let event: serde_json::Value = serde_json::from_str(&text).unwrap();
            if event["phase"] == "Complete" {
                assert_eq!(event["final_watchlist"], serde_json::json!(["AAPL"]));
                saw_complete = true;
                break;
            }
        }
    }
    assert!(saw_complete, "expected a Complete event before the stream ended");

    let _ = ws_stream.close(None).await;
}

#[tokio::test]
async fn streaming_an_unknown_run_id_returns_404() {
    let state = AppState {
        auth_token: "test-token".to_string(),
        filter_config_store: Arc::new(InMemoryFilterConfigStore::new()),
        run_store: Arc::new(InMemoryRunStore::new()),
        engine: Arc::new(FakeScreeningEngine { final_watchlist: vec![] }),
        progress_channels: Default::default(),
    };
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let url = format!("ws://{addr}/runs/00000000-0000-0000-0000-000000000000/stream");
    let result = connect_async(authorized_ws_request(&url)).await;
    assert!(result.is_err(), "connecting to an unknown run id should fail the WS handshake");
}

#[tokio::test]
async fn streams_progress_when_token_is_supplied_as_query_param_instead_of_header() {
    let state = AppState {
        auth_token: "test-token".to_string(),
        filter_config_store: Arc::new(InMemoryFilterConfigStore::new()),
        run_store: Arc::new(InMemoryRunStore::new()),
        engine: Arc::new(FakeScreeningEngine { final_watchlist: vec!["AAPL".to_string()] }),
        progress_channels: Default::default(),
    };
    let app = build_router(state);

    let trigger_response = app
        .clone()
        .oneshot(
            Request::post("/runs")
                .header("authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = trigger_response.into_body().collect().await.unwrap().to_bytes();
    let record: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id = record["id"].as_str().unwrap().to_string();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // No Authorization header at all — token only in the query string, the
    // way a browser/webview WebSocket client is forced to send it.
    let url = format!("ws://{addr}/runs/{run_id}/stream?token=test-token");
    let (mut ws_stream, response) = connect_async(url).await.unwrap();
    assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);

    let mut saw_complete = false;
    while let Some(Ok(msg)) = ws_stream.next().await {
        if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
            let event: serde_json::Value = serde_json::from_str(&text).unwrap();
            if event["phase"] == "Complete" {
                saw_complete = true;
                break;
            }
        }
    }
    assert!(saw_complete);
    let _ = ws_stream.close(None).await;
}

#[tokio::test]
async fn rejects_ws_connection_with_no_token_anywhere() {
    let state = AppState {
        auth_token: "test-token".to_string(),
        filter_config_store: Arc::new(InMemoryFilterConfigStore::new()),
        run_store: Arc::new(InMemoryRunStore::new()),
        engine: Arc::new(FakeScreeningEngine { final_watchlist: vec![] }),
        progress_channels: Default::default(),
    };
    let app = build_router(state);

    let trigger_response = app
        .clone()
        .oneshot(
            Request::post("/runs")
                .header("authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = trigger_response.into_body().collect().await.unwrap().to_bytes();
    let record: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id = record["id"].as_str().unwrap().to_string();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // No header, no query param.
    let url = format!("ws://{addr}/runs/{run_id}/stream");
    let result = connect_async(url).await;
    assert!(result.is_err());
}
