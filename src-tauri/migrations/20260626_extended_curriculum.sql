-- Extended curriculum: category + difficulty tier columns and advanced challenge tracks.

-- ---------------------------------------------------------------------------
-- Schema: structured categorization on cards
-- ---------------------------------------------------------------------------
ALTER TABLE cards ADD COLUMN category TEXT CHECK (
    category IS NULL
    OR category IN (
        'payments',
        'account_security',
        'trust_safety',
        'sql',
        'python',
        'r',
        'statistical_analysis'
    )
);

ALTER TABLE cards ADD COLUMN difficulty_tier TEXT CHECK (
    difficulty_tier IS NULL
    OR difficulty_tier IN ('junior', 'senior', 'architect')
);

CREATE INDEX idx_cards_category ON cards (category);
CREATE INDEX idx_cards_difficulty_tier ON cards (difficulty_tier);
CREATE INDEX idx_cards_category_type ON cards (category, type);
CREATE INDEX idx_cards_category_tier ON cards (category, difficulty_tier);

-- ---------------------------------------------------------------------------
-- Track 1: payments | payload_drill — CNP BIN velocity
-- ---------------------------------------------------------------------------
INSERT INTO cards (id, type, category, difficulty_tier, data) VALUES (
    'curriculum_payments_cnp_bin_velocity',
    'payload_drill',
    'payments',
    'senior',
    '{"question":"A Card-Not-Present checkout stream shows rapid BIN rotation while billing IP stays pinned to a single proxy ASN. Analyze the raw checkout JSON and classify the attack.","payload":{"checkout_id":"chk_cnp_9f31","channel":"CNP","merchant_mcc":"5732","window_minutes":12,"distinct_bins":4,"shared_billing_asn":"AS3320","events":[{"ts":"2026-06-26T14:02:11Z","bin":"544012","bin_country":"BR","billing_ip_country":"DE","amount_usd":189.0,"cvv_result":"M"},{"ts":"2026-06-26T14:04:33Z","bin":"520082","bin_country":"MX","billing_ip_country":"DE","amount_usd":412.5,"cvv_result":"M"},{"ts":"2026-06-26T14:08:47Z","bin":"539983","bin_country":"NG","billing_ip_country":"NL","amount_usd":275.99,"cvv_result":"M"},{"ts":"2026-06-26T14:11:02Z","bin":"411111","bin_country":"US","billing_ip_country":"NL","amount_usd":98.0,"cvv_result":"N"}]},"answer":"CNP BIN velocity with cross-border issuing vs billing mismatch. Attacker cycles issuer BIN countries to defeat static geo rules while holding billing IP in a datacenter ASN. Contain: block session, enforce 3DS step-up, velocity-cap distinct BINs per device_id, alert on ASN-BIN divergence score > threshold."}'
);

-- ---------------------------------------------------------------------------
-- Track 2: account_security | recall — ATO credential stuffing matrix
-- ---------------------------------------------------------------------------
INSERT INTO cards (id, type, category, difficulty_tier, data) VALUES (
    'curriculum_account_security_ato_matrix',
    'recall',
    'account_security',
    'senior',
    '{"question":"Deep-dive profile: an Account Takeover wave presents as a credential stuffing attack matrix. Which signal bundle confirms malicious stuffing versus a benign password-reset surge?","payload":{"attack_matrix":{"window_minutes":30,"total_login_attempts":8420,"unique_ips":2103,"unique_emails_targeted":1204,"success_rate_pct":0.8,"password_reset_requests":12,"avg_requests_per_ip":4.0,"top_user_agents":["python-requests/2.31","okhttp/4.12","curl/8.4"],"geo_entropy_score":0.91,"known_breach_list_hits":876}},"answer":"Malicious stuffing: high unique-email targeting, sub-1% success, bot UA concentration, high geo entropy, breach-list hit correlation, and flat password-reset volume. Benign surge: reset spike dominates, returning-device cookies, success on known residential ASNs, and support-ticket correlation."}'
);

