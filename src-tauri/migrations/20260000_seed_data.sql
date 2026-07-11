-- Seed: fraud attack topology, compliance anchors, and due FSRS triage cards.
-- Safe to run once on empty DB; migration runner tracks applied versions.

-- ---------------------------------------------------------------------------
-- Attack vectors
-- ---------------------------------------------------------------------------
INSERT INTO nodes (id, entity_type, title, description) VALUES
(
    'vec_ato_session_hijack',
    'vector',
    'Account Takeover via Session Hijacking',
    'Adversary replays or steals authenticated session tokens (cookie, JWT, OAuth refresh) to operate as the victim without re-authentication.'
),
(
    'vec_synthetic_identity_burst',
    'vector',
    'Synthetic Identity Burst Fraud',
    'Fabricated or stitched identities (SSN + thin credit file + mule address) used in coordinated onboarding bursts before bust-out.'
),
(
    'vec_device_fp_spoof',
    'vector',
    'Device Fingerprint Spoofing Emulators',
    'Mobile emulators and anti-detect browsers forge hardware/software signals to evade device reputation and velocity controls.'
);

-- ---------------------------------------------------------------------------
-- Indicator signals
-- ---------------------------------------------------------------------------
INSERT INTO nodes (id, entity_type, title, description) VALUES
(
    'ind_rapid_ip_asn',
    'indicator',
    'Rapid IP ASN shifting',
    'Same session or account hops across unrelated autonomous systems within minutes, inconsistent with residential ISP stickiness.'
),
(
    'ind_impossible_travel',
    'indicator',
    'Impossible travel velocity',
    'Geo-IP distance divided by elapsed time between consecutive events exceeds plausible human travel speed.'
),
(
    'ind_credential_stuffing_burst',
    'indicator',
    'Credential stuffing login burst',
    'High failed-login ratio from distributed IPs targeting one identity with password-spray or combo-list patterns.'
),
(
    'ind_thin_file_bureau',
    'indicator',
    'High-velocity credit bureau thin-file hits',
    'Multiple bureau pulls on a newly issued SSN with minimal tradeline depth inside a 72-hour window.'
),
(
    'ind_ssn_issuance_mismatch',
    'indicator',
    'SSN issuance date mismatch',
    'Declared DOB and SSN issuance cohort diverge beyond SSA randomization tolerance bands.'
),
(
    'ind_address_velocity',
    'indicator',
    'Address change velocity anomaly',
    'Billing or KYC address updated multiple times across high-risk postal clusters before first settlement.'
),
(
    'ind_canvas_fp_mismatch',
    'indicator',
    'Mismatched Canvas Fingerprint telemetry',
    'Canvas hash rotates between requests while user-agent and screen dimensions remain static—typical of spoofing hooks.'
),
(
    'ind_webgl_renderer_drift',
    'indicator',
    'WebGL renderer inconsistency',
    'Reported GPU renderer string conflicts with platform UA (e.g., SwiftShader on claimed iOS Safari).'
),
(
    'ind_emulator_build_props',
    'indicator',
    'Android emulator build props',
    'System properties expose generic SDK images (goldfish, ranchu, generic_x86) on a purported retail device.'
);

-- ---------------------------------------------------------------------------
-- Compliance / legal anchors
-- ---------------------------------------------------------------------------
INSERT INTO nodes (id, entity_type, title, description) VALUES
(
    'legal_psd3_sca',
    'legal',
    'PSD3 Strong Customer Authentication',
    'EU payment services directive requiring SCA for remote electronic payments and account access, with RTS exemption catalog.'
),
(
    'legal_aml_kyc_tier1',
    'legal',
    'AML Tier 1 KYC verification rules',
    'CDD baseline: verified name, DOB, address, and government ID match before high-risk product enablement.'
),
(
    'legal_bsa_sar_threshold',
    'legal',
    'BSA Suspicious Activity Reporting threshold',
    'FinCEN obligation to file SAR when institution knows, suspects, or has reason to suspect illicit activity—no de minimis dollar floor.'
);

-- ---------------------------------------------------------------------------
-- Directed edges (indicator -> vector, legal -> vector, cross-vector correlation)
-- ---------------------------------------------------------------------------
INSERT INTO edges (source_node_id, target_node_id, relationship_type) VALUES
-- Account takeover indicators
('ind_rapid_ip_asn',              'vec_ato_session_hijack',       'TRIGGERS'),
('ind_impossible_travel',         'vec_ato_session_hijack',       'TRIGGERS'),
('ind_credential_stuffing_burst', 'vec_ato_session_hijack',       'EXPLOITS'),
-- Synthetic identity indicators
('ind_thin_file_bureau',          'vec_synthetic_identity_burst', 'TRIGGERS'),
('ind_ssn_issuance_mismatch',     'vec_synthetic_identity_burst', 'TRIGGERS'),
('ind_address_velocity',          'vec_synthetic_identity_burst', 'TRIGGERS'),
-- Device spoofing indicators
('ind_canvas_fp_mismatch',          'vec_device_fp_spoof',          'TRIGGERS'),
('ind_webgl_renderer_drift',        'vec_device_fp_spoof',          'TRIGGERS'),
('ind_emulator_build_props',        'vec_device_fp_spoof',          'TRIGGERS'),
-- Compliance constraints
('legal_psd3_sca',                'vec_ato_session_hijack',       'REQUIRES'),
('legal_aml_kyc_tier1',           'vec_synthetic_identity_burst', 'REQUIRES'),
('legal_psd3_sca',                'vec_device_fp_spoof',          'CONSTRAINS'),
('legal_bsa_sar_threshold',       'vec_synthetic_identity_burst', 'MANDATES'),
('legal_bsa_sar_threshold',       'vec_ato_session_hijack',       'MANDATES'),
-- Cross-vector operational correlation
('vec_ato_session_hijack',        'vec_device_fp_spoof',          'CORRELATES_WITH'),
('ind_credential_stuffing_burst', 'vec_device_fp_spoof',          'ENABLES');

