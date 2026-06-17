UPDATE planning_runs
SET completed_at = ?3,
    status = ?2
WHERE id = ?1;
