-- Default local Ollama connection (overridable via Threat Intel Desk settings).
INSERT OR IGNORE INTO app_config (key, value) VALUES ('ollama_host', '127.0.0.1');
INSERT OR IGNORE INTO app_config (key, value) VALUES ('ollama_port', '11434');
INSERT OR IGNORE INTO app_config (key, value) VALUES ('ollama_model', 'llama3.2');
