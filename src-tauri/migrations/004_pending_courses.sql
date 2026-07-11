-- Staged LLM course extractions awaiting analyst accept/reject.
CREATE TABLE pending_courses (
    id            TEXT PRIMARY KEY NOT NULL,
    source_title  TEXT NOT NULL,
    chunk_index   INTEGER NOT NULL DEFAULT 0,
    staged_at     TEXT NOT NULL,
    payload_json  TEXT NOT NULL,
    course_title  TEXT NOT NULL,
    category      TEXT NOT NULL,
    node_count    INTEGER NOT NULL DEFAULT 0,
    edge_count    INTEGER NOT NULL DEFAULT 0,
    card_count    INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_pending_courses_staged_at ON pending_courses (staged_at);
