//! Background curriculum sync: fetch open security / fraud / regulatory feeds,
//! strip HTML boilerplate, chunk prose for downstream LLM ingestion.

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

/// Minimum days between automatic curriculum refresh cycles.
pub const SYNC_INTERVAL_DAYS: i64 = 14;

pub const CONFIG_KEY_LAST_SYNC: &str = "last_sync_timestamp";

/// Target chunk size for LLM consumption (character-based proxy for tokens).
pub const DEFAULT_CHUNK_CHARS: usize = 2_000;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_FEED_BYTES: usize = 2 * 1024 * 1024;
const USER_AGENT: &str = "Mohawk/0.1 (local-first fraud study; +https://github.com/mohawk)";

/// Known ingestion source metadata for the Threat Intel Desk dashboard.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngestionSource {
    pub id: String,
    pub label: String,
    pub url: String,
    pub desk_category: String,
}

struct IngestionSourceDef {
    id: &'static str,
    label: &'static str,
    url: &'static str,
    desk_category: &'static str,
}

const INGESTION_SOURCE_DEFS: &[IngestionSourceDef] = &[
    IngestionSourceDef {
        id: "cisa_advisories",
        label: "CISA Alert Feeds",
        url: "https://www.cisa.gov/cybersecurity-advisories/all.xml",
        desk_category: "regulatory",
    },
    IngestionSourceDef {
        id: "cfpb_briefings",
        label: "CFPB Newsroom",
        url: "https://www.consumerfinance.gov/about-us/newsroom/feed/",
        desk_category: "regulatory",
    },
    IngestionSourceDef {
        id: "krebs_security",
        label: "Security Logs",
        url: "https://krebsonsecurity.com/feed/",
        desk_category: "threat_intel",
    },
    IngestionSourceDef {
        id: "bleepingcomputer",
        label: "Security Logs",
        url: "https://www.bleepingcomputer.com/feed/",
        desk_category: "threat_intel",
    },
    IngestionSourceDef {
        id: "ftc_consumer",
        label: "Consumer Safety Briefings",
        url: "https://www.ftc.gov/feeds/press-release-consumer-safety/rss.xml",
        desk_category: "regulatory",
    },
];

fn feed_urls() -> Vec<&'static str> {
    INGESTION_SOURCE_DEFS.iter().map(|s| s.url).collect()
}

pub fn ingestion_sources() -> Vec<IngestionSource> {
    INGESTION_SOURCE_DEFS
        .iter()
        .map(|s| IngestionSource {
            id: s.id.to_string(),
            label: s.label.to_string(),
            url: s.url.to_string(),
            desk_category: s.desk_category.to_string(),
        })
        .collect()
}

