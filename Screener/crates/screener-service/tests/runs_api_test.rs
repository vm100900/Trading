mod common;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::FakeScreeningEngine;
use http_body_util::BodyExt;
use screener_service::config::InMemoryFilterConfigStore;
use screener_service::runs::InMemoryRunStore;
use screener_service::{build_router, AppState};
use tower::ServiceExt;

fn app_with_fake_engine(final_watchlist: Vec<String>) -> axum::Router {
    let state = AppState {
        auth_token: "test-token".to_string(),
        filter_config_store: Arc::new(InMemoryFilterConfigStore::new()),
        run_store: Arc::new(InMemoryRunStore::new()),
        engine: Arc::new(FakeScreeningEngine { final_watchlist }),
        progress_channels: Default::default(),
    };
    build_router(state)
}

#[tokio::test]
async fn triggering_a_run_returns_a_running_record_and_it_completes_shortly_after() {
    let app = app_with_fake_engine(vec!["AAPL".to_string()]);

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
    assert_eq!(trigger_response.status(), StatusCode::OK);

    let body = trigger_response.into_body().collect().await.unwrap().to_bytes();
    let record: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id = record["id"].as_str().unwrap().to_string();
    assert_eq!(record["status"], "running");

    // Give the spawned run task a moment to finish (FakeScreeningEngine
    // resolves near-instantly).
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let get_response = app
        .oneshot(
            Request::get(format!("/runs/{run_id}"))
                .header("authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);
    let body = get_response.into_body().collect().await.unwrap().to_bytes();
    let record: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(record["status"], "completed");
    assert_eq!(record["final_watchlist"], serde_json::json!(["AAPL"]));
}

#[tokio::test]
async fn triggering_a_second_run_while_one_is_active_returns_409() {
    let app = app_with_fake_engine(vec![]);

    let first = app
        .clone()
        .oneshot(
            Request::post("/runs")
                .header("authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    let second = app
        .oneshot(
            Request::post("/runs")
                .header("authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn getting_an_unknown_run_returns_404() {
    let app = app_with_fake_engine(vec![]);
    let response = app
        .oneshot(
            Request::get("/runs/00000000-0000-0000-0000-000000000000")
                .header("authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
