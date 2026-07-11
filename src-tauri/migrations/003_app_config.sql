-- Application configuration key-value store (sync timestamps, feature flags).
CREATE TABLE app_config (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL DEFAULT ''
);

INSERT OR IGNORE INTO app_config (key, value) VALUES ('last_sync_timestamp', '');
