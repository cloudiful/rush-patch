ALTER TABLE planning_runs
    ADD COLUMN batching_strategy TEXT NOT NULL DEFAULT 'maximize_utilization';

ALTER TABLE planning_batches
    ADD COLUMN batching_strategy TEXT NOT NULL DEFAULT 'maximize_utilization';

ALTER TABLE planning_batches
    ADD COLUMN hard_prompt_body_tokens INTEGER NOT NULL DEFAULT 0;

ALTER TABLE planning_batches
    ADD COLUMN flush_reason TEXT NOT NULL DEFAULT 'final_flush';
