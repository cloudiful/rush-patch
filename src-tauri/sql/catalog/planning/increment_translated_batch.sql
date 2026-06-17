UPDATE planning_runs
SET translated_batches = translated_batches + 1
WHERE id = ?1;
