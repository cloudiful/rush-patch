SELECT
    id AS "id!",
    source_text AS "source_text!",
    translated_text AS "translated_text!",
    status AS "status!"
FROM units
WHERE translated_text IS NOT NULL
  AND status = 'translated'
ORDER BY id;
