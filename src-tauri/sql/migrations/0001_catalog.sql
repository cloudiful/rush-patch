CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    source_kind TEXT NOT NULL,
    order_index INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS units (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL,
    file_id INTEGER NOT NULL,
    order_index INTEGER NOT NULL,
    semantic_kind TEXT NOT NULL,
    status TEXT NOT NULL,
    source_text TEXT NOT NULL,
    translated_text TEXT,
    json_path TEXT,
    map_id INTEGER,
    event_id INTEGER,
    page_id INTEGER,
    command_index INTEGER,
    speaker_name TEXT,
    prev_texts_json TEXT NOT NULL,
    next_texts_json TEXT NOT NULL,
    block_text TEXT,
    notes_json TEXT NOT NULL,
    glossary_hits_json TEXT NOT NULL,
    record_key TEXT,
    scene_key TEXT,
    batch_group_kind TEXT NOT NULL,
    batch_group_key TEXT NOT NULL,
    FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS spans (
    id TEXT PRIMARY KEY,
    file_id INTEGER NOT NULL,
    order_index INTEGER NOT NULL,
    locator TEXT NOT NULL,
    source_text TEXT NOT NULL,
    protected_tokens_json TEXT NOT NULL,
    flags_json TEXT NOT NULL,
    FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS unit_spans (
    unit_id TEXT NOT NULL,
    span_id TEXT NOT NULL,
    span_order INTEGER NOT NULL,
    PRIMARY KEY (unit_id, span_order),
    FOREIGN KEY (unit_id) REFERENCES units(id) ON DELETE CASCADE,
    FOREIGN KEY (span_id) REFERENCES spans(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_files_path ON files(path);
CREATE INDEX IF NOT EXISTS idx_units_file_order ON units(file_id, order_index);
CREATE INDEX IF NOT EXISTS idx_units_status_file_order ON units(status, file_id, order_index);
CREATE INDEX IF NOT EXISTS idx_units_batch_group ON units(batch_group_key, order_index);
CREATE INDEX IF NOT EXISTS idx_units_record_key ON units(record_key, order_index);
CREATE INDEX IF NOT EXISTS idx_units_scene_key ON units(scene_key, order_index);
CREATE INDEX IF NOT EXISTS idx_spans_file_order ON spans(file_id, order_index);
CREATE INDEX IF NOT EXISTS idx_unit_spans_unit_order ON unit_spans(unit_id, span_order);
