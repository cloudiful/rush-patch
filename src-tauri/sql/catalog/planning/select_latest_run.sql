SELECT
    id AS "id!: i64",
    status AS "status!",
    total_segments AS "total_segments!: i64",
    total_batches AS "total_batches!: i64",
    planned_segments AS "planned_segments!: i64",
    planned_batches AS "planned_batches!: i64",
    translated_segments AS "translated_segments!: i64",
    translated_batches AS "translated_batches!: i64"
FROM planning_runs
ORDER BY id DESC
LIMIT 1;
