INSERT INTO spans (
    id,
    file_id,
    order_index,
    locator,
    source_text,
    protected_tokens_json,
    flags_json
)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7);
