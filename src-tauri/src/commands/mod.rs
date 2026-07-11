//! Tauri IPC command handlers exposed to the frontend.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::domain::fsrs::{apply_review, CardState, FsrsPhase, Rating, ReviewLog};
use crate::engine::ai::{
    AiGenerationOutcome, CardType, LocalAiClient, LocalInferenceConfig,
};
use crate::engine::evaluator::{self, ChallengeLanguage, SandboxResult};
use crate::errors::AppError;
use crate::state::AppState;
use tauri::Manager;

// --- Response models ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewCard {
    pub id: String,
    pub card_type: String,
    pub category: Option<String>,
    pub difficulty_tier: Option<String>,
    pub created_at: Option<String>,
    pub data: serde_json::Value,
    pub stability: f64,
    pub difficulty: f64,
    pub lapses: u32,
    pub reviews: u32,
    pub state: u8,
    pub last_review: Option<DateTime<Utc>>,
    pub next_review: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct KnowledgeGraphNode {
    pub id: String,
    pub entity_type: String,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct KnowledgeGraphEdge {
    pub source_node_id: String,
    pub target_node_id: String,
    pub relationship_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGraph {
    pub nodes: Vec<KnowledgeGraphNode>,
    pub edges: Vec<KnowledgeGraphEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedGeneratedCard {
    pub id: String,
    pub card_type: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateCardsResponse {
    pub cards: Vec<SavedGeneratedCard>,
}

#[derive(Debug, FromRow)]
struct ReviewCardRow {
    id: String,
    card_type: String,
    category: Option<String>,
    difficulty_tier: Option<String>,
    created_at: Option<String>,
    data: String,
    stability: f64,
    difficulty: f64,
    lapses: i64,
    reviews: i64,
    state: i64,
    last_review: Option<String>,
    next_review: Option<String>,
}

/// Fetch due cards ordered by soonest review time.
#[tauri::command]
pub async fn get_next_review_cards(
    state: tauri::State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<ReviewCard>, AppError> {
    let limit = limit.unwrap_or(20).clamp(1, 500) as i64;

    let rows = sqlx::query_as::<_, ReviewCardRow>(
        "SELECT c.id, c.type AS card_type, c.category, c.difficulty_tier, c.created_at, c.data,
                f.stability, f.difficulty, f.lapses, f.reviews, f.state,
                f.last_review, f.next_review
         FROM cards c
         INNER JOIN fsrs_states f ON f.card_id = c.id
         WHERE f.state = 0
            OR f.next_review IS NULL
            OR f.next_review <= strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
         ORDER BY f.next_review ASC
         LIMIT ?",
    )
    .bind(limit)
    .fetch_all(state.db())
    .await?;

    rows.into_iter().map(review_card_from_row).collect()
}

/// Record an FSRS rating (1–4) and persist updated scheduling state.
#[tauri::command]
pub async fn submit_review_score(
    state: tauri::State<'_, AppState>,
    card_id: String,
    score: u8,
) -> Result<ReviewCard, AppError> {
    let rating = Rating::from_u8(score)
        .ok_or_else(|| AppError::InternalError(format!("invalid review score: {score}")))?;

    let card_row = sqlx::query_as::<_, ReviewCardRow>(
        "SELECT c.id, c.type AS card_type, c.category, c.difficulty_tier, c.created_at, c.data,
                f.stability, f.difficulty, f.lapses, f.reviews, f.state,
                f.last_review, f.next_review
         FROM cards c
         INNER JOIN fsrs_states f ON f.card_id = c.id
         WHERE c.id = ?",
    )
    .bind(&card_id)
    .fetch_optional(state.db())
    .await?
    .ok_or_else(|| AppError::InternalError(format!("card not found: {card_id}")))?;

    let fsrs = card_state_from_row(&card_row)?;
    let now = Utc::now();
    let elapsed = card_row
        .last_review
        .as_deref()
        .and_then(parse_timestamp)
        .map(|last| crate::domain::fsrs::elapsed_days_between(last, now))
        .unwrap_or(0.0);

    let config = state.config().await;
    let updated = apply_review(
        &fsrs,
        ReviewLog {
            rating,
            reviewed_at: now,
            elapsed_days: elapsed,
        },
        &config.fsrs,
    );

    sqlx::query(
        "UPDATE fsrs_states
         SET stability = ?, difficulty = ?, lapses = ?, reviews = ?, state = ?,
             last_review = ?, next_review = ?
         WHERE card_id = ?",
    )
    .bind(updated.stability)
    .bind(updated.difficulty)
    .bind(updated.lapses as i64)
    .bind(updated.reviews as i64)
    .bind(updated.phase as i64)
    .bind(format_timestamp(updated.last_review))
    .bind(format_timestamp(updated.next_review))
    .bind(&card_id)
    .execute(state.db())
    .await?;

    let refreshed = sqlx::query_as::<_, ReviewCardRow>(
        "SELECT c.id, c.type AS card_type, c.category, c.difficulty_tier, c.created_at, c.data,
                f.stability, f.difficulty, f.lapses, f.reviews, f.state,
                f.last_review, f.next_review
         FROM cards c
         INNER JOIN fsrs_states f ON f.card_id = c.id
         WHERE c.id = ?",
    )
    .bind(&card_id)
    .fetch_one(state.db())
    .await?;

    review_card_from_row(refreshed)
}

/// Return the full knowledge graph (nodes + edges).
#[tauri::command]
pub async fn get_knowledge_graph(
    state: tauri::State<'_, AppState>,
) -> Result<KnowledgeGraph, AppError> {
    let nodes = sqlx::query_as::<_, KnowledgeGraphNode>(
        "SELECT id, entity_type, title, description FROM nodes ORDER BY title",
    )
    .fetch_all(state.db())
    .await?;

    let edges = sqlx::query_as::<_, KnowledgeGraphEdge>(
        "SELECT source_node_id, target_node_id, relationship_type FROM edges",
    )
    .fetch_all(state.db())
    .await?;

    Ok(KnowledgeGraph { nodes, edges })
}

/// Run sandbox heuristic rules or a language challenge against test payloads.
#[tauri::command]
pub async fn execute_sandbox_rules(
    state: tauri::State<'_, AppState>,
    transactions: Vec<String>,
    rule_definition: String,
    challenge_language: Option<ChallengeLanguage>,
) -> Result<SandboxResult, AppError> {
    state.begin_background_task();
    let report = tokio::task::spawn_blocking(move || {
        if let Some(language) = challenge_language {
            if transactions.is_empty() {
                evaluator::evaluate_challenge(&rule_definition, language)
            } else {
                evaluator::evaluate_challenge_against(&rule_definition, language, &transactions)
            }
        } else {
            evaluator::evaluate_transactions(&transactions, &rule_definition).map(SandboxResult::from)
        }
    })
    .await
    .map_err(|e| AppError::InternalError(format!("sandbox worker failed: {e}")))?;
    state.end_background_task();
    report
}

/// Generate flashcards from raw text via a local Ollama/LM Studio instance.
#[tauri::command]
pub async fn generate_cards_from_text(
    state: tauri::State<'_, AppState>,
    source_text: String,
    inference: Option<LocalInferenceConfig>,
) -> Result<GenerateCardsResponse, AppError> {
    if source_text.trim().is_empty() {
        return Err(AppError::InternalError(
            "source text must not be empty".into(),
        ));
    }

    state.begin_background_task();
    let config = match inference {
        Some(c) => c,
        None => LocalInferenceConfig::load(state.db()).await?,
    };
    let client = match LocalAiClient::new(config) {
        Ok(c) => c,
        Err(e) => {
            state.end_background_task();
            return Err(e);
        }
    };
    let outcome = client.generate_cards(&source_text).await;
    state.end_background_task();

    let generated = match outcome {
        AiGenerationOutcome::Ok(result) => result,
        AiGenerationOutcome::Err(envelope) => {
            return Err(AppError::InternalError(format!(
                "card generation failed at {}: {}",
                envelope.stage, envelope.message
            )));
        }
    };

    let mut saved = Vec::with_capacity(generated.cards.len());
    let mut tx = state.db().begin().await?;

    for card in generated.cards {
        let id = Uuid::new_v4().to_string();
        let card_type = card_type_to_sql(card.card_type);
        let data = serde_json::to_string(&card.data)?;

        sqlx::query("INSERT INTO cards (id, type, data) VALUES (?, ?, ?)")
            .bind(&id)
            .bind(card_type)
            .bind(&data)
            .execute(&mut *tx)
            .await?;

        sqlx::query(
            "INSERT INTO fsrs_states (card_id, stability, difficulty, lapses, reviews, state)
             VALUES (?, 0, 0, 0, 0, 0)",
        )
        .bind(&id)
        .execute(&mut *tx)
        .await?;

        saved.push(SavedGeneratedCard {
            id,
            card_type: card_type.to_string(),
            data: card.data,
        });
    }

    tx.commit().await?;
    Ok(GenerateCardsResponse { cards: saved })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedCardSummary {
    pub id: String,
    pub card_type: String,
    pub question: Option<String>,
}

/// Flashcards linked to a knowledge-graph node via `card_node_links`.
#[tauri::command]
pub async fn get_node_linked_cards(
    state: tauri::State<'_, AppState>,
    node_id: String,
) -> Result<Vec<LinkedCardSummary>, AppError> {
    let rows = sqlx::query_as::<_, (String, String, String)>(
        "SELECT c.id, c.type, c.data
         FROM cards c
         INNER JOIN card_node_links l ON l.card_id = c.id
         WHERE l.node_id = ?
         ORDER BY c.created_at DESC",
    )
    .bind(&node_id)
    .fetch_all(state.db())
    .await?;

    rows.into_iter()
        .map(|(id, card_type, data)| {
            let parsed: serde_json::Value = serde_json::from_str(&data)?;
            let question = parsed
                .get("question")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            Ok(LinkedCardSummary {
                id,
                card_type,
                question,
            })
        })
        .collect()
}

/// Create a manual recall flashcard and link it to a graph node.
#[tauri::command]
pub async fn create_manual_card_for_node(
    state: tauri::State<'_, AppState>,
    node_id: String,
    question: String,
    answer: String,
) -> Result<SavedGeneratedCard, AppError> {
    let question = question.trim();
    let answer = answer.trim();
    if question.is_empty() || answer.is_empty() {
        return Err(AppError::InternalError(
            "question and answer must not be empty".into(),
        ));
    }

    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM nodes WHERE id = ?")
        .bind(&node_id)
        .fetch_optional(state.db())
        .await?;
    if exists.is_none() {
        return Err(AppError::InternalError(format!("node not found: {node_id}")));
    }

    let id = Uuid::new_v4().to_string();
    let data = serde_json::json!({
        "question": question,
        "answer": answer,
        "payload": null,
        "source": "manual_topology",
        "linked_node_id": node_id,
    });
    let data_str = serde_json::to_string(&data)?;

    let mut tx = state.db().begin().await?;

    sqlx::query("INSERT INTO cards (id, type, data) VALUES (?, 'recall', ?)")
        .bind(&id)
        .bind(&data_str)
        .execute(&mut *tx)
        .await?;

    sqlx::query(
        "INSERT INTO fsrs_states (card_id, stability, difficulty, lapses, reviews, state)
         VALUES (?, 0, 0, 0, 0, 0)",
    )
    .bind(&id)
    .execute(&mut *tx)
    .await?;

    sqlx::query("INSERT INTO card_node_links (card_id, node_id) VALUES (?, ?)")
        .bind(&id)
        .bind(&node_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(SavedGeneratedCard {
        id,
        card_type: "recall".into(),
        data,
    })
}

fn review_card_from_row(row: ReviewCardRow) -> Result<ReviewCard, AppError> {
    Ok(ReviewCard {
        id: row.id,
        card_type: row.card_type,
        category: row.category,
        difficulty_tier: row.difficulty_tier,
        created_at: row.created_at,
        data: serde_json::from_str(&row.data)?,
        stability: row.stability,
        difficulty: row.difficulty,
        lapses: row.lapses.max(0) as u32,
        reviews: row.reviews.max(0) as u32,
        state: row.state.clamp(0, 3) as u8,
        last_review: row.last_review.as_deref().and_then(parse_timestamp),
        next_review: row.next_review.as_deref().and_then(parse_timestamp),
    })
}

fn card_state_from_row(row: &ReviewCardRow) -> Result<CardState, AppError> {
    Ok(CardState {
        stability: row.stability,
        difficulty: row.difficulty,
        lapses: row.lapses.max(0) as u32,
        reviews: row.reviews.max(0) as u32,
        phase: FsrsPhase::from_u8(row.state.clamp(0, 3) as u8).unwrap_or(FsrsPhase::New),
        last_review: row.last_review.as_deref().and_then(parse_timestamp),
        next_review: row.next_review.as_deref().and_then(parse_timestamp),
    })
}

fn parse_timestamp(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn format_timestamp(value: Option<DateTime<Utc>>) -> Option<String> {
    value.map(|dt| dt.to_rfc3339())
}

fn card_type_to_sql(ty: CardType) -> &'static str {
    match ty {
        CardType::PayloadDrill => "payload_drill",
        CardType::LogicSandbox => "logic_sandbox",
        CardType::Recall => "recall",
    }
}

// --- Threat Intel Desk (sync dashboard) ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncDashboardStatus {
    pub last_sync_at: Option<String>,
    pub next_sync_at: String,
    pub seconds_until_next_sync: i64,
    pub sync_interval_days: i64,
    pub sync_is_due: bool,
    pub sync_in_progress: bool,
    pub sources: Vec<crate::engine::scraper::IngestionSource>,
    pub logs: Vec<crate::engine::sync_log::SyncLogLine>,
    pub last_report: Option<crate::engine::sync_log::SyncReportSummary>,
    pub ollama: crate::engine::ai::OllamaHealth,
    pub pending_courses: Vec<crate::engine::pending_courses::PendingCourseSummary>,
}

/// Pollable sync pipeline status for the Threat Intel Desk dashboard.
#[tauri::command]
pub async fn get_sync_dashboard_status(
    state: tauri::State<'_, AppState>,
) -> Result<SyncDashboardStatus, AppError> {
    use crate::engine::ai::{check_ollama_health, LocalInferenceConfig};
    use crate::engine::pending_courses;
    use crate::engine::scraper::{
        ingestion_sources, next_sync_at, read_last_sync_timestamp, seconds_until_next_sync,
        sync_is_due_now, SYNC_INTERVAL_DAYS,
    };
    use crate::engine::sync_log;

    let now = Utc::now();
    let last = read_last_sync_timestamp(state.db()).await?;
    let inference = LocalInferenceConfig::load(state.db()).await?;
    let ollama = check_ollama_health(&inference).await;

    Ok(SyncDashboardStatus {
        last_sync_at: last.map(|t| t.to_rfc3339()),
        next_sync_at: next_sync_at(last, now).to_rfc3339(),
        seconds_until_next_sync: seconds_until_next_sync(last, now),
        sync_interval_days: SYNC_INTERVAL_DAYS,
        sync_is_due: sync_is_due_now(last, now),
        sync_in_progress: state.has_background_work(),
        sources: ingestion_sources(),
        logs: sync_log::recent_logs(),
        last_report: sync_log::last_report(),
        ollama,
        pending_courses: pending_courses::list_pending(state.db()).await?,
    })
}

/// Manually trigger the curriculum scraper + Ollama synthesis pipeline.
#[tauri::command]
pub async fn force_sync_curriculum(app: tauri::AppHandle) -> Result<(), AppError> {
    use crate::engine::scraper::force_sync_cycle;
    use crate::engine::sync_log;

    let state = app.state::<AppState>();
    if state.has_background_work() {
        return Err(AppError::InternalError(
            "sync pipeline already running — wait for the current cycle to finish".into(),
        ));
    }

    state.begin_background_task();
    sync_log::emit_worker_state_changed();
    let pool = state.db().clone();

    tauri::async_runtime::spawn(async move {
        let outcome = force_sync_cycle(&pool).await;
        match outcome {
            Ok(report) => {
                sync_log::push(
                    "COMPLETE",
                    format!(
                        "Force sync finished — {} chunks, {} errors",
                        report.chunks_produced,
                        report.errors.len()
                    ),
                );
            }
            Err(e) => sync_log::push("ERROR", format!("Force sync aborted: {e}")),
        }
        app.state::<AppState>().end_background_task();
        sync_log::emit_worker_state_changed();
    });

    Ok(())
}

/// Persist a staged course into the knowledge graph after analyst review.
#[tauri::command]
pub async fn accept_pending_course(
    state: tauri::State<'_, AppState>,
    pending_id: String,
) -> Result<crate::engine::generator::PersistedCourse, AppError> {
    use crate::engine::pending_courses;
    use crate::engine::sync_log;

    let saved = pending_courses::accept_pending(state.db(), &pending_id).await?;
    sync_log::push(
        "COMPLETE",
        format!(
            "Accepted course '{}' — {} node(s), {} card(s) committed",
            saved.course_title,
            saved.node_ids.len(),
            saved.card_ids.len()
        ),
    );
    Ok(saved)
}

/// Discard a staged course without writing graph/cards.
#[tauri::command]
pub async fn reject_pending_course(
    state: tauri::State<'_, AppState>,
    pending_id: String,
) -> Result<(), AppError> {
    use crate::engine::pending_courses;
    use crate::engine::sync_log;

    pending_courses::reject_pending(state.db(), &pending_id).await?;
    sync_log::push("REVIEW", format!("Rejected staged course '{pending_id}'"));
    Ok(())
}

/// Full staged course payload for analyst preview before accept/reject.
#[tauri::command]
pub async fn get_pending_course_detail(
    state: tauri::State<'_, AppState>,
    pending_id: String,
) -> Result<crate::engine::pending_courses::PendingCourseDetail, AppError> {
    use crate::engine::pending_courses;
    pending_courses::get_detail(state.db(), &pending_id).await
}

/// Read persisted Ollama connection settings.
#[tauri::command]
pub async fn get_inference_settings(
    state: tauri::State<'_, AppState>,
) -> Result<LocalInferenceConfig, AppError> {
    LocalInferenceConfig::load(state.db()).await
}

/// Persist Ollama host/port/model for sync and generation pipelines.
#[tauri::command]
pub async fn update_inference_settings(
    state: tauri::State<'_, AppState>,
    settings: LocalInferenceConfig,
) -> Result<(), AppError> {
    LocalInferenceConfig::save(state.db(), &settings).await
}

/// Export all study cards as CSV or Anki TSV for external review.
#[tauri::command]
pub async fn export_study_cards(
    state: tauri::State<'_, AppState>,
    format: crate::engine::export::ExportFormat,
) -> Result<crate::engine::export::ExportResult, AppError> {
    crate::engine::export::export_study_cards(state.db(), format).await
}
