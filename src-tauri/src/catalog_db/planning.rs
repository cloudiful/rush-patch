use crate::catalog_db::CatalogDbError;
use sqlx::SqlitePool;
use std::hash::{Hash, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy)]
pub struct PlanningRun {
    pub id: i64,
    pub total_segments: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct PlanningSegmentRecord {
    pub segment_index: usize,
    pub unit_count: usize,
    pub batch_count: usize,
}

#[derive(Debug, Clone)]
pub struct PlanningBatchRecord {
    pub batch_index: usize,
    pub segment_index: usize,
    pub batch_order_in_segment: usize,
    pub unit_count: usize,
    pub batch_kind: String,
    pub source_segments: usize,
    pub group_count: usize,
    pub estimated_input_tokens: usize,
    pub target_input_tokens: usize,
    pub batching_strategy: String,
    pub hard_prompt_body_tokens: usize,
    pub flush_reason: String,
    pub pool_directory: Option<String>,
    pub source_files_json: String,
}

#[cfg(test)]
#[derive(Debug, Clone)]
pub struct PlanningRunSnapshot {
    pub id: i64,
    pub status: String,
    pub total_segments: usize,
    pub total_batches: usize,
    pub planned_segments: usize,
    pub planned_batches: usize,
    pub translated_segments: usize,
    pub translated_batches: usize,
}

#[cfg(test)]
#[derive(Debug, sqlx::FromRow)]
struct PlanningRunSnapshotRow {
    id: i64,
    status: String,
    total_segments: i64,
    total_batches: i64,
    planned_segments: i64,
    planned_batches: i64,
    translated_segments: i64,
    translated_batches: i64,
}

pub async fn start_planning_run(
    pool: &SqlitePool,
    model: &str,
    target_input_tokens: usize,
    batching_strategy: &str,
    system_prompt: &str,
    pending_units: usize,
    total_segments: usize,
) -> Result<PlanningRun, CatalogDbError> {
    let mut tx = pool.begin().await?;
    sqlx::query_file!("sql/catalog/planning/delete_batches.sql")
        .execute(&mut *tx)
        .await?;
    sqlx::query_file!("sql/catalog/planning/delete_segments.sql")
        .execute(&mut *tx)
        .await?;
    sqlx::query_file!("sql/catalog/planning/delete_runs.sql")
        .execute(&mut *tx)
        .await?;

    let created_at = timestamp_string();
    let system_prompt_hash = prompt_hash(system_prompt);
    let target_input_tokens_db = usize_to_i64(target_input_tokens);
    let pending_units_db = usize_to_i64(pending_units);
    let total_segments_db = usize_to_i64(total_segments);
    let run_id = sqlx::query_file_scalar!(
        "sql/catalog/planning/insert_run.sql",
        created_at,
        model,
        target_input_tokens_db,
        target_input_tokens_db,
        batching_strategy,
        system_prompt_hash,
        pending_units_db,
        total_segments_db
    )
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(PlanningRun {
        id: run_id,
        total_segments,
    })
}

pub async fn record_planned_segments(
    pool: &SqlitePool,
    run: PlanningRun,
    segments: &[PlanningSegmentRecord],
) -> Result<(), CatalogDbError> {
    if segments.is_empty() {
        return Ok(());
    }

    let mut tx = pool.begin().await?;
    let planned_at = timestamp_string();
    let completed_at = Option::<String>::None;

    for segment in segments {
        let segment_index_db = usize_to_i64(segment.segment_index);
        let unit_count_db = usize_to_i64(segment.unit_count);
        let batch_count_db = usize_to_i64(segment.batch_count);
        sqlx::query_file!(
            "sql/catalog/planning/insert_segment.sql",
            run.id,
            segment_index_db,
            unit_count_db,
            batch_count_db,
            planned_at,
            completed_at
        )
        .execute(&mut *tx)
        .await?;
    }

    let planned_batch_count_db = usize_to_i64(segments.iter().map(|segment| segment.batch_count).sum());
    sqlx::query_file!(
        "sql/catalog/planning/increment_planned_segment.sql",
        run.id,
        planned_batch_count_db
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn record_planned_batches(
    pool: &SqlitePool,
    run_id: i64,
    batches: &[PlanningBatchRecord],
) -> Result<(), CatalogDbError> {
    if batches.is_empty() {
        return Ok(());
    }

    let mut tx = pool.begin().await?;
    let planned_at = timestamp_string();
    let completed_at = Option::<String>::None;

    for batch in batches {
        let batch_index_db = usize_to_i64(batch.batch_index);
        let segment_index_db = usize_to_i64(batch.segment_index);
        let batch_order_in_segment_db = usize_to_i64(batch.batch_order_in_segment);
        let batch_unit_count_db = usize_to_i64(batch.unit_count);
        let source_segments_db = usize_to_i64(batch.source_segments);
        let group_count_db = usize_to_i64(batch.group_count);
        let estimated_input_tokens_db = usize_to_i64(batch.estimated_input_tokens);
        let target_input_tokens_db = usize_to_i64(batch.target_input_tokens);
        let hard_prompt_body_tokens_db = usize_to_i64(batch.hard_prompt_body_tokens);
        sqlx::query_file!(
            "sql/catalog/planning/insert_batch.sql",
            run_id,
            batch_index_db,
            segment_index_db,
            batch_order_in_segment_db,
            batch_unit_count_db,
            batch.batch_kind,
            source_segments_db,
            group_count_db,
            estimated_input_tokens_db,
            target_input_tokens_db,
            batch.batching_strategy,
            hard_prompt_body_tokens_db,
            batch.flush_reason,
            batch.pool_directory,
            batch.source_files_json,
            planned_at,
            completed_at
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn mark_batch_translated(
    pool: &SqlitePool,
    run_id: i64,
    batch_index: usize,
    retries: usize,
) -> Result<(), CatalogDbError> {
    let mut tx = pool.begin().await?;
    let completed_at = timestamp_string();
    let batch_index_db = usize_to_i64(batch_index);
    let retries_db = usize_to_i64(retries);
    sqlx::query_file!(
        "sql/catalog/planning/mark_batch_translated.sql",
        run_id,
        batch_index_db,
        retries_db,
        completed_at
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query_file!(
        "sql/catalog/planning/increment_translated_batch.sql",
        run_id
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn mark_batch_failed(
    pool: &SqlitePool,
    run_id: i64,
    batch_index: usize,
    retries: usize,
) -> Result<(), CatalogDbError> {
    let completed_at = timestamp_string();
    let batch_index_db = usize_to_i64(batch_index);
    let retries_db = usize_to_i64(retries);
    sqlx::query_file!(
        "sql/catalog/planning/mark_batch_failed.sql",
        run_id,
        batch_index_db,
        retries_db,
        completed_at
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_segment_translated(
    pool: &SqlitePool,
    run_id: i64,
    segment_index: usize,
) -> Result<(), CatalogDbError> {
    let mut tx = pool.begin().await?;
    let completed_at = timestamp_string();
    let segment_index_db = usize_to_i64(segment_index);
    sqlx::query_file!(
        "sql/catalog/planning/mark_segment_translated.sql",
        run_id,
        segment_index_db,
        completed_at
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query_file!(
        "sql/catalog/planning/increment_translated_segment.sql",
        run_id
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn mark_run_status(
    pool: &SqlitePool,
    run_id: i64,
    status: &str,
) -> Result<(), CatalogDbError> {
    let completed_at = timestamp_string();
    sqlx::query_file!(
        "sql/catalog/planning/update_run_status.sql",
        run_id,
        status,
        completed_at
    )
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
pub async fn load_latest_run_snapshot(
    pool: &SqlitePool,
) -> Result<Option<PlanningRunSnapshot>, CatalogDbError> {
    let row = sqlx::query_file_as!(
        PlanningRunSnapshotRow,
        "sql/catalog/planning/select_latest_run.sql"
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|row| PlanningRunSnapshot {
        id: row.id,
        status: row.status,
        total_segments: i64_to_usize(row.total_segments),
        total_batches: i64_to_usize(row.total_batches),
        planned_segments: i64_to_usize(row.planned_segments),
        planned_batches: i64_to_usize(row.planned_batches),
        translated_segments: i64_to_usize(row.translated_segments),
        translated_batches: i64_to_usize(row.translated_batches),
    }))
}

fn prompt_hash(system_prompt: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    system_prompt.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn timestamp_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_owned())
}

fn usize_to_i64(value: usize) -> i64 {
    i64::try_from(value).expect("planning counter exceeds SQLite INTEGER range")
}

#[cfg(test)]
fn i64_to_usize(value: i64) -> usize {
    usize::try_from(value).expect("SQLite planning counter should be non-negative")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog_db;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path(name: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        std::env::temp_dir().join(format!("rush_patch_planning_{name}_{stamp}.sqlite"))
    }

    #[tokio::test]
    async fn stores_segment_and_batch_progress() {
        let db_path = temp_db_path("planning_progress");
        let pool = catalog_db::open_pool(&db_path, true, 1)
            .await
            .expect("open pool");
        sqlx::migrate!("sql/migrations")
            .run(&pool)
            .await
            .expect("migrate");

        let run = start_planning_run(
            &pool,
            "gpt-4.1-mini",
            6000,
            "maximize_utilization",
            "Translate",
            8,
            2,
        )
            .await
            .expect("start run");
        record_planned_segments(
            &pool,
            run,
            &[PlanningSegmentRecord {
                segment_index: 1,
                unit_count: 5,
                batch_count: 2,
            }],
        )
        .await
        .expect("record segments");
        record_planned_batches(
            &pool,
            run.id,
            &[
                PlanningBatchRecord {
                    batch_index: 1,
                    segment_index: 1,
                    batch_order_in_segment: 1,
                    unit_count: 3,
                    batch_kind: "single_segment".to_owned(),
                    source_segments: 1,
                    group_count: 1,
                    estimated_input_tokens: 420,
                    target_input_tokens: 6000,
                    batching_strategy: "maximize_utilization".to_owned(),
                    hard_prompt_body_tokens: 6600,
                    flush_reason: "target_reached".to_owned(),
                    pool_directory: None,
                    source_files_json: "[\"www/data/Map001.json\"]".to_owned(),
                },
                PlanningBatchRecord {
                    batch_index: 2,
                    segment_index: 1,
                    batch_order_in_segment: 2,
                    unit_count: 2,
                    batch_kind: "single_segment".to_owned(),
                    source_segments: 1,
                    group_count: 1,
                    estimated_input_tokens: 380,
                    target_input_tokens: 6000,
                    batching_strategy: "maximize_utilization".to_owned(),
                    hard_prompt_body_tokens: 6600,
                    flush_reason: "final_flush".to_owned(),
                    pool_directory: None,
                    source_files_json: "[\"www/data/Map001.json\"]".to_owned(),
                },
            ],
        )
        .await
        .expect("record batches");
        mark_batch_translated(&pool, run.id, 1, 0)
            .await
            .expect("mark batch translated");
        mark_segment_translated(&pool, run.id, 1)
            .await
            .expect("mark segment translated");
        mark_run_status(&pool, run.id, "completed")
            .await
            .expect("mark run completed");

        let snapshot = load_latest_run_snapshot(&pool)
            .await
            .expect("load snapshot")
            .expect("snapshot row");
        assert_eq!(snapshot.id, run.id);
        assert_eq!(snapshot.status, "completed");
        assert_eq!(snapshot.total_segments, 2);
        assert_eq!(snapshot.total_batches, 2);
        assert_eq!(snapshot.planned_segments, 1);
        assert_eq!(snapshot.planned_batches, 2);
        assert_eq!(snapshot.translated_segments, 1);
        assert_eq!(snapshot.translated_batches, 1);

        pool.close().await;
        fs::remove_file(db_path).expect("cleanup db");
    }
}
