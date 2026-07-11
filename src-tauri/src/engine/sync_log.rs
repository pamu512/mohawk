//! In-memory sync pipeline log ring buffer for the Threat Intel Desk dashboard.

use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use super::scraper::SyncReport;

const MAX_LOG_LINES: usize = 150;

pub const EVENT_SYNC_LOG_LINE: &str = "sync-log-line";
pub const EVENT_SYNC_STATUS_CHANGED: &str = "sync-status-changed";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncLogLine {
    pub timestamp: DateTime<Utc>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncReportSummary {
    pub synced_at: DateTime<Utc>,
    pub feeds_attempted: usize,
    pub feeds_succeeded: usize,
    pub items_extracted: usize,
    pub chunks_produced: usize,
    pub error_count: usize,
}

struct SyncLogState {
    lines: Vec<SyncLogLine>,
    last_report: Option<SyncReportSummary>,
}

static APP: OnceLock<AppHandle> = OnceLock::new();

fn state() -> &'static Mutex<SyncLogState> {
    static STATE: OnceLock<Mutex<SyncLogState>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(SyncLogState {
            lines: vec![SyncLogLine {
                timestamp: Utc::now(),
                text: "[INIT] Threat intel sync worker online — awaiting cycle trigger.".into(),
            }],
            last_report: None,
        })
    })
}

/// Register the app handle so log lines can be pushed to the frontend via Tauri events.
pub fn register_app(app: AppHandle) {
    let _ = APP.set(app);
}

pub fn app_handle() -> Option<AppHandle> {
    APP.get().cloned()
}

fn emit_log_line(line: &SyncLogLine) {
    if let Some(app) = APP.get() {
        let _ = app.emit(EVENT_SYNC_LOG_LINE, line);
    }
}

fn emit_status_changed() {
    if let Some(app) = APP.get() {
        let _ = app.emit(EVENT_SYNC_STATUS_CHANGED, ());
    }
}

/// Append a tagged line to the dashboard console (`[TAG] message`).
pub fn push(tag: &str, message: impl Into<String>) {
    push_line(format!("[{tag}] {}", message.into()));
}

pub fn push_line(text: impl Into<String>) {
    let line = SyncLogLine {
        timestamp: Utc::now(),
        text: text.into(),
    };
    if let Ok(mut guard) = state().lock() {
        guard.lines.push(line.clone());
        if guard.lines.len() > MAX_LOG_LINES {
            let drop_count = guard.lines.len() - MAX_LOG_LINES;
            guard.lines.drain(0..drop_count);
        }
    }
    emit_log_line(&line);
}

pub fn recent_logs() -> Vec<SyncLogLine> {
    state()
        .lock()
        .map(|g| g.lines.clone())
        .unwrap_or_default()
}

pub fn store_report(report: &SyncReport) {
    if let Ok(mut guard) = state().lock() {
        guard.last_report = Some(SyncReportSummary {
            synced_at: report.synced_at,
            feeds_attempted: report.feeds_attempted,
            feeds_succeeded: report.feeds_succeeded,
            items_extracted: report.items_extracted,
            chunks_produced: report.chunks_produced,
            error_count: report.errors.len(),
        });
    }
    emit_status_changed();
}

pub fn emit_worker_state_changed() {
    emit_status_changed();
}

pub fn last_report() -> Option<SyncReportSummary> {
    state()
        .lock()
        .ok()
        .and_then(|g| g.last_report.clone())
}
