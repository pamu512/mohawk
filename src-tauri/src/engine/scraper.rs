//! Background curriculum sync: fetch open security / fraud / regulatory feeds,
//! strip HTML boilerplate, chunk prose for downstream LLM ingestion.
//!
//! The pure ingest path (RSS/Atom parsing, HTML stripping, chunking, source
//! allowlist, sync-schedule math) lives in the `mohawk-ingest` workspace
//! crate; this module owns HTTP fetch, timestamp persistence, and synthesis
//! orchestration, and re-exports the crate surface for app callers.

use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::redirect::Policy;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tauri::AppHandle;
use tauri::Manager;
use tracing::{info, warn};

use crate::engine::ai::{LocalAiClient, LocalInferenceConfig};
use crate::engine::generator;
use crate::engine::sync_log;
use crate::errors::{AppError, AppResult};
use crate::state::AppState;
use mohawk_ingest::{
    chunk_text, feed_urls, normalize_whitespace, parse_rss_or_atom, source_label_for_url,
    strip_html, validate_feed_url, FeedItem,
};

pub use mohawk_ingest::{
    ingestion_sources, next_sync_at, seconds_until_next_sync, sync_is_due_now, IngestionSource,
    TextChunk, DEFAULT_CHUNK_CHARS, SYNC_INTERVAL_DAYS,
};

pub const CONFIG_KEY_LAST_SYNC: &str = "last_sync_timestamp";

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_FEED_BYTES: usize = 2 * 1024 * 1024;
const USER_AGENT: &str = "Mohawk/0.1 (local-first fraud study; +https://github.com/mohawk)";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncReport {
    pub synced_at: DateTime<Utc>,
    pub feeds_attempted: usize,
    pub feeds_succeeded: usize,
    pub items_extracted: usize,
    pub chunks_produced: usize,
    pub errors: Vec<String>,
}

/// Spawn a startup sync when the 14-day interval has elapsed (non-blocking).
pub fn spawn_startup_sync_if_due(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        match maybe_run_startup_sync(&app).await {
            Ok(report) => {
                if report.feeds_attempted > 0 {
                    sync_log::push(
                        "COMPLETE",
                        format!(
                            "Biweekly sync finished — {} chunks from {} items ({} feeds ok)",
                            report.chunks_produced,
                            report.items_extracted,
                            report.feeds_succeeded
                        ),
                    );
                }
            }
            Err(e) => sync_log::push("ERROR", format!("Curriculum sync failed: {e}")),
        }
    });
}

/// Read `last_sync_timestamp`, run fetch pipeline when due, persist new timestamp.
pub async fn maybe_run_startup_sync(app: &AppHandle) -> AppResult<SyncReport> {
    let state = app.state::<AppState>();
    let pool = state.db().clone();

    if !sync_is_due(&pool).await? {
        return Ok(SyncReport {
            synced_at: Utc::now(),
            feeds_attempted: 0,
            feeds_succeeded: 0,
            items_extracted: 0,
            chunks_produced: 0,
            errors: Vec::new(),
        });
    }

    state.begin_background_task();
    sync_log::emit_worker_state_changed();
    sync_log::push("SCHEDULER", "Automated biweekly sync cycle initiated.");
    let result = run_sync_cycle(&pool).await;
    state.end_background_task();
    sync_log::emit_worker_state_changed();
    result
}

/// Run a full fetch → chunk → synthesize pipeline, ignoring the biweekly gate.
pub async fn force_sync_cycle(pool: &SqlitePool) -> AppResult<SyncReport> {
    sync_log::push("FETCHING", "Analyst force-sync override engaged.");
    run_sync_cycle(pool).await
}

async fn sync_is_due(pool: &SqlitePool) -> AppResult<bool> {
    let last = read_last_sync_timestamp(pool).await?;
    Ok(sync_is_due_now(last, Utc::now()))
}

pub async fn read_last_sync_timestamp(pool: &SqlitePool) -> AppResult<Option<DateTime<Utc>>> {
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT value FROM app_config WHERE key = ?",
    )
    .bind(CONFIG_KEY_LAST_SYNC)
    .fetch_optional(pool)
    .await?;

    let Some(value) = raw else {
        return Ok(None);
    };
    if value.trim().is_empty() {
        return Ok(None);
    }

    DateTime::parse_from_rfc3339(value.trim())
        .map(|dt| dt.with_timezone(&Utc))
        .map(Some)
        .map_err(|e| AppError::InternalError(format!("invalid last_sync_timestamp: {e}")))
}

