INSERT INTO planning_runs (
    created_at,
    model,
    max_input_tokens,
    target_input_tokens,
    batching_strategy,
    system_prompt_hash,
    pending_units,
    total_segments,
    status
)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'planning')
RETURNING id;
