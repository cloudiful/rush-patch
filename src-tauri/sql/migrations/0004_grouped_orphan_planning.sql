ALTER TABLE planning_batches
    ADD COLUMN batch_kind TEXT NOT NULL DEFAULT 'single_segment';

ALTER TABLE planning_batches
    ADD COLUMN source_segments INTEGER NOT NULL DEFAULT 1;

ALTER TABLE planning_batches
    ADD COLUMN group_count INTEGER NOT NULL DEFAULT 1;

ALTER TABLE planning_batches
    ADD COLUMN source_files_json TEXT NOT NULL DEFAULT '[]';