pub async fn write_last_sync_timestamp(pool: &SqlitePool, at: DateTime<Utc>) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO app_config (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(CONFIG_KEY_LAST_SYNC)
    .bind(at.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn run_sync_cycle(pool: &SqlitePool) -> AppResult<SyncReport> {
    info!("curriculum sync cycle starting");
    sync_log::push("FETCHING", "Grabbing RSS feeds from threat intel matrix...");

    let client = Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .redirect(Policy::limited(5))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| AppError::InternalError(format!("http client init failed: {e}")))?;

    let mut errors = Vec::new();
    let mut feeds_succeeded = 0usize;
    let mut chunks = Vec::new();
    let mut titles_seen = std::collections::HashSet::new();

    for (url, result) in fetch_all_feeds(&client).await {
        let label = source_label_for_url(url);
        match result {
            Ok(items) => {
                feeds_succeeded += 1;
                sync_log::push(
                    "FETCHING",
                    format!("{label} — pulled {} item(s).", items.len()),
                );
                for item in items {
                    titles_seen.insert(item.title.clone());
                    let clean = strip_html(&item.body);
                    let prose = normalize_whitespace(&clean);
                    if prose.len() < 80 {
                        continue;
                    }
                    for (idx, chunk) in chunk_text(&prose, DEFAULT_CHUNK_CHARS)
                        .into_iter()
                        .enumerate()
                    {
                        chunks.push(TextChunk {
                            source_url: url.to_string(),
                            title: item.title.clone(),
                            chunk_index: idx,
                            text: chunk,
                        });
                    }
                }
            }
            Err(e) => {
                sync_log::push("WARN", format!("{label} feed unreachable: {e}"));
                errors.push(format!("{url}: {e}"));
            }
        }
    }

    let items_extracted = titles_seen.len();
    let chunks_produced = chunks.len();
    let synced_at = Utc::now();

    sync_log::push(
        "CHUNKING",
        format!("Segmented {chunks_produced} prose chunk(s) from {items_extracted} article(s)."),
    );

    if feeds_succeeded > 0 && chunks_produced > 0 {
        write_last_sync_timestamp(pool, synced_at).await?;

        sync_log::push(
            "SYNTHESIZING",
            "Ollama compiling payload structures into course graph...",
        );

        match LocalInferenceConfig::load(pool).await {
            Ok(inference) => match LocalAiClient::new(inference) {
            Ok(client) => match generator::generate_from_chunks(pool, &client, &chunks).await {
                Ok(gen) => {
                    if gen.courses_staged > 0 {
                        sync_log::push(
                            "SYNTHESIZING",
                            format!(
                                "Staged {} course(s) for review — {} node(s), {} card(s).",
                                gen.courses_staged, gen.nodes_staged, gen.cards_staged
                            ),
                        );
                    } else if gen.chunks_attempted > 0 {
                        sync_log::push(
                            "SYNTHESIZING",
                            "Ollama pass complete — no new courses met validation threshold.",
                        );
                    }
                    for err in &gen.errors {
                        sync_log::push("WARN", err.clone());
                    }
                    errors.extend(gen.errors);
                }
                Err(e) => {
                    sync_log::push("ERROR", format!("Course generation batch failed: {e}"));
                    errors.push(format!("course generation batch failed: {e}"));
                }
            },
            Err(e) => {
                sync_log::push("ERROR", format!("Ollama client init failed: {e}"));
                errors.push(format!("ollama client init failed: {e}"));
            }
        },
            Err(e) => {
                sync_log::push("ERROR", format!("Inference settings load failed: {e}"));
                errors.push(format!("inference settings load failed: {e}"));
            }
        }
    } else if feeds_succeeded == 0 {
        sync_log::push("ERROR", "All ingestion sources unreachable.");
        return Err(AppError::InternalError(format!(
            "curriculum sync failed: all feeds unreachable ({})",
            errors.join("; ")
        )));
    }

    let urls = feed_urls();
    let report = SyncReport {
        synced_at,
        feeds_attempted: urls.len(),
        feeds_succeeded,
        items_extracted,
        chunks_produced,
        errors,
    };
    sync_log::store_report(&report);
    info!(
        feeds_ok = feeds_succeeded,
        chunks = chunks_produced,
        "curriculum sync cycle finished"
    );
    Ok(report)
}

async fn fetch_all_feeds(
    client: &Client,
) -> Vec<(&'static str, Result<Vec<FeedItem>, AppError>)> {
    let urls = feed_urls();
    let mut out = Vec::with_capacity(urls.len());
    for url in urls {
        let result = fetch_feed(client, url).await;
        out.push((url, result));
    }
    out
}

async fn fetch_feed(client: &Client, url: &str) -> AppResult<Vec<FeedItem>> {
    validate_feed_url(url)?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::InternalError(format!("GET {url} failed: {e}")))?;

    if !response.status().is_success() {
        return Err(AppError::InternalError(format!(
            "GET {url} returned HTTP {}",
            response.status()
        )));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| AppError::InternalError(format!("read body from {url}: {e}")))?;

    if bytes.len() > MAX_FEED_BYTES {
        warn!(url, bytes = bytes.len(), "feed response exceeded size cap");
        return Err(AppError::InternalError(format!(
            "feed body from {url} exceeds {MAX_FEED_BYTES} byte cap"
        )));
    }

    let body = String::from_utf8(bytes.to_vec())
        .map_err(|e| AppError::InternalError(format!("feed body from {url} is not UTF-8: {e}")))?;

    parse_rss_or_atom(&body).map_err(|e| AppError::InternalError(format!("parse {url}: {e}")))
}
