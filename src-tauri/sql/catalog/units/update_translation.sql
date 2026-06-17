UPDATE units
SET translated_text = ?2,
    status = ?3
WHERE id = ?1;
