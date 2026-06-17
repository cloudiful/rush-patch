CREATE TABLE IF NOT EXISTS planning_runs (
    id INTEGER PRIMARY KEY,
    created_at TEXT NOT NULL,
    completed_at TEXT,
    model TEXT NOT NULL,
    max_input_tokens INTEGER NOT NULL,
    system_prompt_hash TEXT NOT NULL,
    pending_units INTEGER NOT NULL,
    total_segments INTEGER NOT NULL DEFAULT 0,
    total_batches INTEGER NOT NULL DEFAULT 0,
    planned_segments INTEGER NOT NULL DEFAULT 0,
    planned_batches INTEGER NOT NULL DEFAULT 0,
    translated_segments INTEGER NOT NULL DEFAULT 0,
    translated_batches INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS planning_segments (
    run_id INTEGER NOT NULL,
    segment_index INTEGER NOT NULL,
    unit_count INTEGER NOT NULL,
    batch_count INTEGER NOT NULL,
    planned_at TEXT NOT NULL,
    completed_at TEXT,
    status TEXT NOT NULL,
    PRIMARY KEY (run_id, segment_index),
    FOREIGN KEY (run_id) REFERENCES planning_runs(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS planning_batches (
    run_id INTEGER NOT NULL,
    batch_index INTEGER NOT NULL,
    segment_index INTEGER NOT NULL,
    batch_order_in_segment INTEGER NOT NULL,
    unit_count INTEGER NOT NULL,
    retries INTEGER NOT NULL DEFAULT 0,
    planned_at TEXT NOT NULL,
    completed_at TEXT,
    status TEXT NOT NULL,
    PRIMARY KEY (run_id, batch_index),
    FOREIGN KEY (run_id) REFERENCES planning_runs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_planning_segments_run_status
    ON planning_segments(run_id, status, segment_index);
CREATE INDEX IF NOT EXISTS idx_planning_batches_run_status
    ON planning_batches(run_id, status, batch_index);
