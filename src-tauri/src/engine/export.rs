//! Export study cards to CSV or Anki-compatible TSV.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Row, SqlitePool};

use crate::errors::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Csv,
    AnkiTsv,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportResult {
    pub filename: String,
    pub content: String,
}

fn card_front_back(card_type: &str, data: &Value) -> (String, String) {
    let question = data
        .get("question")
        .or_else(|| data.get("front"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let answer = data
        .get("answer")
        .or_else(|| data.get("back"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    if !question.is_empty() || !answer.is_empty() {
        return (question, answer);
    }

    match card_type {
        "payload_drill" => {
            let payload = data
                .get("payload_mock")
                .map(|v| v.to_string())
                .unwrap_or_else(|| "{}".into());
            (
                "Analyze the payload scenario.".into(),
                format!("Type: payload_drill\n{payload}"),
            )
        }
        "logic_sandbox" => (
            "Apply sandbox rule logic.".into(),
            data.to_string(),
        ),
        _ => (card_type.to_string(), data.to_string()),
    }
}

pub fn escape_csv_field(value: &str) -> String {
    if value.contains(['"', ',', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

pub fn format_csv_row(front: &str, back: &str, card_type: &str) -> String {
    format!(
        "{},{},{}",
        escape_csv_field(front),
        escape_csv_field(back),
        escape_csv_field(card_type)
    )
}

pub async fn export_study_cards(pool: &SqlitePool, format: ExportFormat) -> AppResult<ExportResult> {
    let rows = sqlx::query("SELECT type, data FROM cards ORDER BY created_at ASC")
        .fetch_all(pool)
        .await?;

    let mut lines: Vec<String> = match format {
        ExportFormat::Csv => vec!["Front,Back,Type".into()],
        ExportFormat::AnkiTsv => Vec::new(),
    };

    for row in rows {
        let card_type: String = row.get("type");
        let data_raw: String = row.get("data");
        let data: Value = serde_json::from_str(&data_raw).map_err(|e| {
            AppError::InternalError(format!("card data JSON corrupt for export: {e}"))
        })?;
        let (front, back) = card_front_back(&card_type, &data);

        match format {
            ExportFormat::Csv => {
                lines.push(format_csv_row(&front, &back, &card_type));
            }
            ExportFormat::AnkiTsv => {
                lines.push(format!("{front}\t{back}"));
            }
        }
    }

    let (filename, content) = match format {
        ExportFormat::Csv => ("mohawk-cards.csv".into(), lines.join("\n")),
        ExportFormat::AnkiTsv => ("mohawk-cards-anki.txt".into(), lines.join("\n")),
    };

    Ok(ExportResult { filename, content })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_csv_quotes_commas_and_newlines() {
        assert_eq!(escape_csv_field("plain"), "plain");
        assert_eq!(escape_csv_field("a,b"), "\"a,b\"");
        assert_eq!(escape_csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(escape_csv_field("line\nbreak"), "\"line\nbreak\"");
    }

    #[test]
    fn card_front_back_prefers_question_answer() {
        let data = serde_json::json!({"question": "Q?", "answer": "A."});
        let (f, b) = card_front_back("recall", &data);
        assert_eq!(f, "Q?");
        assert_eq!(b, "A.");
    }
}
