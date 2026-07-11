//! Local inference client (Ollama / LM Studio on localhost).

use std::time::Duration;

use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::errors::{AppError, AppResult};

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_OLLAMA_PORT: u16 = 11434;
const DEFAULT_MODEL: &str = "llama3.2";

/// Supported local inference wire formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InferenceBackend {
    OllamaChat,
    OllamaGenerate,
}

/// Card types aligned with the `cards.type` column constraint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardType {
    PayloadDrill,
    LogicSandbox,
    Recall,
}

impl CardType {
    fn parse(raw: &str) -> AppResult<Self> {
        match raw {
            "payload_drill" => Ok(Self::PayloadDrill),
            "logic_sandbox" => Ok(Self::LogicSandbox),
            "recall" => Ok(Self::Recall),
            other => Err(AppError::InternalError(format!(
                "invalid card type from model: {other}"
            ))),
        }
    }
}

/// One flashcard object returned by the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneratedCard {
    #[serde(rename = "type")]
    pub card_type: CardType,
    pub data: Value,
}

/// Successful generation payload passed to the core parser / persistence layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardGenerationResult {
    pub cards: Vec<GeneratedCard>,
}

/// Structured failure from the AI pipeline (HTTP, stream, or parse stage).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiErrorEnvelope {
    pub stage: String,
    pub message: String,
}

impl AiErrorEnvelope {
    fn new(stage: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            stage: stage.into(),
            message: message.into(),
        }
    }
}

/// Outcome of a generation request — never panics; errors stay typed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AiGenerationOutcome {
    Ok(CardGenerationResult),
    Err(AiErrorEnvelope),
}

impl AiGenerationOutcome {
    pub fn into_result(self) -> Result<CardGenerationResult, AiErrorEnvelope> {
        match self {
            Self::Ok(value) => Ok(value),
            Self::Err(err) => Err(err),
        }
    }
}

/// Connection target for a local inference engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalInferenceConfig {
    pub host: String,
    pub port: u16,
    pub model: String,
    pub backend: InferenceBackend,
}

impl Default for LocalInferenceConfig {
    fn default() -> Self {
        Self {
            host: DEFAULT_HOST.into(),
            port: DEFAULT_OLLAMA_PORT,
            model: DEFAULT_MODEL.into(),
            backend: InferenceBackend::OllamaChat,
        }
    }
}

impl LocalInferenceConfig {
    pub fn base_url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }

    fn endpoint_path(&self) -> &'static str {
        match self.backend {
            InferenceBackend::OllamaChat => "/api/chat",
            InferenceBackend::OllamaGenerate => "/api/generate",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OllamaHealth {
    pub online: bool,
    pub model_count: usize,
    pub default_model: String,
}

/// Probe the local Ollama `/api/tags` endpoint (2s timeout).
pub async fn check_ollama_health(config: &LocalInferenceConfig) -> OllamaHealth {
    let offline = OllamaHealth {
        online: false,
        model_count: 0,
        default_model: config.model.clone(),
    };

    let client = match Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(c) => c,
        Err(_) => return offline,
    };

    let url = format!("{}/api/tags", config.base_url());
    let Ok(response) = client.get(&url).send().await else {
        return offline;
    };

    if !response.status().is_success() {
        return offline;
    }

    let Ok(body) = response.json::<Value>().await else {
        return offline;
    };

    let model_count = body
        .get("models")
        .and_then(|m| m.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    OllamaHealth {
        online: true,
        model_count,
        default_model: config.model.clone(),
    }
}

/// HTTP client for localhost inference APIs.
pub struct LocalAiClient {
    http: Client,
    config: LocalInferenceConfig,
}

// --- Ollama request / stream types ---

#[derive(Debug, Serialize)]
struct OllamaChatRequest<'a> {
    model: &'a str,
    messages: Vec<OllamaMessage<'a>>,
    stream: bool,
    format: &'static str,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Serialize)]
struct OllamaGenerateRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    stream: bool,
    format: &'static str,
}

#[derive(Debug, Deserialize)]
struct OllamaStreamLine {
    #[serde(default)]
    message: Option<OllamaStreamMessage>,
    #[serde(default)]
    response: String,
    #[serde(default)]
    done: bool,
}