-- ---------------------------------------------------------------------------
-- Track 3: sql | logic_sandbox — rolling-window device token self-join
-- ---------------------------------------------------------------------------
INSERT INTO cards (id, type, category, difficulty_tier, data) VALUES (
    'curriculum_sql_device_token_window',
    'logic_sandbox',
    'sql',
    'architect',
    '{"question":"Write a rolling-window self-join SQL query on auth_events that flags device_token values shared by more than 5 unique account_id rows within any 30-minute interval.","payload":{"table":"auth_events","columns":["event_ts","account_id","device_token"],"window_minutes":30,"threshold_distinct_accounts":5,"sample_rows":[{"event_ts":"2026-06-26T10:00:00Z","account_id":"acct_a1","device_token":"dtok_shared_9x"},{"event_ts":"2026-06-26T10:12:00Z","account_id":"acct_b2","device_token":"dtok_shared_9x"},{"event_ts":"2026-06-26T10:25:00Z","account_id":"acct_c3","device_token":"dtok_shared_9x"}]},"answer":"Self-join on device_token with e2.event_ts BETWEEN e1.event_ts AND e1.event_ts + 30 minutes; GROUP BY e1.device_token, e1.event_ts HAVING COUNT(DISTINCT e2.account_id) > 5. Prefer window functions at scale (RANGE INTERVAL or stream processor) to avoid O(n^2) explosion.","starter_sql":"-- WITH flagged AS (\n--   SELECT e1.device_token, e1.event_ts AS window_start,\n--          COUNT(DISTINCT e2.account_id) AS acct_cnt\n--   FROM auth_events e1\n--   JOIN auth_events e2\n--     ON e1.device_token = e2.device_token\n--    AND e2.event_ts BETWEEN e1.event_ts AND datetime(e1.event_ts, ''+30 minutes'')\n--   GROUP BY 1, 2\n--   HAVING acct_cnt > 5\n-- ) SELECT * FROM flagged;"}'
);

-- ---------------------------------------------------------------------------
-- Track 4: statistical_analysis | logic_sandbox — IQR outlier filter (Python)
-- ---------------------------------------------------------------------------
INSERT INTO cards (id, type, category, difficulty_tier, data) VALUES (
    'curriculum_stats_iqr_outlier_filter',
    'logic_sandbox',
    'statistical_analysis',
    'architect',
    '{"question":"Implement a Python function that applies an Interquartile Range (IQR) outlier filter to transaction amounts and returns only inlier values.","payload":{"amounts":[12.5,14.0,15.25,13.8,420.0,16.1,14.9,890.0,15.0,13.2],"multiplier":1.5,"language":"python"},"answer":"Sort amounts; compute Q1 and Q3 (linear interpolation or inclusive method per policy); IQR = Q3 - Q1; keep x where Q1 - k*IQR <= x <= Q3 + k*IQR. Document method for auditability. Flag 420.0 and 890.0 as outliers at k=1.5.","starter_python":"def iqr_filter(amounts: list[float], k: float = 1.5) -> list[float]:\n    \"\"\"Return inlier transaction amounts using Tukey IQR fences.\"\"\"\n    ..."}'
);

-- FSRS: all tracks due immediately for triage queue pickup
INSERT INTO fsrs_states (card_id, stability, difficulty, lapses, reviews, state, last_review, next_review) VALUES
('curriculum_payments_cnp_bin_velocity',      0.0, 0.0, 0, 0, 0, NULL, NULL),
('curriculum_account_security_ato_matrix',    0.0, 0.0, 0, 0, 0, NULL, NULL),
('curriculum_sql_device_token_window',      0.0, 0.0, 0, 0, 0, NULL, NULL),
('curriculum_stats_iqr_outlier_filter',     0.0, 0.0, 0, 0, 0, NULL, NULL);

-- Link curriculum tracks to existing topology nodes where applicable
INSERT INTO card_node_links (card_id, node_id) VALUES
('curriculum_payments_cnp_bin_velocity',   'ind_thin_file_bureau'),
('curriculum_account_security_ato_matrix', 'vec_ato_session_hijack'),
('curriculum_account_security_ato_matrix', 'ind_credential_stuffing_burst'),
('curriculum_sql_device_token_window',     'ind_emulator_build_props'),
('curriculum_stats_iqr_outlier_filter',    'ind_canvas_fp_mismatch');
