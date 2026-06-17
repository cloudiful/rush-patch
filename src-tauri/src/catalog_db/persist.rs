use crate::catalog_db::insert_glossary_terms;
use crate::catalog_db::migrate::migrate_catalog_db;
use crate::catalog_db::models::{encode_json, source_kind_to_db, status_to_db};
use crate::catalog_db::CatalogDbError;
use crate::domain::{SourceKind, TranslationCatalog, TranslationUnit};
use crate::patch_storage;
use crate::workflow_events::WorkflowReporter;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const FORMAT_VERSION: u32 = 4;
pub const CATALOG_DB_FILE_NAME: &str = "catalog.sqlite";

pub fn catalog_path(game_root: &Path) -> PathBuf {
    patch_storage::work_dir(game_root).join(CATALOG_DB_FILE_NAME)
}

pub async fn persist_catalog(
    game_root: &Path,
    catalog: &TranslationCatalog,
    reporter: &WorkflowReporter,
) -> Result<PathBuf, CatalogDbError> {
    let work_dir = patch_storage::work_dir(game_root);
    fs::create_dir_all(&work_dir).map_err(|source| CatalogDbError::CreateDir {
        path: work_dir.display().to_string(),
        source,
    })?;
    let legacy_catalog_dir = super::reuse::legacy_catalog_dir(game_root);
    if legacy_catalog_dir.exists() {
        fs::remove_dir_all(&legacy_catalog_dir).map_err(|source| CatalogDbError::RemovePath {
            path: legacy_catalog_dir.display().to_string(),
            source,
        })?;
    }

    let db_path = catalog_path(game_root);
    if db_path.exists() {
        fs::remove_file(&db_path).map_err(|source| CatalogDbError::RemovePath {
            path: db_path.display().to_string(),
            source,
        })?;
    }
    patch_storage::mark_hidden_work_dir(game_root);

    let pool = super::open_pool(&db_path, true, 1).await?;
    migrate_catalog_db(&pool).await?;

    reporter.info_key(
        crate::domain::WorkflowEventPhase::Catalog,
        "workflow.catalog.writeSqlite",
        "Writing SQLite catalog",
        Some("正在把提取结果写入本地文本缓存数据库".to_owned()),
    );

    let mut tx = pool.begin().await?;
    write_meta(&mut tx, &catalog.project).await?;
    let file_ids = write_files(&mut tx, catalog).await?;
    write_units(&mut tx, catalog, &file_ids, reporter).await?;
    write_spans(&mut tx, catalog, &file_ids).await?;
    write_unit_spans(&mut tx, catalog, reporter).await?;
    reporter.info_key(
        crate::domain::WorkflowEventPhase::Catalog,
        "workflow.catalog.buildGlossary",
        "Building automatic glossary",
        Some("正在整理人名、道具名、状态名等固定术语".to_owned()),
    );
    let glossary_terms = insert_glossary_terms(&mut tx, catalog).await?;
    tx.commit().await?;
    pool.close().await;

    reporter.info_key(
        crate::domain::WorkflowEventPhase::Catalog,
        "workflow.catalog.ready",
        "SQLite catalog written",
        Some(format!(
            "文本缓存已写入完成：{} 条文本单元、{} 个写回片段、{} 条自动术语",
            catalog.units.len(),
            catalog.spans.len(),
            glossary_terms
        )),
    );
    Ok(db_path)
}

