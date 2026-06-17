UPDATE planning_segments
SET completed_at = ?3,
    status = 'translated'
WHERE run_id = ?1
  AND segment_index = ?2;
