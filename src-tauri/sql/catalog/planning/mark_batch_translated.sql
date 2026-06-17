UPDATE planning_batches
SET retries = ?3,
    completed_at = ?4,
    status = 'translated'
WHERE run_id = ?1
  AND batch_index = ?2;
