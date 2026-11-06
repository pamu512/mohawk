//! Threat Intel Desk ingest: pure feed parsing, HTML stripping, prose
//! chunking, feed-URL allowlist validation, and sync-schedule math.
//!
//! Network fetch, persistence, and LLM synthesis orchestration stay in the
//! app (`src-tauri/src/engine/scraper.rs`); this crate owns the pure path so
//! it is testable without HTTP, a database, or Ollama.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Crate-local error (mapped to AppError at the src-tauri boundary).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestError(pub String);

impl std::fmt::Display for IngestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for IngestError {}

/// Minimum days between automatic curriculum refresh cycles.
pub const SYNC_INTERVAL_DAYS: i64 = 14;

/// Target chunk size for LLM consumption (character-based proxy for tokens).
pub const DEFAULT_CHUNK_CHARS: usize = 2_000;

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

pub fn feed_urls() -> Vec<&'static str> {
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

pub fn source_label_for_url(url: &str) -> &str {
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

#[derive(Debug, Clone)]
pub struct FeedItem {
    pub title: String,
    pub body: String,
}

/// HTTPS-only allowlist guard for outbound feed fetches.
pub fn validate_feed_url(url: &str) -> Result<(), IngestError> {
    if !url.starts_with("https://") {
        return Err(IngestError(format!("feed URL must use HTTPS: {url}")));
    }
    if !INGESTION_SOURCE_DEFS.iter().any(|s| s.url == url) {
        return Err(IngestError(format!("feed URL not on allowlist: {url}")));
    }
    Ok(())
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

fn sync_elapsed_days(last: Option<DateTime<Utc>>, now: DateTime<Utc>) -> i64 {
    match last {
        None => SYNC_INTERVAL_DAYS,
        Some(ts) => now.signed_duration_since(ts).num_days().max(0),
    }
}

pub fn parse_rss_or_atom(xml: &str) -> Result<Vec<FeedItem>, String> {
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
