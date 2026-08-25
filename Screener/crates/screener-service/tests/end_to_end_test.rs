mod common;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::FakeScreeningEngine;
use http_body_util::BodyExt;
use screener_service::config::{FilterConfigDto, InMemoryFilterConfigStore};
use screener_service::runs::InMemoryRunStore;
use screener_service::{build_router, AppState};
use tower::ServiceExt;

#[tokio::test]
async fn full_lifecycle_config_update_then_run_then_history() {
    let state = AppState {
        auth_token: "test-token".to_string(),
        filter_config_store: Arc::new(InMemoryFilterConfigStore::new()),
        run_store: Arc::new(InMemoryRunStore::new()),
        engine: Arc::new(FakeScreeningEngine { final_watchlist: vec!["NVDA".to_string()] }),
        progress_channels: Default::default(),
    };
    let app = build_router(state);
    let auth = "Bearer test-token";

    // 1. Update filter config.
    let mut config = FilterConfigDto::default();
    config.phase3.enable_vwap = false;
    let put_response = app
        .clone()
        .oneshot(
            Request::put("/filter-config")
                .header("authorization", auth)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&config).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put_response.status(), StatusCode::OK);

    // 2. Confirm it persisted.
    let get_config_response = app
        .clone()
        .oneshot(
            Request::get("/filter-config")
                .header("authorization", auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = get_config_response.into_body().collect().await.unwrap().to_bytes();
    let stored: FilterConfigDto = serde_json::from_slice(&body).unwrap();
    assert!(!stored.phase3.enable_vwap);

    // 3. Trigger a run.
    let trigger_response = app
        .clone()
        .oneshot(
            Request::post("/runs")
                .header("authorization", auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(trigger_response.status(), StatusCode::OK);
    let body = trigger_response.into_body().collect().await.unwrap().to_bytes();
    let record: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id = record["id"].as_str().unwrap().to_string();

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // 4. Confirm it shows up completed in history.
    let list_response = app
        .oneshot(
            Request::get("/runs")
                .header("authorization", auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = list_response.into_body().collect().await.unwrap().to_bytes();
    let runs: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let runs = runs.as_array().unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["id"], run_id);
    assert_eq!(runs[0]["status"], "completed");
    assert_eq!(runs[0]["final_watchlist"], serde_json::json!(["NVDA"]));
}