async fn write_meta(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    project: &crate::domain::CatalogProject,
) -> Result<(), CatalogDbError> {
    for (key, value) in [
        ("format_version", FORMAT_VERSION.to_string()),
        ("game_root", project.game_root.clone()),
        ("engine", project.engine.clone()),
        ("generated_at", project.generated_at.clone()),
    ] {
        sqlx::query_file!("sql/catalog/meta/upsert.sql", key, value)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

async fn write_files(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    catalog: &TranslationCatalog,
) -> Result<HashMap<String, i64>, CatalogDbError> {
    let mut file_ids = HashMap::<String, i64>::new();
    let mut next_id = 1i64;

    for span in &catalog.spans {
        if file_ids.contains_key(&span.file) {
            continue;
        }
        let file_path = span.file.as_str();
        let source_kind = source_kind_to_db(span.source_kind);
        let file_order = next_id - 1;
        sqlx::query_file!(
            "sql/catalog/files/insert.sql",
            next_id,
            file_path,
            source_kind,
            file_order
        )
        .execute(&mut **tx)
        .await?;
        file_ids.insert(span.file.clone(), next_id);
        next_id += 1;
    }

    for unit in &catalog.units {
        if file_ids.contains_key(&unit.context.file) {
            continue;
        }
        let source_kind = infer_kind_from_unit(unit);
        let file_path = unit.context.file.as_str();
        let source_kind_db = source_kind_to_db(source_kind);
        let file_order = next_id - 1;
        sqlx::query_file!(
            "sql/catalog/files/insert.sql",
            next_id,
            file_path,
            source_kind_db,
            file_order
        )
        .execute(&mut **tx)
        .await?;
        file_ids.insert(unit.context.file.clone(), next_id);
        next_id += 1;
    }

    Ok(file_ids)
}

async fn write_units(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    catalog: &TranslationCatalog,
    file_ids: &HashMap<String, i64>,
    reporter: &WorkflowReporter,
) -> Result<(), CatalogDbError> {
    let total = catalog.units.len();
    for (order_index, unit) in catalog.units.iter().enumerate() {
        let record_key = record_key_for_unit(unit);
        let scene_key = scene_key_for_unit(unit);
        let (batch_group_kind, batch_group_key) =
            batch_group_for_unit(unit, record_key.as_ref(), scene_key.as_ref());
        let prev_texts_json = encode_json(&unit.context.prev_texts)?;
        let next_texts_json = encode_json(&unit.context.next_texts)?;
        let notes_json = encode_json(&unit.context.notes)?;
        let glossary_hits_json = encode_json(&unit.context.glossary_hits)?;
        let unit_id = unit.id.as_str();
        let group_id = unit.group_id.as_str();
        let file_id = *file_ids
            .get(&unit.context.file)
            .expect("file id prepared for unit file");
        let order_index_db = order_index as i64;
        let semantic_kind = unit.semantic_kind.as_str();
        let status = status_to_db(&unit.status);
        let source_text = unit.source_text.as_str();
        let translated_text = unit.translated_text.as_deref();
        let json_path = unit.context.json_path.as_deref();
        let map_id = opt_u32_to_i64(unit.context.map_id);
        let event_id = opt_u32_to_i64(unit.context.event_id);
        let page_id = opt_u32_to_i64(unit.context.page_id);
        let command_index = opt_u32_to_i64(unit.context.command_index);
        let speaker_name = unit.context.speaker_name.as_deref();
        let block_text = unit.context.block_text.as_deref();
        let record_key_ref = record_key.as_deref();
        let scene_key_ref = scene_key.as_deref();
        let batch_group_key_ref = batch_group_key.as_str();
        sqlx::query_file!(
            "sql/catalog/units/insert.sql",
            unit_id,
            group_id,
            file_id,
            order_index_db,
            semantic_kind,
            status,
            source_text,
            translated_text,
            json_path,
            map_id,
            event_id,
            page_id,
            command_index,
            speaker_name,
            prev_texts_json,
            next_texts_json,
            block_text,
            notes_json,
            glossary_hits_json,
            record_key_ref,
            scene_key_ref,
            batch_group_kind,
            batch_group_key_ref
        )
        .execute(&mut **tx)
        .await?;

        reporter.progress_throttled_key(
            "catalog-sqlite-units",
            crate::domain::WorkflowEventPhase::Catalog,
            "workflow.catalog.persistUnits",
            "Persisting SQLite units",
            order_index + 1,
            total.max(1),
            Some(format!("正在写入文本单元 {}/{}", order_index + 1, total)),
        );
    }
    Ok(())
}

async fn write_spans(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    catalog: &TranslationCatalog,
    file_ids: &HashMap<String, i64>,
) -> Result<(), CatalogDbError> {
    for (order_index, span) in catalog.spans.iter().enumerate() {
        let protected_tokens_json = encode_json(&span.protected_tokens)?;
        let flags_json = encode_json(&span.flags)?;
        let span_id = span.id.as_str();
        let file_id = *file_ids
            .get(&span.file)
            .expect("file id prepared for span file");
        let order_index_db = order_index as i64;
        let locator = span.locator.as_str();
        let source_text = span.source_text.as_str();
        sqlx::query_file!(
            "sql/catalog/spans/insert.sql",
            span_id,
            file_id,
            order_index_db,
            locator,
            source_text,
            protected_tokens_json,
            flags_json
        )
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn write_unit_spans(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    catalog: &TranslationCatalog,
    reporter: &WorkflowReporter,
) -> Result<(), CatalogDbError> {
    let total = catalog.units.len();
    for (unit_index, unit) in catalog.units.iter().enumerate() {
        let unit_id = unit.id.as_str();
        for (span_order, span_id) in unit.span_ids.iter().enumerate() {
            let span_id_ref = span_id.as_str();
            let span_order_db = span_order as i64;
            sqlx::query_file!(
                "sql/catalog/unit_spans/insert.sql",
                unit_id,
                span_id_ref,
                span_order_db
            )
            .execute(&mut **tx)
            .await?;
        }
        reporter.progress_throttled_key(
            "catalog-sqlite-unit-spans",
            crate::domain::WorkflowEventPhase::Catalog,
            "workflow.catalog.persistLinks",
            "Persisting SQLite span links",
            unit_index + 1,
            total.max(1),
            Some(format!("正在建立写回映射 {}/{}", unit_index + 1, total)),
        );
    }
    Ok(())
}

fn opt_u32_to_i64(value: Option<u32>) -> Option<i64> {
    value.map(i64::from)
}

fn infer_kind_from_unit(unit: &TranslationUnit) -> SourceKind {
    if unit
        .context
        .file
        .rsplit('.')
        .next()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("js"))
    {
        SourceKind::Js
    } else {
        SourceKind::Json
    }
}

fn record_key_for_unit(unit: &TranslationUnit) -> Option<String> {
    let path = unit.context.json_path.as_deref()?;
    let suffix = array_record_prefix(path)?;
    Some(format!("{}::{suffix}", unit.context.file))
}

fn array_record_prefix(path: &str) -> Option<String> {
    if !path.starts_with("$[") {
        return None;
    }
    let end = path.find(']')?;
    Some(path[..=end].to_owned())
}

fn scene_key_for_unit(unit: &TranslationUnit) -> Option<String> {
    let is_event_group = unit.context.notes.iter().any(|note| {
        matches!(
            note.as_str(),
            "event_dialogue_block" | "event_scroll_text_block" | "choice_group"
        )
    });
    if !is_event_group {
        return None;
    }
    Some(format!(
        "{}::map={}::event={}::page={}",
        unit.context.file,
        unit.context.map_id.unwrap_or(0),
        unit.context.event_id.unwrap_or(0),
        unit.context.page_id.unwrap_or(0)
    ))
}

fn batch_group_for_unit(
    unit: &TranslationUnit,
    record_key: Option<&String>,
    scene_key: Option<&String>,
) -> (&'static str, String) {
    if let Some(scene_key) = scene_key {
        return ("event_scene", scene_key.clone());
    }
    if let Some(record_key) = record_key {
        return ("database_record", record_key.clone());
    }
    if unit.context.json_path.is_some() {
        return (
            "database_file_semantic",
            format!("{}::{}", unit.context.file, unit.semantic_kind),
        );
    }
    ("file_window", unit.context.file.clone())
}
