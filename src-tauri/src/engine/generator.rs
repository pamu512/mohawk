//! LLM structured extraction: raw threat prose → course graph + flashcards in SQLite.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::engine::ai::{AiErrorEnvelope, LocalAiClient};
use crate::engine::pending_courses;
use crate::engine::scraper::TextChunk;
use crate::engine::sync_log;
use crate::errors::{AppError, AppResult};

/// Cap LLM calls per scraper sync cycle (ponytail: avoid hammering local Ollama).
pub const MAX_CHUNKS_PER_SYNC: usize = 5;

const MAX_PARSE_ATTEMPTS: usize = 3;

const ALLOWED_CATEGORIES: &[&str] = &[
    "payments",
    "account_security",
    "trust_safety",
    "sql",
    "python",
];

const ALLOWED_ENTITY_TYPES: &[&str] = &["vector", "indicator"];

const ALLOWED_CARD_TYPES: &[&str] = &["payload_drill", "recall"];

const COURSE_EXTRACTION_SYSTEM: &str = r#"You are a fraud-intelligence curriculum architect. Parse the user's threat-intelligence text into ONE study course.

Respond with valid JSON ONLY — no markdown fences, no commentary. The root object MUST match this schema exactly:

{
  "course_title": "String",
  "category": "payments | account_security | trust_safety | sql | python",
  "new_nodes": [
    {
      "id": "UUID",
      "entity_type": "vector | indicator",
      "title": "String",
      "description": "String"
    }
  ],
  "new_edges": [
    {
      "source_node_id": "UUID",
      "target_node_id": "UUID",
      "relationship_type": "String"
    }
  ],
  "new_cards": [
    {
      "card_type": "payload_drill | recall",
      "question": "String",
      "answer": "String",
      "payload_mock": {}
    }
  ]
}

