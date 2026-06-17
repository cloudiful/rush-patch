UPDATE planning_runs
SET total_batches = total_batches + ?2,
    planned_segments = planned_segments + 1,
    planned_batches = planned_batches + ?2
WHERE id = ?1;