#[derive(Debug, Deserialize)]
struct OllamaStreamMessage {
    #[serde(default)]
    content: String,
}

#[derive(Debug, Deserialize)]
struct LlmCardsEnvelope {
    cards: Vec<LlmCardDraft>,
}

#[derive(Debug, Deserialize)]
struct LlmCardDraft {
    #[serde(rename = "type")]
    card_type: String,
    data: Value,
}

impl LocalAiClient {
    pub fn new(config: LocalInferenceConfig) -> AppResult<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|e| AppError::InternalError(e.to_string()))?;
        Ok(Self { http, config })
    }

    pub fn config(&self) -> &LocalInferenceConfig {
        &self.config
    }

    /// Build the user prompt sent to the model (system rules + source document).
    pub fn build_flashcard_prompt(source_text: &str) -> String {
        format!(
            "Generate study flashcards from the source material below.\n\
             Output ONLY a JSON object with this exact shape:\n\
             {{\"cards\":[{{\"type\":\"recall|payload_drill|logic_sandbox\",\"data\":{{...}}}}]}}\n\
             Use recall for Q&A, payload_drill for fraud payload analysis, \
             logic_sandbox for rule/threshold reasoning drills.\n\n\
             --- SOURCE ---\n{source_text}\n--- END ---"
        )
    }

    /// Submit a prompt and collect a streamed JSON response into typed cards.
    pub async fn generate_cards(&self, source_text: &str) -> AiGenerationOutcome {
        match self.generate_cards_inner(source_text).await {
            Ok(cards) => AiGenerationOutcome::Ok(cards),
            Err(envelope) => AiGenerationOutcome::Err(envelope),
        }
    }

    async fn generate_cards_inner(&self, source_text: &str) -> Result<CardGenerationResult, AiErrorEnvelope> {
        let prompt = Self::build_flashcard_prompt(source_text);
        let raw = self
            .request_json_completion(COURSE_FLASHCARD_SYSTEM, &prompt)
            .await?;
        parse_cards_json(&raw).map_err(|e| AiErrorEnvelope::new("parse", e.to_string()))
    }

    /// Stream a JSON-formatted completion from Ollama (chat API, `format: json`).
    pub async fn request_json_completion(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<String, AiErrorEnvelope> {
        let url = format!("{}{}", self.config.base_url(), self.config.endpoint_path());

        let response = match self.config.backend {
            InferenceBackend::OllamaChat => {
                let body = OllamaChatRequest {
                    model: &self.config.model,
                    messages: vec![
                        OllamaMessage {
                            role: "system",
                            content: system_prompt,
                        },
                        OllamaMessage {
                            role: "user",
                            content: user_prompt,
                        },
                    ],
                    stream: true,
                    format: "json",
                };
                self.http.post(&url).json(&body).send().await
            }
            InferenceBackend::OllamaGenerate => {
                let combined = format!("{system_prompt}\n\n{user_prompt}");
                let body = build_generate_payload(&self.config.model, &combined);
                self.http.post(&url).json(&body).send().await
            }
        }
        .map_err(|e| AiErrorEnvelope::new("http", e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AiErrorEnvelope::new(
                "http",
                format!("inference engine returned {status}: {body}"),
            ));
        }

        collect_stream(response)
            .await
            .map_err(|e| AiErrorEnvelope::new("stream", e.to_string()))
    }
}

const COURSE_FLASHCARD_SYSTEM: &str =
    "You are a fraud and risk engineering study assistant. Respond with valid JSON only.";

fn build_chat_payload<'a>(model: &'a str, prompt: &'a str) -> OllamaChatRequest<'a> {
    OllamaChatRequest {
        model,
        messages: vec![
            OllamaMessage {
                role: "system",
                content: COURSE_FLASHCARD_SYSTEM,
            },
            OllamaMessage {
                role: "user",
                content: prompt,
            },
        ],
        stream: true,
        format: "json",
    }
}

fn build_generate_payload<'a>(model: &'a str, prompt: &'a str) -> OllamaGenerateRequest<'a> {
    OllamaGenerateRequest {
        model,
        prompt,
        stream: true,
        format: "json",
    }
}

