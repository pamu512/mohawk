//! Staged LLM course extractions awaiting analyst accept/reject (SQLite-backed).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use tauri::Emitter;
use tracing::{info, warn};
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingCourseDetail {
    pub summary: PendingCourseSummary,
    pub course: CourseExtraction,
}

fn row_to_summary(row: &sqlx::sqlite::SqliteRow) -> AppResult<PendingCourseSummary> {
    let staged_at_raw: String = row.try_get("staged_at")?;
    let staged_at = DateTime::parse_from_rfc3339(&staged_at_raw)
        .map_err(|e| AppError::InternalError(format!("invalid staged_at: {e}")))?
        .with_timezone(&Utc);
    let chunk_index: i64 = row.try_get("chunk_index")?;
    Ok(PendingCourseSummary {
        id: row.try_get("id")?,
        course_title: row.try_get("course_title")?,
        category: row.try_get("category")?,
        source_title: row.try_get("source_title")?,
        chunk_index: usize::try_from(chunk_index)
            .map_err(|_| AppError::InternalError("chunk_index out of range".into()))?,
        node_count: row.try_get::<i64, _>("node_count")? as usize,
        edge_count: row.try_get::<i64, _>("edge_count")? as usize,
        card_count: row.try_get::<i64, _>("card_count")? as usize,
        staged_at,
    })
}

async fn fetch_payload_json(pool: &SqlitePool, id: &str) -> AppResult<String> {
    let row = sqlx::query("SELECT payload_json FROM pending_courses WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::InternalError(format!("pending course '{id}' not found")))?;
    Ok(row.try_get("payload_json")?)
}

async fn emit_pending_changed(pool: &SqlitePool) {
    if let Some(app) = super::sync_log::app_handle() {
        match list_pending(pool).await {
            Ok(summaries) => {
                let _ = app.emit("pending-courses-changed", summaries);
            }
            Err(e) => warn!(error = %e, "failed to emit pending-courses-changed"),
        }
    }
}

async fn trim_excess(pool: &SqlitePool) -> AppResult<()> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pending_courses")
        .fetch_one(pool)
        .await?;

    if count <= MAX_PENDING as i64 {
        return Ok(());
    }

    let to_drop = count - MAX_PENDING as i64;
    sqlx::query(
        "DELETE FROM pending_courses WHERE id IN (
            SELECT id FROM pending_courses ORDER BY staged_at ASC LIMIT ?
        )",
    )
    .bind(to_drop)
    .execute(pool)
    .await?;

    info!(dropped = to_drop, "trimmed oldest pending courses beyond cap");
    Ok(())
}

/// Stage a validated course for analyst review (does not write graph/cards yet).
pub async fn stage(
    pool: &SqlitePool,
    course: CourseExtraction,
    source_title: String,
    chunk_index: usize,
) -> AppResult<PendingCourseSummary> {
    let id = Uuid::new_v4().to_string();
    let staged_at = Utc::now();
    let payload_json = serde_json::to_string(&course)?;
    let summary = PendingCourseSummary {
        id: id.clone(),
        course_title: course.course_title.clone(),
        category: course.category.clone(),
        source_title: source_title.clone(),
        chunk_index,
        node_count: course.new_nodes.len(),
        edge_count: course.new_edges.len(),
        card_count: course.new_cards.len(),
        staged_at,
    };

    sqlx::query(
        "INSERT INTO pending_courses
         (id, source_title, chunk_index, staged_at, payload_json, course_title, category,
          node_count, edge_count, card_count)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&source_title)
    .bind(i64::try_from(chunk_index).unwrap_or(0))
    .bind(staged_at.to_rfc3339())
    .bind(&payload_json)
    .bind(&summary.course_title)
    .bind(&summary.category)
    .bind(summary.node_count as i64)
    .bind(summary.edge_count as i64)
    .bind(summary.card_count as i64)
    .execute(pool)
    .await?;

    trim_excess(pool).await?;
    info!(pending_id = %id, title = %summary.course_title, "staged course for review");
    emit_pending_changed(pool).await;
    Ok(summary)
}

