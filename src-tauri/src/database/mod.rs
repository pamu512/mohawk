use std::ops::Deref;
use std::path::PathBuf;
use std::time::Duration;

use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use tauri::{AppHandle, Manager};

use crate::errors::{AppError, AppResult};

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// Thread-safe SQLite pool registered as Tauri managed state.
#[derive(Clone)]
pub struct DbState(pub SqlitePool);

impl Deref for DbState {
    type Target = SqlitePool;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DbState {
    pub fn pool(&self) -> &SqlitePool {
        &self.0
    }
}

pub fn resolve_db_path(app: &AppHandle) -> AppResult<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::InternalError(e.to_string()))?;

    Ok(dir.join("mohawk.db"))
}

async fn connect_pool(path: &PathBuf) -> AppResult<SqlitePool> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    Ok(pool)
}

pub async fn run_migrations(pool: &SqlitePool) -> AppResult<()> {
    MIGRATOR.run(pool).await?;
    Ok(())
}

pub async fn init(app: &AppHandle) -> AppResult<DbState> {
    let path = resolve_db_path(app)?;
    let pool = connect_pool(&path).await?;
    run_migrations(&pool).await?;
    Ok(DbState(pool))
}
