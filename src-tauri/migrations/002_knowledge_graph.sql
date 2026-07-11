-- Knowledge graph nodes: fraud attack path entities.
CREATE TABLE nodes (
    id           TEXT PRIMARY KEY NOT NULL,
    entity_type  TEXT NOT NULL CHECK (entity_type IN ('vector', 'indicator', 'legal', 'pattern')),
    title        TEXT NOT NULL,
    description  TEXT NOT NULL DEFAULT ''
);

CREATE INDEX idx_nodes_entity_type ON nodes (entity_type);
CREATE INDEX idx_nodes_title ON nodes (title);

-- Directed edges between graph nodes.
CREATE TABLE edges (
    source_node_id    TEXT NOT NULL REFERENCES nodes (id) ON DELETE CASCADE,
    target_node_id    TEXT NOT NULL REFERENCES nodes (id) ON DELETE CASCADE,
    relationship_type TEXT NOT NULL,
    PRIMARY KEY (source_node_id, target_node_id, relationship_type),
    CHECK (source_node_id != target_node_id)
);

CREATE INDEX idx_edges_target_node_id ON edges (target_node_id);
CREATE INDEX idx_edges_source_relationship ON edges (source_node_id, relationship_type);

-- Many-to-many: attach study cards to graph nodes.
CREATE TABLE card_node_links (
    card_id TEXT NOT NULL REFERENCES cards (id) ON DELETE CASCADE,
    node_id TEXT NOT NULL REFERENCES nodes (id) ON DELETE CASCADE,
    PRIMARY KEY (card_id, node_id)
);

CREATE INDEX idx_card_node_links_node_id ON card_node_links (node_id);