/// Collect NDJSON stream chunks from Ollama into one assembled text buffer.
async fn collect_stream(response: reqwest::Response) -> AppResult<String> {
    let mut stream = response.bytes_stream();
    let mut line_buf = String::new();
    let mut assembled = String::new();

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| AppError::InternalError(e.to_string()))?;
        line_buf.push_str(&String::from_utf8_lossy(&bytes));

        while let Some(newline) = line_buf.find('\n') {
            let line: String = line_buf.drain(..=newline).collect();
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            append_stream_line(line, &mut assembled)?;
        }
    }

    let tail = line_buf.trim();
    if !tail.is_empty() {
        append_stream_line(tail, &mut assembled)?;
    }

    if assembled.is_empty() {
        return Err(AppError::InternalError(
            "inference stream returned no content".into(),
        ));
    }

    Ok(assembled)
}

fn append_stream_line(line: &str, assembled: &mut String) -> AppResult<()> {
    let event: OllamaStreamLine = serde_json::from_str(line).map_err(|e| {
        AppError::InternalError(format!("malformed stream line: {e}; line={line}"))
    })?;

    if let Some(message) = event.message {
        assembled.push_str(&message.content);
    } else {
        assembled.push_str(&event.response);
    }

    Ok(())
}

/// Parse the model's JSON text into validated card objects.
fn parse_cards_json(raw: &str) -> AppResult<CardGenerationResult> {
    let json_str = strip_code_fence(raw.trim());
    let envelope: LlmCardsEnvelope = serde_json::from_str(json_str)?;

    if envelope.cards.is_empty() {
        return Err(AppError::InternalError(
            "model returned zero cards".into(),
        ));
    }

    let cards = envelope
        .cards
        .into_iter()
        .map(|draft| {
            Ok(GeneratedCard {
                card_type: CardType::parse(&draft.card_type)?,
                data: draft.data,
            })
        })
        .collect::<AppResult<Vec<_>>>()?;

    Ok(CardGenerationResult { cards })
}

fn strip_code_fence(text: &str) -> &str {
    let trimmed = text.trim();
    trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|inner| inner.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_payload_enforces_json_format_and_streaming() {
        let body = build_chat_payload("llama3.2", "hello");
        let value = serde_json::to_value(&body).unwrap();
        assert_eq!(value["format"], "json");
        assert_eq!(value["stream"], true);
        assert_eq!(value["model"], "llama3.2");
    }

    #[test]
    fn collects_content_from_chat_stream_lines() {
        let mut out = String::new();
        append_stream_line(
            r#"{"message":{"content":"{\"cards\""},"done":false}"#,
            &mut out,
        )
        .unwrap();
        append_stream_line(
            r#"{"message":{"content":":[]}"},"done":true}"#,
            &mut out,
        )
        .unwrap();
        assert_eq!(out, r#"{"cards":[]}"#);
    }

    #[test]
    fn parses_generated_cards_from_json() {
        let raw = r#"{"cards":[{"type":"recall","data":{"front":"Q","back":"A"}}]}"#;
        let result = parse_cards_json(raw).unwrap();
        assert_eq!(result.cards.len(), 1);
        assert_eq!(result.cards[0].card_type, CardType::Recall);
        assert_eq!(result.cards[0].data["front"], "Q");
    }

    #[test]
    fn rejects_invalid_card_type() {
        let raw = r#"{"cards":[{"type":"invalid","data":{}}]}"#;
        let err = parse_cards_json(raw).unwrap_err();
        assert!(matches!(err, AppError::InternalError(_)));
    }

    #[test]
    fn strips_markdown_fence_before_parse() {
        let raw = "```json\n{\"cards\":[{\"type\":\"recall\",\"data\":{}}]}\n```";
        let result = parse_cards_json(raw).unwrap();
        assert_eq!(result.cards[0].card_type, CardType::Recall);
    }

    #[test]
    fn error_envelope_round_trips() {
        let outcome = AiGenerationOutcome::Err(AiErrorEnvelope::new("parse", "bad json"));
        let value = serde_json::to_value(&outcome).unwrap();
        assert_eq!(value["status"], "err");
        assert_eq!(value["message"], "bad json");
    }
}