pub async fn list_pending(pool: &SqlitePool) -> AppResult<Vec<PendingCourseSummary>> {
    let rows = sqlx::query(
        "SELECT id, source_title, chunk_index, staged_at, course_title, category,
                node_count, edge_count, card_count
         FROM pending_courses ORDER BY staged_at DESC",
    )
    .fetch_all(pool)
    .await?;

    rows.iter().map(row_to_summary).collect()
}

pub async fn get_detail(pool: &SqlitePool, id: &str) -> AppResult<PendingCourseDetail> {
    let row = sqlx::query(
        "SELECT id, source_title, chunk_index, staged_at, course_title, category,
                node_count, edge_count, card_count
         FROM pending_courses WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::InternalError(format!("pending course '{id}' not found")))?;

    let summary = row_to_summary(&row)?;
    let payload_json = fetch_payload_json(pool, id).await?;
    let course: CourseExtraction = serde_json::from_str(&payload_json)
        .map_err(|e| AppError::InternalError(format!("pending payload_json corrupt: {e}")))?;

    Ok(PendingCourseDetail { summary, course })
}

async fn take_pending(pool: &SqlitePool, id: &str) -> AppResult<CourseExtraction> {
    let payload_json = fetch_payload_json(pool, id).await?;
    let course: CourseExtraction = serde_json::from_str(&payload_json)
        .map_err(|e| AppError::InternalError(format!("pending payload_json corrupt: {e}")))?;

    let deleted = sqlx::query("DELETE FROM pending_courses WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();

    if deleted == 0 {
        return Err(AppError::InternalError(format!(
            "pending course '{id}' not found"
        )));
    }

    emit_pending_changed(pool).await;
    Ok(course)
}

pub async fn reject_pending(pool: &SqlitePool, id: &str) -> AppResult<()> {
    take_pending(pool, id).await.map(|_| ())
}

pub async fn accept_pending(pool: &SqlitePool, id: &str) -> AppResult<PersistedCourse> {
    let course = take_pending(pool, id).await?;
    let saved = super::generator::persist_course(pool, &course).await?;
    info!(title = %saved.course_title, "accepted pending course into graph");
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::generator::{ExtractedCard, ExtractedEdge, ExtractedNode};

    fn sample_course() -> CourseExtraction {
        CourseExtraction {
            course_title: "Test Course".into(),
            category: "payments".into(),
            new_nodes: vec![ExtractedNode {
                id: "550e8400-e29b-41d4-a716-446655440000".into(),
                entity_type: "vector".into(),
                title: "Node A".into(),
                description: "Desc".into(),
            }],
            new_edges: vec![ExtractedEdge {
                source_node_id: "550e8400-e29b-41d4-a716-446655440000".into(),
                target_node_id: "550e8400-e29b-41d4-a716-446655440000".into(),
                relationship_type: "INDICATES".into(),
            }],
            new_cards: vec![ExtractedCard {
                card_type: "recall".into(),
                question: "Q?".into(),
                answer: "A".into(),
                payload_mock: serde_json::json!({}),
            }],
        }
    }

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn stage_and_list_round_trip() {
        let pool = test_pool().await;
        let summary = stage(&pool, sample_course(), "Article".into(), 0)
            .await
            .unwrap();
        let listed = list_pending(&pool).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, summary.id);
        assert_eq!(listed[0].course_title, "Test Course");
    }

    #[tokio::test]
    async fn get_detail_returns_full_course() {
        let pool = test_pool().await;
        let summary = stage(&pool, sample_course(), "Src".into(), 1)
            .await
            .unwrap();
        let detail = get_detail(&pool, &summary.id).await.unwrap();
        assert_eq!(detail.summary.id, summary.id);
        assert_eq!(detail.course.new_cards.len(), 1);
    }
}
