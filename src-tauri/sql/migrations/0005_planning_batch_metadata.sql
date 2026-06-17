ALTER TABLE planning_runs
    ADD COLUMN target_input_tokens INTEGER NOT NULL DEFAULT 0;

UPDATE planning_runs
SET target_input_tokens = max_input_tokens
WHERE target_input_tokens = 0;

ALTER TABLE planning_batches
    ADD COLUMN estimated_input_tokens INTEGER NOT NULL DEFAULT 0;

ALTER TABLE planning_batches
    ADD COLUMN target_input_tokens INTEGER NOT NULL DEFAULT 0;

ALTER TABLE planning_batches
    ADD COLUMN pool_directory TEXT;
