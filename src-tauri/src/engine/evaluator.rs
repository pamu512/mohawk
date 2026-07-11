//! Sandbox evaluator: IF/THEN heuristic rules and multi-language challenge validation.
//!
//! Challenge path token-validates analyst submissions (SQL, Python, R, Stats) then
//! scores them against 500 pre-loaded mock fraud payloads for TP/FP telemetry.

use std::sync::{OnceLock, atomic::{AtomicU32, Ordering}};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::errors::{AppError, AppResult};

const MOCK_PAYLOAD_COUNT: usize = 500;

/// Language track for technical sandbox challenges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChallengeLanguage {
    #[serde(rename = "SQL")]
    Sql,
    #[serde(rename = "Python")]
    Python,
    #[serde(rename = "R")]
    R,
    #[serde(rename = "Stats")]
    Stats,
}

/// Unified sandbox telemetry returned to the frontend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SandboxResult {
    pub execution_time_us: u64,
    pub execution_time_ms: f64,
    pub total_evaluated: u32,
    pub true_positives: u32,
    pub false_positives: u32,
    pub false_negatives: u32,
    pub rules_triggered: u32,
    pub fraud_caught_percentage: f64,
    pub false_positive_percentage: f64,
    pub syntax_valid: bool,
    pub validation_errors: Vec<String>,
    pub challenge_language: Option<ChallengeLanguage>,
}

/// Legacy report alias — kept for internal IF/THEN path conversion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluationReport {
    pub execution_time_us: u64,
    pub execution_time_ms: f64,
    pub total_transactions: u32,
    pub rules_triggered: u32,
    pub true_positives: u32,
    pub false_positives: u32,
    pub false_negatives: u32,
    pub labeled_benign: u32,
    pub labeled_fraud: u32,
    pub false_positive_pct: f64,
    pub fraud_caught_pct: f64,
}

#[derive(Debug, Clone)]
struct ValidationOutcome {
    valid: bool,
    errors: Vec<String>,
    /// 0.0–1.0 strength score from matched algorithmic patterns.
    quality: f64,
}

static MOCK_PAYLOADS: OnceLock<Vec<String>> = OnceLock::new();

fn mock_payloads() -> &'static [String] {
    MOCK_PAYLOADS
        .get_or_init(build_mock_payloads)
        .as_slice()
}

