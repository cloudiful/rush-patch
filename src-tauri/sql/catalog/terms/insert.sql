INSERT INTO glossary_terms (
    source_text,
    target_text,
    term_kind,
    semantic_kind,
    source_file,
    source_unit_id,
    source_json_path,
    priority,
    status,
    conflicted,
    updated_at
)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);
