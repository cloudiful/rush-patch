SELECT
    units.id AS "id!",
    units.group_id AS "group_id!",
    files.path AS "file!",
    units.semantic_kind AS "semantic_kind!",
    units.status AS "status!",
    units.source_text AS "source_text!",
    units.translated_text AS "translated_text?",
    units.json_path AS "json_path?",
    units.map_id AS "map_id?",
    units.event_id AS "event_id?",
    units.page_id AS "page_id?",
    units.command_index AS "command_index?",
    units.speaker_name AS "speaker_name?",
    units.prev_texts_json AS "prev_texts_json!",
    units.next_texts_json AS "next_texts_json!",
    units.block_text AS "block_text?",
    units.notes_json AS "notes_json!",
    units.glossary_hits_json AS "glossary_hits_json!",
    units.record_key AS "record_key?",
    units.scene_key AS "scene_key?",
    units.batch_group_kind AS "batch_group_kind!",
    units.batch_group_key AS "batch_group_key!"
FROM units
JOIN files ON files.id = units.file_id
ORDER BY files.order_index, units.order_index;