fn build_mock_payloads() -> Vec<String> {
    (0..MOCK_PAYLOAD_COUNT)
        .map(|i| {
            let is_fraud = (i.wrapping_mul(17).wrapping_add(3)) % 10 < 6;
            let risk_score = if is_fraud {
                55.0 + (i % 45) as f64
            } else {
                5.0 + (i % 35) as f64
            };
            serde_json::json!({
                "id": format!("mock_tx_{i:04}"),
                "expected_fraud": is_fraud,
                "label": if is_fraud { "fraud" } else { "benign" },
                "risk_score": risk_score,
                "amount_usd": 8.0 + (i % 500) as f64 * 0.37,
                "device_token": format!("dt_{:02}", i % 48),
                "account_id": format!("acct_{:04}", i % 130),
                "velocity_1h": i % 18,
                "device_accounts_30m": (i % 8) + 1,
            })
            .to_string()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Challenge validation matrix
// ---------------------------------------------------------------------------

fn validate_submission(language: ChallengeLanguage, submission: &str) -> ValidationOutcome {
    match language {
        ChallengeLanguage::Sql => validate_sql(submission),
        ChallengeLanguage::Python => validate_python_or_stats(submission, ChallengeLanguage::Python),
        ChallengeLanguage::R => validate_r(submission),
        ChallengeLanguage::Stats => validate_python_or_stats(submission, ChallengeLanguage::Stats),
    }
}

fn validate_sql(submission: &str) -> ValidationOutcome {
    let upper = submission.to_uppercase();
    let mut errors = Vec::new();
    let mut matched = 0u32;
    const REQUIRED: &[&str] = &["WINDOW", "PARTITION BY", "COUNT", "GROUP BY"];
    const TOTAL: u32 = REQUIRED.len() as u32 + 1; // + self-join

    for kw in REQUIRED {
        if upper.contains(kw) {
            matched += 1;
        } else {
            errors.push(format!("missing required SQL keyword: {kw}"));
        }
    }

    let join_tokens = upper.matches(" JOIN ").count()
        + usize::from(upper.contains("SELF JOIN"))
        + usize::from(upper.starts_with("JOIN "));
    let has_self_join = upper.contains("SELF JOIN")
        || join_tokens >= 2
        || (upper.contains(" JOIN ") && count_table_aliases(&upper) >= 2);

    if has_self_join {
        matched += 1;
    } else {
        errors.push("missing self-join: expect JOIN (>=2) or SELF JOIN".into());
    }

    let quality = matched as f64 / TOTAL as f64;
    ValidationOutcome {
        valid: errors.is_empty(),
        errors,
        quality,
    }
}

fn count_table_aliases(upper: &str) -> usize {
    upper.matches(" AS ").count() + upper.matches(" JOIN ").count()
}

fn validate_python_or_stats(submission: &str, lang: ChallengeLanguage) -> ValidationOutcome {
    let lower = submission.to_lowercase();
    let mut errors = Vec::new();
    let mut matched = 0u32;

    let outlier_markers: &[(&str, u32)] = &[
        (".quantile(", 1),
        ("quantile(", 1),
        (".std(", 1),
        ("std(", 1),
        ("iqr", 1),
        ("interquartile", 1),
        ("np.percentile", 1),
        ("percentile(", 1),
    ];

    for (marker, weight) in outlier_markers {
        if lower.contains(marker) {
            matched += weight;
        }
    }

    if lower.contains("q1") && lower.contains("q3") {
        matched += 1;
    }

    if matched == 0 {
        errors.push(
            "missing outlier logic: expected .quantile(), std(), IQR, or manual Q1/Q3 bounds"
                .into(),
        );
    }

    if lang == ChallengeLanguage::Python && !lower.contains("def ") && !lower.contains("lambda") {
        errors.push("Python submission should define a function or lambda".into());
    }

    let quality = (matched as f64 / 3.0).min(1.0);
    ValidationOutcome {
        valid: errors.is_empty(),
        errors,
        quality,
    }
}

fn validate_r(submission: &str) -> ValidationOutcome {
    let lower = submission.to_lowercase();
    let mut errors = Vec::new();
    let mut matched = 0u32;

    for marker in ["quantile(", "iqr(", "iqr ", "sd(", "stats::iqr"] {
        if lower.contains(marker) {
            matched += 1;
        }
    }
    if lower.contains("q1") && lower.contains("q3") {
        matched += 1;
    }

    if matched == 0 {
        errors.push("missing R outlier logic: quantile(), sd(), or IQR".into());
    }

    let quality = (matched as f64 / 2.0).min(1.0);
    ValidationOutcome {
        valid: errors.is_empty(),
        errors,
        quality,
    }
}

// ---------------------------------------------------------------------------
// Challenge scoring against mock payloads
// ---------------------------------------------------------------------------

/// Validate a technical submission and score it against 500 mock fraud payloads.
pub fn evaluate_challenge(
    submission: &str,
    language: ChallengeLanguage,
) -> AppResult<SandboxResult> {
    evaluate_challenge_against(submission, language, mock_payloads())
}

pub fn evaluate_challenge_against(
    submission: &str,
    language: ChallengeLanguage,
    payloads: &[String],
) -> AppResult<SandboxResult> {
    let start = Instant::now();
    let validation = validate_submission(language, submission);

    if payloads.is_empty() {
        return Ok(empty_sandbox_result(
            start.elapsed(),
            language,
            validation.valid,
            validation.errors,
        ));
    }

    if !validation.valid {
        return Ok(SandboxResult {
            execution_time_us: start.elapsed().as_micros() as u64,
            execution_time_ms: start.elapsed().as_secs_f64() * 1000.0,
            total_evaluated: payloads.len() as u32,
            true_positives: 0,
            false_positives: 0,
            false_negatives: payloads
                .iter()
                .filter(|raw| payload_is_fraud(raw))
                .count() as u32,
            rules_triggered: 0,
            fraud_caught_percentage: 0.0,
            false_positive_percentage: 0.0,
            syntax_valid: false,
            validation_errors: validation.errors,
            challenge_language: Some(language),
        });
    }

    let tp = AtomicU32::new(0);
    let fp = AtomicU32::new(0);
    let fn_count = AtomicU32::new(0);
    let triggered = AtomicU32::new(0);

    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(payloads.len());
    let chunk_size = payloads.len().div_ceil(workers);
    let quality = validation.quality;

    std::thread::scope(|scope| {
        let handles: Vec<_> = payloads
            .chunks(chunk_size)
            .map(|chunk| {
                let chunk = chunk.to_vec();
                scope.spawn(move || {
                    let mut local_tp = 0u32;
                    let mut local_fp = 0u32;
                    let mut local_fn = 0u32;
                    let mut local_trig = 0u32;

                    for raw in chunk {
                        let Ok(txn) = serde_json::from_str::<Value>(&raw) else {
                            continue;
                        };
                        let is_fraud = ground_truth(&txn) == Some(GroundTruth::Fraud);
                        let flagged = simulate_challenge_flag(language, quality, &txn);

                        if flagged {
                            local_trig += 1;
                            if is_fraud {
                                local_tp += 1;
                            } else {
                                local_fp += 1;
                            }
                        } else if is_fraud {
                            local_fn += 1;
                        }
                    }

                    (local_tp, local_fp, local_fn, local_trig)
                })
            })
            .collect();

        for handle in handles {
            let (t, f, n, tr) = handle.join().expect("challenge worker panicked");
            tp.fetch_add(t, Ordering::Relaxed);
            fp.fetch_add(f, Ordering::Relaxed);
            fn_count.fetch_add(n, Ordering::Relaxed);
            triggered.fetch_add(tr, Ordering::Relaxed);
        }
    });

    let elapsed = start.elapsed();
    let total = payloads.len() as u32;
    let tp_v = tp.load(Ordering::Relaxed);
    let fp_v = fp.load(Ordering::Relaxed);
    let fn_v = fn_count.load(Ordering::Relaxed);
    let fraud_total = tp_v + fn_v;

    Ok(SandboxResult {
        execution_time_us: elapsed.as_micros() as u64,
        execution_time_ms: elapsed.as_secs_f64() * 1000.0,
        total_evaluated: total,
        true_positives: tp_v,
        false_positives: fp_v,
        false_negatives: fn_v,
        rules_triggered: triggered.load(Ordering::Relaxed),
        fraud_caught_percentage: pct(tp_v, fraud_total),
        false_positive_percentage: pct(fp_v, total.saturating_sub(fraud_total)),
        syntax_valid: true,
        validation_errors: validation.errors,
        challenge_language: Some(language),
    })
}

fn simulate_challenge_flag(
    language: ChallengeLanguage,
    quality: f64,
    txn: &Value,
) -> bool {
    let risk = txn
        .get("risk_score")
        .and_then(value_as_f64)
        .unwrap_or(0.0);
    let device_spread = txn
        .get("device_accounts_30m")
        .and_then(|v| v.as_u64())
        .unwrap_or(1) as f64;

    let lang_boost = match language {
        ChallengeLanguage::Sql => 1.0,
        ChallengeLanguage::Python => 0.98,
        ChallengeLanguage::R => 0.96,
        ChallengeLanguage::Stats => 1.02,
    };

    let score = (risk + device_spread * 4.0) * quality * lang_boost;
    score >= 45.0
}

fn payload_is_fraud(raw: &str) -> bool {
    serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|v| ground_truth(&v))
        == Some(GroundTruth::Fraud)
}

fn pct(numer: u32, denom: u32) -> f64 {
    if denom == 0 {
        0.0
    } else {
        (numer as f64 / denom as f64) * 100.0
    }
}

fn empty_sandbox_result(
    elapsed: std::time::Duration,
    language: ChallengeLanguage,
    syntax_valid: bool,
    validation_errors: Vec<String>,
) -> SandboxResult {
    SandboxResult {
        execution_time_us: elapsed.as_micros() as u64,
        execution_time_ms: elapsed.as_secs_f64() * 1000.0,
        total_evaluated: 0,
        true_positives: 0,
        false_positives: 0,
        false_negatives: 0,
        rules_triggered: 0,
        fraud_caught_percentage: 0.0,
        false_positive_percentage: 0.0,
        syntax_valid,
        validation_errors,
        challenge_language: Some(language),
    }
}

impl From<EvaluationReport> for SandboxResult {
    fn from(r: EvaluationReport) -> Self {
        Self {
            execution_time_us: r.execution_time_us,
            execution_time_ms: r.execution_time_ms,
            total_evaluated: r.total_transactions,
            true_positives: r.true_positives,
            false_positives: r.false_positives,
            false_negatives: r.false_negatives,
            rules_triggered: r.rules_triggered,
            fraud_caught_percentage: r.fraud_caught_pct,
            false_positive_percentage: r.false_positive_pct,
            syntax_valid: true,
            validation_errors: Vec::new(),
            challenge_language: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Legacy IF ... THEN heuristic rule engine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompareOp {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
    Ne,
}

impl CompareOp {
    fn parse(token: &str) -> AppResult<Self> {
        match token {
            ">" => Ok(Self::Gt),
            ">=" => Ok(Self::Gte),
            "<" => Ok(Self::Lt),
            "<=" => Ok(Self::Lte),
            "==" => Ok(Self::Eq),
            "!=" => Ok(Self::Ne),
            other => Err(AppError::InternalError(format!("unknown operator: {other}"))),
        }
    }

    fn eval(&self, left: &Value, right: &Value) -> bool {
        match self {
            Self::Eq => json_values_equal(left, right),
            Self::Ne => !json_values_equal(left, right),
            _ => {
                let Some(l) = value_as_f64(left) else {
                    return false;
                };
                let Some(r) = value_as_f64(right) else {
                    return false;
                };
                match self {
                    Self::Gt => l > r,
                    Self::Gte => l >= r,
                    Self::Lt => l < r,
                    Self::Lte => l <= r,
                    _ => false,
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Condition {
    field: String,
    op: CompareOp,
    value: Value,
}

#[derive(Debug, Clone, PartialEq)]
struct Rule {
    conditions: Vec<Condition>,
    assign_field: String,
    assign_value: Value,
}

/// Parse and evaluate `rule_definition` against each transaction JSON string.
pub fn evaluate_transactions(
    transactions: &[String],
    rule_definition: &str,
) -> AppResult<EvaluationReport> {
    let rule = parse_rule(rule_definition)?;
    let start = Instant::now();
    let total = transactions.len() as u32;

    let triggered = AtomicU32::new(0);
    let true_positives = AtomicU32::new(0);
    let false_positives = AtomicU32::new(0);
    let false_negatives = AtomicU32::new(0);
    let labeled_benign = AtomicU32::new(0);
    let labeled_fraud = AtomicU32::new(0);

    if transactions.is_empty() {
        return Ok(empty_evaluation_report(start.elapsed()));
    }

    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(transactions.len());
    let chunk_size = transactions.len().div_ceil(workers);

    std::thread::scope(|scope| {
        let handles: Vec<_> = transactions
            .chunks(chunk_size)
            .map(|chunk| {
                let chunk = chunk.to_vec();
                let rule = rule.clone();
                scope.spawn(move || {
                    let mut local_triggered = 0u32;
                    let mut local_tp = 0u32;
                    let mut local_fp = 0u32;
                    let mut local_fn = 0u32;
                    let mut local_benign = 0u32;
                    let mut local_fraud = 0u32;

                    for raw in chunk {
                        let Ok(txn) = serde_json::from_str::<Value>(&raw) else {
                            continue;
                        };

                        match ground_truth(&txn) {
                            Some(GroundTruth::Benign) => local_benign += 1,
                            Some(GroundTruth::Fraud) => local_fraud += 1,
                            None => {}
                        }

                        if rule_matches(&txn, &rule) {
                            local_triggered += 1;
                            match ground_truth(&txn) {
                                Some(GroundTruth::Fraud) => local_tp += 1,
                                Some(GroundTruth::Benign) => local_fp += 1,
                                None => {}
                            }
                        } else if matches!(ground_truth(&txn), Some(GroundTruth::Fraud)) {
                            local_fn += 1;
                        }
                    }

                    (
                        local_triggered,
                        local_tp,
                        local_fp,
                        local_fn,
                        local_benign,
                        local_fraud,
                    )
                })
            })
            .collect();

        for handle in handles {
            let (t, tp, fp, fn_v, benign, fraud) =
                handle.join().expect("evaluator worker panicked");
            triggered.fetch_add(t, Ordering::Relaxed);
            true_positives.fetch_add(tp, Ordering::Relaxed);
            false_positives.fetch_add(fp, Ordering::Relaxed);
            false_negatives.fetch_add(fn_v, Ordering::Relaxed);
            labeled_benign.fetch_add(benign, Ordering::Relaxed);
            labeled_fraud.fetch_add(fraud, Ordering::Relaxed);
        }
    });

    let elapsed = start.elapsed();
    let rules_triggered = triggered.load(Ordering::Relaxed);
    let tp = true_positives.load(Ordering::Relaxed);
    let fp = false_positives.load(Ordering::Relaxed);
    let fn_v = false_negatives.load(Ordering::Relaxed);
    let benign = labeled_benign.load(Ordering::Relaxed);
    let fraud = labeled_fraud.load(Ordering::Relaxed);

    Ok(EvaluationReport {
        execution_time_us: elapsed.as_micros() as u64,
        execution_time_ms: elapsed.as_secs_f64() * 1000.0,
        total_transactions: total,
        rules_triggered,
        true_positives: tp,
        false_positives: fp,
        false_negatives: fn_v,
        labeled_benign: benign,
        labeled_fraud: fraud,
        false_positive_pct: pct(fp, benign),
        fraud_caught_pct: pct(tp, fraud),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GroundTruth {
    Fraud,
    Benign,
}

fn ground_truth(txn: &Value) -> Option<GroundTruth> {
    if let Some(flag) = txn.get("expected_fraud").and_then(Value::as_bool) {
        return Some(if flag {
            GroundTruth::Fraud
        } else {
            GroundTruth::Benign
        });
    }

    txn.get("label")
        .and_then(Value::as_str)
        .map(|label| {
            if label.eq_ignore_ascii_case("fraud") {
                GroundTruth::Fraud
            } else {
                GroundTruth::Benign
            }
        })
}

fn rule_matches(txn: &Value, rule: &Rule) -> bool {
    rule.conditions
        .iter()
        .all(|cond| condition_matches(txn, cond))
}

fn condition_matches(txn: &Value, cond: &Condition) -> bool {
    let Some(field_value) = txn.get(&cond.field) else {
        return false;
    };
    cond.op.eval(field_value, &cond.value)
}

fn parse_rule(input: &str) -> AppResult<Rule> {
    let normalized = collapse_spaces(input.trim());
    let upper = normalized.to_uppercase();

    let then_idx = upper
        .find(" THEN ")
        .ok_or_else(|| AppError::InternalError("rule must contain THEN clause".into()))?;
    let (cond_section, action_section) = normalized.split_at(then_idx);
    let action_section = action_section[" THEN ".len()..].trim();

    let cond_body = cond_section
        .strip_prefix("IF ")
        .or_else(|| cond_section.strip_prefix("if "))
        .ok_or_else(|| AppError::InternalError("rule must start with IF".into()))?;

    let conditions = cond_body
        .split(" AND ")
        .map(parse_condition)
        .collect::<AppResult<Vec<_>>>()?;

    if conditions.is_empty() {
        return Err(AppError::InternalError(
            "rule must include at least one condition".into(),
        ));
    }

    let (assign_field, assign_value) = parse_assignment(action_section)?;

    Ok(Rule {
        conditions,
        assign_field,
        assign_value,
    })
}

fn parse_condition(segment: &str) -> AppResult<Condition> {
    let segment = segment.trim();
    for op_token in [">=", "<=", "!=", "==", ">", "<"] {
        if let Some(idx) = segment.find(op_token) {
            let field = segment[..idx].trim().to_string();
            let value_raw = segment[idx + op_token.len()..].trim();
            if field.is_empty() {
                return Err(AppError::InternalError(format!(
                    "missing field in condition: {segment}"
                )));
            }
            return Ok(Condition {
                field,
                op: CompareOp::parse(op_token)?,
                value: parse_literal(value_raw)?,
            });
        }
    }
    Err(AppError::InternalError(format!(
        "could not parse condition: {segment}"
    )))
}

fn parse_assignment(segment: &str) -> AppResult<(String, Value)> {
    let Some((field, value_raw)) = segment.split_once('=') else {
        return Err(AppError::InternalError(format!(
            "THEN clause must assign with '=': {segment}"
        )));
    };
    let field = field.trim().to_string();
    if field.is_empty() {
        return Err(AppError::InternalError(
            "THEN assignment missing field name".into(),
        ));
    }
    Ok((field, parse_literal(value_raw.trim())?))
}

fn parse_literal(raw: &str) -> AppResult<Value> {
    let raw = raw.trim();
    if raw.eq_ignore_ascii_case("true") {
        return Ok(Value::Bool(true));
    }
    if raw.eq_ignore_ascii_case("false") {
        return Ok(Value::Bool(false));
    }
    if (raw.starts_with('"') && raw.ends_with('"'))
        || (raw.starts_with('\'') && raw.ends_with('\''))
    {
        return Ok(Value::String(raw[1..raw.len() - 1].to_string()));
    }
    if let Ok(int) = raw.parse::<i64>() {
        return Ok(Value::Number(int.into()));
    }
    if let Ok(float) = raw.parse::<f64>() {
        return Ok(
            serde_json::Number::from_f64(float)
                .map(Value::Number)
                .ok_or_else(|| AppError::InternalError(format!("invalid number: {raw}")))?,
        );
    }
    Err(AppError::InternalError(format!("invalid literal: {raw}")))
}

fn collapse_spaces(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn json_values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Bool(l), Value::Bool(r)) => l == r,
        (Value::String(l), Value::String(r)) => l == r,
        (Value::Number(l), Value::Number(r)) => {
            value_as_f64(&Value::Number(l.clone())) == value_as_f64(&Value::Number(r.clone()))
        }
        (Value::Null, Value::Null) => true,
        _ => left == right,
    }
}

fn value_as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64().or_else(|| n.as_i64().map(|i| i as f64)),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

fn empty_evaluation_report(elapsed: std::time::Duration) -> EvaluationReport {
    EvaluationReport {
        execution_time_us: elapsed.as_micros() as u64,
        execution_time_ms: elapsed.as_secs_f64() * 1000.0,
        total_transactions: 0,
        rules_triggered: 0,
        true_positives: 0,
        false_positives: 0,
        false_negatives: 0,
        labeled_benign: 0,
        labeled_fraud: 0,
        false_positive_pct: 0.0,
        fraud_caught_pct: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_SQL: &str = r#"
        SELECT e1.device_token, COUNT(DISTINCT e2.account_id) AS acct_cnt
        FROM auth_events e1
        JOIN auth_events e2 ON e1.device_token = e2.device_token
        WINDOW w AS (PARTITION BY e1.device_token ORDER BY e1.event_ts)
        GROUP BY e1.device_token
    "#;

    const INVALID_SQL: &str = "SELECT * FROM auth_events WHERE velocity > 5";

    const VALID_PYTHON: &str = r#"
        def iqr_filter(amounts):
            q1 = np.quantile(amounts, 0.25)
            q3 = np.quantile(amounts, 0.75)
            iqr = q3 - q1
            return [x for x in amounts if q1 - 1.5 * iqr <= x <= q3 + 1.5 * iqr]
    "#;

    const INVALID_PYTHON: &str = "def filter(xs): return [x for x in xs if x > 0]";

    #[test]
    fn sql_validation_passes_with_required_keywords_and_self_join() {
        let v = validate_sql(VALID_SQL);
        assert!(v.valid, "{:?}", v.errors);
        assert!(v.quality > 0.9);
    }

    #[test]
    fn sql_validation_fails_without_group_by_and_join() {
        let v = validate_sql(INVALID_SQL);
        assert!(!v.valid);
        assert!(v.errors.iter().any(|e| e.contains("GROUP BY")));
    }

    #[test]
    fn python_validation_passes_with_quantile_iqr() {
        let v = validate_python_or_stats(VALID_PYTHON, ChallengeLanguage::Python);
        assert!(v.valid, "{:?}", v.errors);
    }

    #[test]
    fn python_validation_fails_without_outlier_logic() {
        let v = validate_python_or_stats(INVALID_PYTHON, ChallengeLanguage::Python);
        assert!(!v.valid);
    }

    #[test]
    fn challenge_eval_returns_zero_flags_on_invalid_submission() {
        let result =
            evaluate_challenge(INVALID_SQL, ChallengeLanguage::Sql).expect("eval should not error");
        assert!(!result.syntax_valid);
        assert_eq!(result.rules_triggered, 0);
        assert_eq!(result.true_positives, 0);
        assert_eq!(result.total_evaluated, MOCK_PAYLOAD_COUNT as u32);
    }

    #[test]
    fn challenge_eval_scores_valid_sql_against_mock_payloads() {
        let result =
            evaluate_challenge(VALID_SQL, ChallengeLanguage::Sql).expect("eval should not error");
        assert!(result.syntax_valid);
        assert!(result.true_positives > 0);
        assert!(result.fraud_caught_percentage > 0.0);
        assert!(result.execution_time_ms >= 0.0);
        assert_eq!(result.total_evaluated, MOCK_PAYLOAD_COUNT as u32);
    }

    #[test]
    fn challenge_eval_scores_valid_python() {
        let result = evaluate_challenge(VALID_PYTHON, ChallengeLanguage::Python)
            .expect("eval should not error");
        assert!(result.syntax_valid);
        assert!(result.true_positives > result.false_positives);
    }

    #[test]
    fn legacy_if_then_still_computes_tp_and_fp() {
        let txns = vec![
            r#"{"velocity":200,"fingerprint_matches":false,"expected_fraud":false}"#.into(),
            r#"{"velocity":10,"fingerprint_matches":true,"expected_fraud":false}"#.into(),
            r#"{"velocity":300,"fingerprint_matches":false,"expected_fraud":true}"#.into(),
        ];
        let report = evaluate_transactions(
            &txns,
            "IF velocity > 100 AND fingerprint_matches == false THEN score = 85",
        )
        .unwrap();

        assert_eq!(report.total_transactions, 3);
        assert_eq!(report.rules_triggered, 2);
        assert_eq!(report.true_positives, 1);
        assert_eq!(report.false_positives, 1);
        assert!((report.false_positive_pct - 50.0).abs() < f64::EPSILON);

        let sandbox: SandboxResult = report.into();
        assert_eq!(sandbox.true_positives, 1);
        assert_eq!(sandbox.false_positives, 1);
    }

    #[test]
    fn mock_payload_count_is_five_hundred() {
        assert_eq!(mock_payloads().len(), MOCK_PAYLOAD_COUNT);
    }
}
