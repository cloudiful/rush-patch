UPDATE planning_runs
SET translated_segments = translated_segments + 1
WHERE id = ?1;
