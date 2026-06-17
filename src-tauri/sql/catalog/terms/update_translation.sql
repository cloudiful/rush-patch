UPDATE glossary_terms
SET target_text = ?,
    status = ?,
    conflicted = ?,
    updated_at = ?
WHERE id = ?;
