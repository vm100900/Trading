use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunRecord {
    pub id: Uuid,
    pub status: RunStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub final_watchlist: Vec<String>,
    pub error_message: Option<String>,
}

#[async_trait]
pub trait RunStore: Send + Sync {
    /// Atomically starts a new run iff none is currently active. Returns
    /// `None` without creating anything if a run is already running.
    async fn try_start_run(&self) -> Option<RunRecord>;
    async fn complete_run(&self, id: Uuid, final_watchlist: Vec<String>);
    async fn fail_run(&self, id: Uuid, message: String);
    async fn get_run(&self, id: Uuid) -> Option<RunRecord>;
    async fn list_runs(&self) -> Vec<RunRecord>;
}

#[derive(Default)]
pub struct InMemoryRunStore {
    runs: RwLock<HashMap<Uuid, RunRecord>>,
    active: RwLock<bool>,
}

impl InMemoryRunStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl RunStore for InMemoryRunStore {
    async fn try_start_run(&self) -> Option<RunRecord> {
        let mut active = self.active.write().await;
        if *active {
            return None;
        }
        *active = true;

        let record = RunRecord {
            id: Uuid::new_v4(),
            status: RunStatus::Running,
            started_at: Utc::now(),
            finished_at: None,
            final_watchlist: Vec::new(),
            error_message: None,
        };
        self.runs.write().await.insert(record.id, record.clone());
        Some(record)
    }

    async fn complete_run(&self, id: Uuid, final_watchlist: Vec<String>) {
        if let Some(record) = self.runs.write().await.get_mut(&id) {
            record.status = RunStatus::Completed;
            record.finished_at = Some(Utc::now());
            record.final_watchlist = final_watchlist;
        }
        *self.active.write().await = false;
    }

    async fn fail_run(&self, id: Uuid, message: String) {
        if let Some(record) = self.runs.write().await.get_mut(&id) {
            record.status = RunStatus::Failed;
            record.finished_at = Some(Utc::now());
            record.error_message = Some(message);
        }
        *self.active.write().await = false;
    }

    async fn get_run(&self, id: Uuid) -> Option<RunRecord> {
        self.runs.read().await.get(&id).cloned()
    }

    async fn list_runs(&self) -> Vec<RunRecord> {
        self.runs.read().await.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn try_start_run_succeeds_when_no_run_is_active() {
        let store = InMemoryRunStore::new();
        let record = store.try_start_run().await;
        assert!(record.is_some());
        assert_eq!(record.unwrap().status, RunStatus::Running);
    }

    #[tokio::test]
    async fn try_start_run_fails_while_one_is_already_active() {
        let store = InMemoryRunStore::new();
        let first = store.try_start_run().await;
        assert!(first.is_some());

        let second = store.try_start_run().await;
        assert!(second.is_none());
    }

    #[tokio::test]
    async fn completing_a_run_allows_a_new_one_to_start() {
        let store = InMemoryRunStore::new();
        let first = store.try_start_run().await.unwrap();
        store.complete_run(first.id, vec!["AAPL".to_string()]).await;

        let second = store.try_start_run().await;
        assert!(second.is_some());

        let completed = store.get_run(first.id).await.unwrap();
        assert_eq!(completed.status, RunStatus::Completed);
        assert_eq!(completed.final_watchlist, vec!["AAPL".to_string()]);
        assert!(completed.finished_at.is_some());
    }

    #[tokio::test]
    async fn failing_a_run_allows_a_new_one_to_start() {
        let store = InMemoryRunStore::new();
        let first = store.try_start_run().await.unwrap();
        store.fail_run(first.id, "boom".to_string()).await;

        let second = store.try_start_run().await;
        assert!(second.is_some());

        let failed = store.get_run(first.id).await.unwrap();
        assert_eq!(failed.status, RunStatus::Failed);
        assert_eq!(failed.error_message, Some("boom".to_string()));
    }

    #[tokio::test]
    async fn get_run_returns_none_for_unknown_id() {
        let store = InMemoryRunStore::new();
        assert!(store.get_run(uuid::Uuid::new_v4()).await.is_none());
    }

    #[tokio::test]
    async fn list_runs_returns_all_created_runs() {
        let store = InMemoryRunStore::new();
        let first = store.try_start_run().await.unwrap();
        store.complete_run(first.id, vec![]).await;
        store.try_start_run().await.unwrap();

        assert_eq!(store.list_runs().await.len(), 2);
    }
}
