SELECT
    unit_id AS "unit_id!",
    span_id AS "span_id!",
    span_order AS "span_order!"
FROM unit_spans
ORDER BY unit_id, span_order;
