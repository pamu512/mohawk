//! Staged LLM course extractions awaiting analyst accept/reject.

use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri::Emitter;
use uuid::Uuid;

use super::generator::{CourseExtraction, PersistedCourse};
use crate::errors::{AppError, AppResult};

const MAX_PENDING: usize = 20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingCourseSummary {
    pub id: String,
    pub course_title: String,
    pub category: String,
    pub source_title: String,
    pub chunk_index: usize,
    pub node_count: usize,
    pub edge_count: usize,
    pub card_count: usize,
    pub staged_at: DateTime<Utc>,
}

struct PendingCourse {
    id: String,
    course: CourseExtraction,
    source_title: String,
    chunk_index: usize,
    staged_at: DateTime<Utc>,
}

struct PendingState {
    items: Vec<PendingCourse>,
}

fn state() -> &'static Mutex<PendingState> {
    static STATE: OnceLock<Mutex<PendingState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(PendingState { items: Vec::new() }))
}

fn emit_pending_changed() {
    if let Some(app) = super::sync_log::app_handle() {
        let summaries = list_pending();
        let _ = app.emit("pending-courses-changed", summaries);
    }
}

/// Stage a validated course for analyst review (does not write to SQLite).
pub fn stage(course: CourseExtraction, source_title: String, chunk_index: usize) -> PendingCourseSummary {
    let summary = PendingCourseSummary {
        id: Uuid::new_v4().to_string(),
        course_title: course.course_title.clone(),
        category: course.category.clone(),
        source_title: source_title.clone(),
        chunk_index,
        node_count: course.new_nodes.len(),
        edge_count: course.new_edges.len(),
        card_count: course.new_cards.len(),
        staged_at: Utc::now(),
    };

    if let Ok(mut guard) = state().lock() {
        guard.items.push(PendingCourse {
            id: summary.id.clone(),
            course,
            source_title,
            chunk_index,
            staged_at: summary.staged_at,
        });
        if guard.items.len() > MAX_PENDING {
            let drop_count = guard.items.len() - MAX_PENDING;
            guard.items.drain(0..drop_count);
        }
    }

    emit_pending_changed();
    summary
}

pub fn list_pending() -> Vec<PendingCourseSummary> {
    state()
        .lock()
        .map(|g| {
            g.items
                .iter()
                .map(|item| PendingCourseSummary {
                    id: item.id.clone(),
                    course_title: item.course.course_title.clone(),
                    category: item.course.category.clone(),
                    source_title: item.source_title.clone(),
                    chunk_index: item.chunk_index,
                    node_count: item.course.new_nodes.len(),
                    edge_count: item.course.new_edges.len(),
                    card_count: item.course.new_cards.len(),
                    staged_at: item.staged_at,
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn take_pending(id: &str) -> AppResult<CourseExtraction> {
    let mut guard = state()
        .lock()
        .map_err(|_| AppError::InternalError("pending course lock poisoned".into()))?;

    let idx = guard
        .items
        .iter()
        .position(|item| item.id == id)
        .ok_or_else(|| AppError::InternalError(format!("pending course '{id}' not found")))?;

    let item = guard.items.remove(idx);
    emit_pending_changed();
    Ok(item.course)
}

pub fn reject_pending(id: &str) -> AppResult<()> {
    take_pending(id).map(|_| ())
}

pub async fn accept_pending(
    pool: &sqlx::SqlitePool,
    id: &str,
) -> AppResult<PersistedCourse> {
    let course = take_pending(id)?;
    super::generator::persist_course(pool, &course).await
}
