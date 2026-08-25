use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::engine::ProgressEvent;
use crate::error::ServiceError;
use crate::runs::{RunRecord, RunStatus};
use crate::AppState;

pub async fn trigger_run(State(state): State<AppState>) -> Result<Json<RunRecord>, ServiceError> {
    let record = state
        .run_store
        .try_start_run()
        .await
        .ok_or(ServiceError::RunAlreadyInProgress)?;

    let (tx, _rx) = broadcast::channel(64);
    state.progress_channels.write().await.insert(record.id, tx.clone());

    let run_id = record.id;
    let engine = Arc::clone(&state.engine);
    let run_store = Arc::clone(&state.run_store);
    let config = state.filter_config_store.get().await;

    tokio::spawn(async move {
        // RunStore is updated BEFORE broadcasting: a WebSocket client that
        // subscribes late (after the run already finished) checks RunStore
        // to synthesize the terminal event, since a broadcast sent with no
        // subscribers is lost forever. Updating the store first guarantees
        // that if a client ever observes "Running" here, the terminal
        // broadcast hasn't been sent yet — so its subscription (taken
        // before this check, in `run_stream`) is guaranteed to catch it.
        match engine.run(config, tx.clone()).await {
            Ok(final_watchlist) => {
                run_store.complete_run(run_id, final_watchlist.clone()).await;
                let _ = tx.send(ProgressEvent::Complete { final_watchlist });
            }
            Err(message) => {
                run_store.fail_run(run_id, message.clone()).await;
                let _ = tx.send(ProgressEvent::Failed { message });
            }
        }
    });

    Ok(Json(record))
}

pub async fn get_run(State(state): State<AppState>, Path(id): Path<Uuid>) -> Result<Json<RunRecord>, ServiceError> {
    state.run_store.get_run(id).await.map(Json).ok_or(ServiceError::RunNotFound)
}

pub async fn list_runs(State(state): State<AppState>) -> Json<Vec<RunRecord>> {
    Json(state.run_store.list_runs().await)
}

pub type ProgressChannels = Arc<RwLock<HashMap<Uuid, broadcast::Sender<ProgressEvent>>>>;

#[derive(serde::Deserialize)]
pub struct StreamAuthQuery {
    token: Option<String>,
}

pub async fn run_stream(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<StreamAuthQuery>,
    headers: axum::http::HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let header_token = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));

    let authorized = header_token == Some(state.auth_token.as_str())
        || query.token.as_deref() == Some(state.auth_token.as_str());

    if !authorized {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let tx = {
        let channels = state.progress_channels.read().await;
        channels.get(&id).cloned()
    };

    let Some(tx) = tx else {
        return (StatusCode::NOT_FOUND, "run not found").into_response();
    };

    // Subscribe BEFORE checking RunStore: this guarantees that if the check
    // below observes RunStatus::Running, the terminal broadcast has not
    // been sent yet (trigger_run updates the store first), so this
    // subscription is guaranteed to receive it when it eventually happens.
    let rx = tx.subscribe();
    let existing_record = state.run_store.get_run(id).await;

    ws.on_upgrade(move |socket| handle_socket(socket, rx, existing_record)).into_response()
}

async fn handle_socket(mut socket: WebSocket, mut rx: broadcast::Receiver<ProgressEvent>, existing_record: Option<RunRecord>) {
    // If the run already finished before we subscribed, its terminal event
    // was broadcast to zero receivers and is lost to `rx` forever — recover
    // it from the authoritative RunStore instead of waiting indefinitely.
    if let Some(record) = existing_record {
        let synthetic = match record.status {
            RunStatus::Completed => Some(ProgressEvent::Complete { final_watchlist: record.final_watchlist }),
            RunStatus::Failed => Some(ProgressEvent::Failed {
                message: record.error_message.unwrap_or_default(),
            }),
            RunStatus::Running => None,
        };
        if let Some(event) = synthetic {
            if let Ok(json) = serde_json::to_string(&event) {
                let _ = socket.send(Message::Text(json.into())).await;
            }
            return;
        }
    }

    while let Ok(event) = rx.recv().await {
        let is_terminal = matches!(event, ProgressEvent::Complete { .. } | ProgressEvent::Failed { .. });
        let Ok(json) = serde_json::to_string(&event) else {
            break;
        };
        if socket.send(Message::Text(json.into())).await.is_err() {
            break;
        }
        if is_terminal {
            break;
        }
    }
}
