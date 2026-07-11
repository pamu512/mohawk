-- Cards: flashcard content for payload drills, logic sandboxes, and recall prompts.
CREATE TABLE cards (
    id          TEXT PRIMARY KEY NOT NULL,
    type        TEXT NOT NULL CHECK (type IN ('payload_drill', 'logic_sandbox', 'recall')),
    data        TEXT NOT NULL DEFAULT '{}',
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX idx_cards_type ON cards (type);
CREATE INDEX idx_cards_created_at ON cards (created_at);

-- FSRS scheduling state (1:1 with cards).
CREATE TABLE fsrs_states (
    card_id     TEXT PRIMARY KEY NOT NULL REFERENCES cards (id) ON DELETE CASCADE,
    stability   REAL NOT NULL DEFAULT 0.0 CHECK (stability >= 0.0),
    difficulty  REAL NOT NULL DEFAULT 0.0 CHECK (difficulty >= 0.0),
    lapses      INTEGER NOT NULL DEFAULT 0 CHECK (lapses >= 0),
    reviews     INTEGER NOT NULL DEFAULT 0 CHECK (reviews >= 0),
    state       INTEGER NOT NULL DEFAULT 0 CHECK (state BETWEEN 0 AND 3),
    last_review TEXT,
    next_review TEXT
);

-- Due-card lookups and queue ordering by interval.
CREATE INDEX idx_fsrs_states_next_review ON fsrs_states (next_review);
CREATE INDEX idx_fsrs_states_state_next_review ON fsrs_states (state, next_review);
CREATE INDEX idx_fsrs_states_due ON fsrs_states (next_review)
    WHERE next_review IS NOT NULL;
