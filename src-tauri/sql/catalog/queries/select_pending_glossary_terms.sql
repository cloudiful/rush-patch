SELECT id AS "id!: i64",
       source_text AS "source_text!: String",
       target_text AS "target_text: String",
       term_kind AS "term_kind!: String",
       semantic_kind AS "semantic_kind!: String",
       source_file AS "source_file!: String",
       source_unit_id AS "source_unit_id!: String",
       source_json_path AS "source_json_path: String",
       priority AS "priority!: i64",
       status AS "status!: String",
       conflicted AS "conflicted!: i64"
FROM glossary_terms
WHERE status = 'pending'
  AND conflicted = 0
ORDER BY priority DESC, id ASC;