fn source_label_for_url(url: &str) -> &str {
    INGESTION_SOURCE_DEFS
        .iter()
        .find(|s| s.url == url)
        .map(|s| s.label)
        .unwrap_or(url)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextChunk {
    pub source_url: String,
    pub title: String,
    pub chunk_index: usize,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncReport {
    pub synced_at: DateTime<Utc>,
    pub feeds_attempted: usize,
    pub feeds_succeeded: usize,
    pub items_extracted: usize,
    pub chunks_produced: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone)]
struct FeedItem {
    title: String,
    body: String,
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

pub fn next_sync_at(last: Option<DateTime<Utc>>, now: DateTime<Utc>) -> DateTime<Utc> {
    match last {
        None => now,
        Some(ts) => ts + chrono::Duration::days(SYNC_INTERVAL_DAYS),
    }
}

pub fn seconds_until_next_sync(last: Option<DateTime<Utc>>, now: DateTime<Utc>) -> i64 {
    if sync_is_due_now(last, now) {
        return 0;
    }
    let next = next_sync_at(last, now);
    (next - now).num_seconds().max(0)
}

pub fn sync_is_due_now(last: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    sync_elapsed_days(last, now) >= SYNC_INTERVAL_DAYS
}

async fn sync_is_due(pool: &SqlitePool) -> AppResult<bool> {
    let last = read_last_sync_timestamp(pool).await?;
    Ok(sync_is_due_now(last, Utc::now()))
}

fn sync_elapsed_days(last: Option<DateTime<Utc>>, now: DateTime<Utc>) -> i64 {
    match last {
        None => SYNC_INTERVAL_DAYS,
        Some(ts) => now.signed_duration_since(ts).num_days().max(0),
    }
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

/// HTTPS-only allowlist guard for outbound feed fetches.
pub fn validate_feed_url(url: &str) -> AppResult<()> {
    if !url.starts_with("https://") {
        return Err(AppError::InternalError(format!(
            "feed URL must use HTTPS: {url}"
        )));
    }
    if !INGESTION_SOURCE_DEFS.iter().any(|s| s.url == url) {
        return Err(AppError::InternalError(format!(
            "feed URL not on allowlist: {url}"
        )));
    }
    Ok(())
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

fn parse_rss_or_atom(xml: &str) -> Result<Vec<FeedItem>, String> {
    let lower = xml.to_lowercase();
    if lower.contains("<rss") || lower.contains("<rdf:rdf") {
        return parse_rss_items(xml);
    }
    if lower.contains("<feed") {
        return parse_atom_entries(xml);
    }
    Err("unsupported feed format (expected RSS or Atom)".into())
}

fn parse_rss_items(xml: &str) -> Result<Vec<FeedItem>, String> {
    let mut items = Vec::new();
    let lower = xml.to_lowercase();
    let mut search_from = 0usize;

    while let Some(start) = lower[search_from..].find("<item") {
        let abs_start = search_from + start;
        let Some(close) = lower[abs_start..].find("</item>") else {
            break;
        };
        let block = &xml[abs_start..abs_start + close + "</item>".len()];
        if let Some(item) = feed_item_from_block(block) {
            items.push(item);
        }
        search_from = abs_start + close + "</item>".len();
    }

    if items.is_empty() {
        return Err("no <item> elements found".into());
    }
    Ok(items)
}

fn parse_atom_entries(xml: &str) -> Result<Vec<FeedItem>, String> {
    let mut items = Vec::new();
    let lower = xml.to_lowercase();
    let mut search_from = 0usize;

    while let Some(start) = lower[search_from..].find("<entry") {
        let abs_start = search_from + start;
        let Some(close) = lower[abs_start..].find("</entry>") else {
            break;
        };
        let block = &xml[abs_start..abs_start + close + "</entry>".len()];
        if let Some(item) = feed_item_from_block(block) {
            items.push(item);
        }
        search_from = abs_start + close + "</entry>".len();
    }

    if items.is_empty() {
        return Err("no <entry> elements found".into());
    }
    Ok(items)
}

fn feed_item_from_block(block: &str) -> Option<FeedItem> {
    let title = extract_first_tag(block, "title")?;
    let body = extract_first_tag(block, "content:encoded")
        .or_else(|| extract_first_tag(block, "description"))
        .or_else(|| extract_first_tag(block, "summary"))
        .or_else(|| extract_first_tag(block, "content"))
        .unwrap_or_default();

    if title.trim().is_empty() && body.trim().is_empty() {
        return None;
    }

    Some(FeedItem {
        title: strip_html(&title),
        body,
    })
}

fn extract_first_tag(block: &str, tag: &str) -> Option<String> {
    let lower = block.to_lowercase();
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let start = lower.find(&open)?;
    let after_open = block[start..].find('>')? + start + 1;
    let end = lower[after_open..].find(&close)? + after_open;
    Some(block[after_open..end].to_string())
}

pub fn strip_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    let mut in_script = false;
    let lower = input.to_lowercase();
    let mut i = 0;
    let bytes = input.as_bytes();

    while i < bytes.len() {
        // ponytail: byte-walk for ASCII tags; skip mid-UTF-8 so nbsp/etc. don't panic
        if !lower.is_char_boundary(i) {
            i += 1;
            continue;
        }

        if in_script {
            if lower[i..].starts_with("</script>") {
                in_script = false;
                i += "</script>".len();
                continue;
            }
            i += lower[i..].chars().next().map_or(1, char::len_utf8);
            continue;
        }

        if !in_tag && lower[i..].starts_with("<script") {
            in_script = true;
            if let Some(gt) = lower[i..].find('>') {
                i += gt + 1;
            } else {
                i += 1;
            }
            continue;
        }

        match bytes[i] {
            b'<' => {
                in_tag = true;
                i += 1;
            }
            b'>' => {
                in_tag = false;
                out.push(' ');
                i += 1;
            }
            _ if !in_tag => {
                let ch = input[i..].chars().next().unwrap_or('\0');
                out.push(ch);
                i += ch.len_utf8();
            }
            _ => i += 1,
        }
    }

    normalize_whitespace(&decode_xml_entities(&out))
}

fn decode_xml_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

pub fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Split prose into semantic chunks on paragraph/sentence boundaries under `max_chars`.
pub fn chunk_text(prose: &str, max_chars: usize) -> Vec<String> {
    if prose.is_empty() {
        return Vec::new();
    }
    if prose.len() <= max_chars {
        return vec![prose.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();

    for paragraph in prose.split("\n\n") {
        let paragraph = paragraph.trim();
        if paragraph.is_empty() {
            continue;
        }

        if current.is_empty() {
            if paragraph.len() <= max_chars {
                current.push_str(paragraph);
            } else {
                chunks.extend(split_long_segment(paragraph, max_chars));
            }
            continue;
        }

        if current.len() + 2 + paragraph.len() <= max_chars {
            current.push_str("\n\n");
            current.push_str(paragraph);
        } else {
            chunks.push(current.clone());
            current.clear();
            if paragraph.len() <= max_chars {
                current.push_str(paragraph);
            } else {
                chunks.extend(split_long_segment(paragraph, max_chars));
            }
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

fn split_long_segment(text: &str, max_chars: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();

    for sentence in text.split(". ") {
        let piece = if sentence.ends_with('.') {
            sentence.to_string()
        } else {
            format!("{sentence}.")
        };

        if buf.is_empty() {
            if piece.len() <= max_chars {
                buf = piece;
            } else {
                for word in piece.split_whitespace() {
                    if buf.len() + word.len() + 1 > max_chars && !buf.is_empty() {
                        out.push(buf.trim().to_string());
                        buf.clear();
                    }
                    if !buf.is_empty() {
                        buf.push(' ');
                    }
                    buf.push_str(word);
                }
            }
            continue;
        }

        if buf.len() + 1 + piece.len() <= max_chars {
            buf.push(' ');
            buf.push_str(&piece);
        } else {
            out.push(buf.trim().to_string());
            buf = piece;
        }
    }

    if !buf.is_empty() {
        out.push(buf.trim().to_string());
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RSS: &str = r#"<?xml version="1.0"?>
<rss><channel>
<item><title>BIN Velocity Alert</title>
<description><p>Fraudsters rotate <b>CNP BINs</b> across regions.</p></description>
</item>
<item><title>ATO Wave</title>
<description>Credential stuffing against login endpoints.</description>
</item>
</channel></rss>"#;

    #[test]
    fn strip_html_removes_tags_and_scripts() {
        let raw = "<p>Hello <b>world</b></p><script>alert(1)</script><span>!</span>";
        assert_eq!(strip_html(raw), "Hello world !");
    }

    #[test]
    fn strip_html_handles_multibyte_chars() {
        // U+00A0 nbsp — previously panicked on byte-index string slices
        let raw = "catalog\u{a0}<b>item</b>";
        assert_eq!(strip_html(raw), "catalog item");
    }

    #[test]
    fn chunk_text_respects_max_size() {
        let prose = "Word ".repeat(500);
        let chunks = chunk_text(prose.trim(), 200);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|c| c.len() <= 250));
    }

    #[test]
    fn parse_rss_extracts_items() {
        let items = parse_rss_items(SAMPLE_RSS).unwrap();
        assert_eq!(items.len(), 2);
        assert!(items[0].title.contains("BIN Velocity"));
        assert!(strip_html(&items[0].body).contains("CNP BINs"));
    }

    #[test]
    fn validate_feed_url_rejects_http_and_unknown() {
        assert!(validate_feed_url("http://evil.com/feed").is_err());
        assert!(validate_feed_url("https://evil.com/feed").is_err());
        assert!(validate_feed_url(INGESTION_SOURCE_DEFS[0].url).is_ok());
    }

    #[test]
    fn sync_due_when_never_synced() {
        assert!(sync_elapsed_days(None, Utc::now()) >= SYNC_INTERVAL_DAYS);
    }

    #[test]
    fn sync_not_due_within_window() {
        let last = Utc::now() - chrono::Duration::days(3);
        assert!(sync_elapsed_days(Some(last), Utc::now()) < SYNC_INTERVAL_DAYS);
    }

    #[test]
    fn sync_due_after_fourteen_days() {
        let last = Utc::now() - chrono::Duration::days(14);
        assert!(sync_elapsed_days(Some(last), Utc::now()) >= SYNC_INTERVAL_DAYS);
    }
}
