SELECT
    spans.id AS "id!",
    files.path AS "file!",
    files.source_kind AS "source_kind!",
    spans.locator AS "locator!",
    spans.source_text AS "source_text!",
    spans.protected_tokens_json AS "protected_tokens_json!",
    spans.flags_json AS "flags_json!"
FROM spans
JOIN files ON files.id = spans.file_id
ORDER BY files.order_index, spans.order_index;