-- ---------------------------------------------------------------------------
-- FSRS flashcards (due immediately: state=0, next_review=NULL)
-- ---------------------------------------------------------------------------
INSERT INTO cards (id, type, data) VALUES
(
    'card_ato_session_replay',
    'payload_drill',
    '{"question":"A logged-in user completes a wire transfer. Session telemetry shows three ASN changes in four minutes with no re-auth step-up. Classify the primary attack vector.","payload":{"event_id":"evt_8f2a","user_id":"usr_44102","session_id":"sess_k9m2","auth_method":"cookie_sso","asn_sequence":["AS7922","AS15169","AS3356"],"elapsed_minutes":4,"mfa_step_up":false,"transfer_amount_usd":12400},"answer":"Account Takeover via Session Hijacking. Stolen or replayed session token used across geographically inconsistent ASNs without SCA step-up—file SAR if confirmed, invalidate all refresh tokens, force PSD3 re-authentication."}'
),
(
    'card_synthetic_burst_onboard',
    'payload_drill',
    '{"question":"Onboarding batch flagged: five applicants share a mule address and SSN issuance cohort mismatch. Identify the fraud pattern.","payload":{"batch_id":"onb_2026_031","applicants":5,"shared_address_hash":"addr_9c1e","bureau_pulls_72h":11,"avg_tradeline_months":2,"ssn_cohort_delta_years":14,"first_settlement_attempt_hours":18},"answer":"Synthetic Identity Burst Fraud. Thin-file bureau velocity plus SSN/DOB inconsistency indicates fabricated identities staged for bust-out—hold settlements, escalate AML Tier 1 reverification, prepare BSA SAR narrative."}'
),
(
    'card_emulator_canvas_spoof',
    'payload_drill',
    '{"question":"Device risk score spiked despite stable UA string. Inspect fingerprint telemetry and name the evasion technique.","payload":{"device_id":"dev_x88","user_agent":"Mozilla/5.0 (iPhone; CPU iPhone OS 17_2 like Mac OS X)","canvas_hash_sequence":["a1b2","f9e8","a1b2","c3d4"],"webgl_renderer":"Google SwiftShader","screen":"390x844","emulator_props_detected":false,"transaction_count_30m":7},"answer":"Device Fingerprint Spoofing Emulators. Rotating canvas hash with SwiftShader renderer on claimed iOS hardware indicates anti-detect/emulator environment—block device, require hardware-backed WebAuthn, tune canvas stability rules."}'
),
(
    'card_psd3_sca_recall',
    'recall',
    '{"question":"Under PSD3 Strong Customer Authentication, when must a payment service provider apply SCA for remote card payments?","payload":null,"answer":"SCA is required for remote electronic payments unless a valid RTS exemption applies (low value, trusted beneficiary, TRA low fraud, corporate process, secure corporate payment). Account access and payment initiation both trigger SCA unless exempt."}'
),
(
    'card_velocity_rule_sandbox',
    'logic_sandbox',
    '{"question":"Write a heuristic rule that flags synthetic burst onboarding using bureau and address signals.","payload":{"sample_applicant":{"bureau_pulls_72h":9,"tradeline_months":1,"address_changes_7d":3,"ssn_cohort_match":false}},"answer":"IF bureau_pulls_72h > 5 AND tradeline_months < 6 AND address_changes_7d > 2 THEN risk_score = 90. Layer SSN cohort validation and shared-address clustering for production."}'
);

INSERT INTO fsrs_states (card_id, stability, difficulty, lapses, reviews, state, last_review, next_review) VALUES
('card_ato_session_replay',       0.0, 0.0, 0, 0, 0, NULL, NULL),
('card_synthetic_burst_onboard',  0.0, 0.0, 0, 0, 0, NULL, NULL),
('card_emulator_canvas_spoof',    0.0, 0.0, 0, 0, 0, NULL, NULL),
('card_psd3_sca_recall',          0.0, 0.0, 0, 0, 0, NULL, NULL),
('card_velocity_rule_sandbox',    0.0, 0.0, 0, 0, 0, NULL, NULL);

INSERT INTO card_node_links (card_id, node_id) VALUES
('card_ato_session_replay',       'vec_ato_session_hijack'),
('card_ato_session_replay',       'ind_rapid_ip_asn'),
('card_synthetic_burst_onboard',  'vec_synthetic_identity_burst'),
('card_synthetic_burst_onboard',  'ind_thin_file_bureau'),
('card_emulator_canvas_spoof',    'vec_device_fp_spoof'),
('card_emulator_canvas_spoof',    'ind_canvas_fp_mismatch'),
('card_psd3_sca_recall',          'legal_psd3_sca'),
('card_psd3_sca_recall',          'vec_ato_session_hijack'),
('card_velocity_rule_sandbox',    'vec_synthetic_identity_burst'),
('card_velocity_rule_sandbox',    'ind_address_velocity');
