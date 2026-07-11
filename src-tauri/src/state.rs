//! Application-wide managed state for Tauri commands and background tasks.
//!
//! `AppState` is `Send + Sync`: the pool is shareable across async workers,
//! config mutations serialize through `RwLock`, and background work is tracked
//! with atomics (no mutex on the hot path).

use std::sync::atomic::{AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePool;
use tokio::sync::RwLock;

use crate::database;
use crate::domain::fsrs::FsrsParams;
use crate::errors::AppResult;
use tauri::AppHandle;

/// User-tunable settings loaded at startup and updated at runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub fsrs: FsrsParams,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            fsrs: FsrsParams::default(),
        }
    }
}

/// Unified orchestrator registered once via `AppHandle::manage`.
pub struct AppState {
    db: SqlitePool,
    active_background_tasks: AtomicUsize,
    config: RwLock<AppConfig>,
}

impl AppState {
    pub async fn init(app: &AppHandle) -> AppResult<Self> {
        let db = database::init(app).await?.0;
        Ok(Self {
            db,
            active_background_tasks: AtomicUsize::new(0),
            config: RwLock::new(AppConfig::default()),
        })
    }

    pub fn db(&self) -> &SqlitePool {
        &self.db
    }

    pub fn active_background_tasks(&self) -> usize {
        self.active_background_tasks.load(Ordering::Acquire)
    }

    pub fn has_background_work(&self) -> bool {
        self.active_background_tasks() > 0
    }

    /// Increment before spawning durable background work (ingestion, exports, etc.).
    pub fn begin_background_task(&self) {
        self.active_background_tasks.fetch_add(1, Ordering::Release);
    }

    /// Decrement when background work finishes; saturates at zero.
    pub fn end_background_task(&self) {
        self.active_background_tasks
            .fetch_update(Ordering::Release, Ordering::Acquire, |n| {
                Some(n.saturating_sub(1))
            })
            .ok();
    }

    pub async fn config(&self) -> AppConfig {
        self.config.read().await.clone()
    }

    pub async fn update_config(&self, f: impl FnOnce(&mut AppConfig)) {
        let mut guard = self.config.write().await;
        f(&mut guard);
    }
}

// Compile-time guarantee for Tauri's managed-state bounds.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<AppState>();
};

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn background_task_counter_tracks_in_flight_work() {
        let db = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let state = AppState {
            db,
            active_background_tasks: AtomicUsize::new(0),
            config: RwLock::new(AppConfig::default()),
        };

        state.begin_background_task();
        state.begin_background_task();
        assert_eq!(state.active_background_tasks(), 2);
        assert!(state.has_background_work());

        state.end_background_task();
        state.end_background_task();
        state.end_background_task();
        assert_eq!(state.active_background_tasks(), 0);
    }
}