Rules:
- Generate RFC-4122 UUIDs (version 4) for every new_nodes[].id.
- new_edges source_node_id and target_node_id MUST reference ids from new_nodes.
- relationship_type examples: ENABLES, CORRELATES_WITH, INDICATES, MITIGATES, MANDATES.
- payload_drill cards MUST include a realistic payload_mock object (transaction telemetry, device signals, etc.).
- recall cards MUST set payload_mock to {}.
- Produce 2–6 nodes, 1–5 edges, and 2–4 cards grounded in the source text.
- category MUST be exactly one of the five enum values above."#;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CourseExtraction {
    pub course_title: String,
    pub category: String,
    pub new_nodes: Vec<ExtractedNode>,
    pub new_edges: Vec<ExtractedEdge>,
    pub new_cards: Vec<ExtractedCard>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractedNode {
    pub id: String,
    pub entity_type: String,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractedEdge {
    pub source_node_id: String,
    pub target_node_id: String,
    pub relationship_type: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractedCard {
    pub card_type: String,
    pub question: String,
    pub answer: String,
    #[serde(default)]
    pub payload_mock: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedCourse {
    pub course_title: String,
    pub category: String,
    pub node_ids: Vec<String>,
    pub card_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenerationBatchReport {
    pub courses_staged: usize,
    pub nodes_staged: usize,
    pub cards_staged: usize,
    pub chunks_attempted: usize,
    pub errors: Vec<String>,
}

/// Process scraped text chunks through Ollama and stage validated courses for analyst review.
pub async fn generate_from_chunks(
    pool: &SqlitePool,
    client: &LocalAiClient,
    chunks: &[TextChunk],
) -> AppResult<GenerationBatchReport> {
    let mut report = GenerationBatchReport {
        courses_staged: 0,
        nodes_staged: 0,
        cards_staged: 0,
        chunks_attempted: 0,
        errors: Vec::new(),
    };

    for chunk in chunks.iter().take(MAX_CHUNKS_PER_SYNC) {
        report.chunks_attempted += 1;
        match extract_and_stage_course(pool, client, chunk).await {
            Ok(staged) => {
                report.courses_staged += 1;
                report.nodes_staged += staged.node_count;
                report.cards_staged += staged.card_count;
                sync_log::push(
                    "REVIEW",
                    format!(
                        "Staged '{}' — {} node(s), {} card(s) awaiting analyst accept",
                        staged.course_title, staged.node_count, staged.card_count
                    ),
                );
            }
            Err(e) => report.errors.push(format!("{} (chunk {}): {e}", chunk.title, chunk.chunk_index)),
        }
    }

    Ok(report)
}

/// Run the LLM extraction + validation loop, then stage for review (no DB write).
pub async fn extract_and_stage_course(
    pool: &SqlitePool,
    client: &LocalAiClient,
    chunk: &TextChunk,
) -> AppResult<pending_courses::PendingCourseSummary> {
    let course = request_validated_course(client, chunk).await?;
    pending_courses::stage(
        pool,
        course,
        chunk.title.clone(),
        chunk.chunk_index,
    )
    .await
}

async fn request_validated_course(
    client: &LocalAiClient,
    chunk: &TextChunk,
) -> AppResult<CourseExtraction> {
    let base_prompt = build_user_prompt(chunk);
    let mut last_err: Option<AppError> = None;

    for attempt in 0..MAX_PARSE_ATTEMPTS {
        let user_prompt = if attempt == 0 {
            base_prompt.clone()
        } else {
            format!(
                "{base_prompt}\n\nYour previous JSON was invalid ({attempt}/{MAX_PARSE_ATTEMPTS}): {}. \
                 Return corrected JSON matching the schema exactly.",
                last_err.as_ref().map(|e| e.to_string()).unwrap_or_default()
            )
        };

        let raw = match client
            .request_json_completion(COURSE_EXTRACTION_SYSTEM, &user_prompt)
            .await
        {
            Ok(text) => text,
            Err(AiErrorEnvelope { stage, message }) => {
                return Err(AppError::InternalError(format!(
                    "ollama {stage}: {message}"
                )));
            }
        };

        match parse_and_validate(&raw) {
            Ok(course) => return Ok(course),
            Err(e) => last_err = Some(e),
        }
    }

    Err(last_err.unwrap_or_else(|| {
        AppError::InternalError("course extraction failed after validation retries".into())
    }))
}

fn build_user_prompt(chunk: &TextChunk) -> String {
    format!(
        "Source feed: {}\nArticle title: {}\nChunk index: {}\n\n--- THREAT INTELLIGENCE TEXT ---\n{}\n--- END ---",
        chunk.source_url, chunk.title, chunk.chunk_index, chunk.text
    )
}

/// Parse raw LLM output and enforce the rigid course schema.
pub fn parse_and_validate(raw: &str) -> AppResult<CourseExtraction> {
    let json_str = strip_code_fence(raw.trim());
    let value: Value = serde_json::from_str(json_str).map_err(|e| {
        AppError::InternalError(format!("serde_json parse failed: {e}"))
    })?;

    let course: CourseExtraction = serde_json::from_value(value).map_err(|e| {
        AppError::InternalError(format!("schema deserialize failed: {e}"))
    })?;

    validate_course(&course)?;
    Ok(course)
}

fn validate_course(course: &CourseExtraction) -> AppResult<()> {
    if course.course_title.trim().is_empty() {
        return Err(AppError::InternalError(
            "course_title must not be empty".into(),
        ));
    }

    if !ALLOWED_CATEGORIES.contains(&course.category.as_str()) {
        return Err(AppError::InternalError(format!(
            "invalid category '{}'; expected one of {:?}",
            course.category, ALLOWED_CATEGORIES
        )));
    }

    if course.new_nodes.is_empty() {
        return Err(AppError::InternalError(
            "new_nodes must contain at least one node".into(),
        ));
    }

    if course.new_cards.is_empty() {
        return Err(AppError::InternalError(
            "new_cards must contain at least one card".into(),
        ));
    }

    let mut node_ids = HashSet::new();
    for node in &course.new_nodes {
        if !is_valid_uuid(&node.id) {
            return Err(AppError::InternalError(format!(
                "new_nodes[].id '{}' is not a valid UUID",
                node.id
            )));
        }
        if !ALLOWED_ENTITY_TYPES.contains(&node.entity_type.as_str()) {
            return Err(AppError::InternalError(format!(
                "invalid entity_type '{}'",
                node.entity_type
            )));
        }
        if node.title.trim().is_empty() {
            return Err(AppError::InternalError(
                "new_nodes[].title must not be empty".into(),
            ));
        }
        if !node_ids.insert(node.id.clone()) {
            return Err(AppError::InternalError(format!(
                "duplicate node id '{}'",
                node.id
            )));
        }
    }

    for edge in &course.new_edges {
        if edge.source_node_id == edge.target_node_id {
            return Err(AppError::InternalError(
                "edge source and target must differ".into(),
            ));
        }
        if edge.relationship_type.trim().is_empty() {
            return Err(AppError::InternalError(
                "relationship_type must not be empty".into(),
            ));
        }
        if !node_ids.contains(&edge.source_node_id) {
            return Err(AppError::InternalError(format!(
                "edge source '{}' not found in new_nodes",
                edge.source_node_id
            )));
        }
        if !node_ids.contains(&edge.target_node_id) {
            return Err(AppError::InternalError(format!(
                "edge target '{}' not found in new_nodes",
                edge.target_node_id
            )));
        }
    }

    for card in &course.new_cards {
        if !ALLOWED_CARD_TYPES.contains(&card.card_type.as_str()) {
            return Err(AppError::InternalError(format!(
                "invalid card_type '{}'",
                card.card_type
            )));
        }
        if card.question.trim().is_empty() || card.answer.trim().is_empty() {
            return Err(AppError::InternalError(
                "card question and answer must not be empty".into(),
            ));
        }
        if card.card_type == "payload_drill"
            && (!card.payload_mock.is_object() || card.payload_mock.as_object().is_some_and(|o| o.is_empty()))
        {
            return Err(AppError::InternalError(
                "payload_drill cards require a non-empty payload_mock object".into(),
            ));
        }
    }

    Ok(())
}

fn is_valid_uuid(raw: &str) -> bool {
    Uuid::parse_str(raw).is_ok()
}

pub async fn persist_course(pool: &SqlitePool, course: &CourseExtraction) -> AppResult<PersistedCourse> {
    let mut tx = pool.begin().await?;
    let mut card_ids = Vec::with_capacity(course.new_cards.len());
    let node_ids: Vec<String> = course.new_nodes.iter().map(|n| n.id.clone()).collect();

    for node in &course.new_nodes {
        sqlx::query(
            "INSERT INTO nodes (id, entity_type, title, description) VALUES (?, ?, ?, ?)",
        )
        .bind(&node.id)
        .bind(&node.entity_type)
        .bind(&node.title)
        .bind(&node.description)
        .execute(&mut *tx)
        .await?;
    }

    for edge in &course.new_edges {
        sqlx::query(
            "INSERT INTO edges (source_node_id, target_node_id, relationship_type)
             VALUES (?, ?, ?)",
        )
        .bind(&edge.source_node_id)
        .bind(&edge.target_node_id)
        .bind(&edge.relationship_type)
        .execute(&mut *tx)
        .await?;
    }

    let link_targets: Vec<&str> = {
        let vectors: Vec<&str> = course
            .new_nodes
            .iter()
            .filter(|n| n.entity_type == "vector")
            .map(|n| n.id.as_str())
            .collect();
        if !vectors.is_empty() {
            vectors
        } else {
            course.new_nodes.iter().map(|n| n.id.as_str()).collect()
        }
    };

    for card in &course.new_cards {
        let id = Uuid::new_v4().to_string();
        let payload = if card.card_type == "recall" {
            Value::Null
        } else {
            card.payload_mock.clone()
        };
        let data = json!({
            "question": card.question,
            "answer": card.answer,
            "payload": payload,
            "course_title": course.course_title,
        });
        let data_str = serde_json::to_string(&data)?;

        sqlx::query(
            "INSERT INTO cards (id, type, category, data) VALUES (?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&card.card_type)
        .bind(&course.category)
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

        for node_id in &link_targets {
            sqlx::query(
                "INSERT INTO card_node_links (card_id, node_id) VALUES (?, ?)",
            )
            .bind(&id)
            .bind(node_id)
            .execute(&mut *tx)
            .await?;
        }

        card_ids.push(id);
    }

    tx.commit().await?;

    Ok(PersistedCourse {
        course_title: course.course_title.clone(),
        category: course.category.clone(),
        node_ids,
        card_ids,
    })
}

fn strip_code_fence(text: &str) -> &str {
    let trimmed = text.trim();
    if !trimmed.starts_with("```") {
        return trimmed;
    }
    let without_open = trimmed.trim_start_matches('`').trim_start_matches("json");
    without_open
        .trim_start_matches('\n')
        .split("```")
        .next()
        .unwrap_or(without_open)
        .trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_course_json() -> &'static str {
        r#"{
  "course_title": "BIN Velocity Rotation",
  "category": "payments",
  "new_nodes": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "entity_type": "vector",
      "title": "CNP BIN Rotation",
      "description": "Fraudsters cycle card BINs to evade velocity rules."
    },
    {
      "id": "550e8400-e29b-41d4-a716-446655440001",
      "entity_type": "indicator",
      "title": "Cross-region BIN spike",
      "description": "Same BIN seen across multiple geos in minutes."
    }
  ],
  "new_edges": [
    {
      "source_node_id": "550e8400-e29b-41d4-a716-446655440001",
      "target_node_id": "550e8400-e29b-41d4-a716-446655440000",
      "relationship_type": "INDICATES"
    }
  ],
  "new_cards": [
    {
      "card_type": "payload_drill",
      "question": "Classify the attack given BIN telemetry.",
      "answer": "CNP BIN rotation fraud.",
      "payload_mock": {"bin": "411111", "regions": ["US", "DE"], "tx_count_10m": 42}
    },
    {
      "card_type": "recall",
      "question": "What is BIN velocity?",
      "answer": "Transaction rate per BIN over a window.",
      "payload_mock": {}
    }
  ]
}"#
    }

    #[test]
    fn parse_and_validate_accepts_rigid_schema() {
        let course = parse_and_validate(sample_course_json()).unwrap();
        assert_eq!(course.course_title, "BIN Velocity Rotation");
        assert_eq!(course.new_nodes.len(), 2);
        assert_eq!(course.new_cards.len(), 2);
    }

    #[test]
    fn parse_and_validate_rejects_bad_category() {
        let mut raw = sample_course_json().to_string();
        raw = raw.replace("\"payments\"", "\"crypto\"");
        let err = parse_and_validate(&raw).unwrap_err();
        assert!(err.to_string().contains("invalid category"));
    }

    #[test]
    fn parse_and_validate_rejects_dangling_edge() {
        let raw = r#"{
  "course_title": "Bad Edge",
  "category": "payments",
  "new_nodes": [{
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "entity_type": "vector",
    "title": "Only Node",
    "description": "desc"
  }],
  "new_edges": [{
    "source_node_id": "550e8400-e29b-41d4-a716-446655440000",
    "target_node_id": "00000000-0000-0000-0000-000000000099",
    "relationship_type": "INDICATES"
  }],
  "new_cards": [{
    "card_type": "recall",
    "question": "Q?",
    "answer": "A.",
    "payload_mock": {}
  }]
}"#;
        let err = parse_and_validate(raw).unwrap_err();
        assert!(err.to_string().contains("not found in new_nodes"));
    }

    #[test]
    fn parse_and_validate_strips_markdown_fence() {
        let fenced = format!("```json\n{}\n```", sample_course_json());
        assert!(parse_and_validate(&fenced).is_ok());
    }

    #[tokio::test]
    async fn persist_course_writes_graph_and_cards() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        let course = parse_and_validate(sample_course_json()).unwrap();
        let saved = persist_course(&pool, &course).await.unwrap();

        assert_eq!(saved.node_ids.len(), 2);
        assert_eq!(saved.card_ids.len(), 2);

        let node_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE id LIKE '550e8400%'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(node_count, 2);

        let card_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM cards WHERE category = 'payments'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(card_count >= 2);
    }
}
