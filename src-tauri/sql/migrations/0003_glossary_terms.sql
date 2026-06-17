CREATE TABLE IF NOT EXISTS glossary_terms (
    id INTEGER PRIMARY KEY,
    source_text TEXT NOT NULL,
    target_text TEXT,
    term_kind TEXT NOT NULL,
    semantic_kind TEXT NOT NULL,
    source_file TEXT NOT NULL,
    source_unit_id TEXT NOT NULL,
    source_json_path TEXT,
    priority INTEGER NOT NULL,
    status TEXT NOT NULL,
    conflicted INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (source_unit_id) REFERENCES units(id) ON DELETE CASCADE,
    UNIQUE (term_kind, source_text)
);

CREATE INDEX IF NOT EXISTS idx_glossary_terms_status_priority
    ON glossary_terms(status, priority DESC, id);
CREATE INDEX IF NOT EXISTS idx_glossary_terms_source_text
    ON glossary_terms(source_text, term_kind);
CREATE INDEX IF NOT EXISTS idx_glossary_terms_source_unit
    ON glossary_terms(source_unit_id);
