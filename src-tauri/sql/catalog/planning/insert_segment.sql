INSERT INTO planning_segments (
    run_id,
    segment_index,
    unit_count,
    batch_count,
    planned_at,
    completed_at,
    status
)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'planned')
ON CONFLICT(run_id, segment_index) DO UPDATE SET
    unit_count = excluded.unit_count,
    batch_count = excluded.batch_count,
    planned_at = excluded.planned_at,
    completed_at = excluded.completed_at,
    status = excluded.status;
