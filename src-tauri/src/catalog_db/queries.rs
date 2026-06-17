use crate::catalog_db::migrate::migrate_catalog_db;
use crate::catalog_db::models::{FileCatalog, LoadedCatalog, build_catalog};
use crate::catalog_db::{CatalogDbError, CatalogUnitUpdate};
use crate::domain::{SourceKind, TranslationCatalog};
use sqlx::SqlitePool;
use std::collections::BTreeMap;
use std::path::Path;

pub async fn is_catalog_valid(path: &Path) -> Result<bool, CatalogDbError> {
    if !path.exists() {
        return Ok(false);
    }
    let pool = super::open_pool(path, false, 1).await?;
    migrate_catalog_db(&pool).await?;
    let version = sqlx::query_file_scalar!("sql/catalog/meta/select_value.sql", "format_version")
        .fetch_optional(&pool)
        .await?;
    let is_valid = matches!(
        version.as_deref(),
        Some(value) if value == super::FORMAT_VERSION.to_string()
    );
    pool.close().await;
    Ok(is_valid)
}

pub async fn load_catalog(path: &Path) -> Result<LoadedCatalog, CatalogDbError> {
    if !path.exists() {
        return Err(CatalogDbError::MissingDatabase(path.display().to_string()));
    }
    let pool = super::open_pool(path, false, 4).await?;
    let loaded = load_catalog_from_pool(&pool).await?;
    pool.close().await;
    Ok(loaded)
}

pub async fn load_catalog_from_pool(pool: &SqlitePool) -> Result<LoadedCatalog, CatalogDbError> {
    let version = sqlx::query_file_scalar!("sql/catalog/meta/select_value.sql", "format_version")
        .fetch_optional(pool)
        .await?;
    if !matches!(
        version.as_deref(),
        Some(value) if value == super::FORMAT_VERSION.to_string()
    ) {
        pool.close().await;
        return Err(CatalogDbError::UnsupportedFormat(
            version.unwrap_or_else(|| "missing".to_owned()),
        ));
    }

    let game_root = sqlx::query_file_scalar!("sql/catalog/meta/select_value.sql", "game_root")
        .fetch_one(pool)
        .await?;
    let engine = sqlx::query_file_scalar!("sql/catalog/meta/select_value.sql", "engine")
        .fetch_one(pool)
        .await?;
    let generated_at =
        sqlx::query_file_scalar!("sql/catalog/meta/select_value.sql", "generated_at")
            .fetch_one(pool)
            .await?;
    let unit_rows = sqlx::query_file_as!(
        crate::catalog_db::models::StoredUnitRow,
        "sql/catalog/queries/load_units.sql"
    )
    .fetch_all(pool)
    .await?;
    let span_rows = sqlx::query_file_as!(
        crate::catalog_db::models::StoredSpanRow,
        "sql/catalog/queries/load_spans.sql"
    )
    .fetch_all(pool)
    .await?;
    let unit_span_rows = sqlx::query_file_as!(
        crate::catalog_db::models::StoredUnitSpanRow,
        "sql/catalog/queries/load_unit_spans.sql"
    )
    .fetch_all(pool)
    .await?;

    build_catalog(
        crate::domain::CatalogProject {
            game_root,
            engine,
            generated_at,
        },
        unit_rows,
        span_rows,
        unit_span_rows,
    )
}

pub async fn update_units(
    path: &Path,
    updates: &[CatalogUnitUpdate],
) -> Result<(), CatalogDbError> {
    if updates.is_empty() {
        return Ok(());
    }
    let pool = super::open_pool(path, false, 1).await?;
    update_units_with_pool(&pool, updates).await?;
    pool.close().await;
    Ok(())
}

pub async fn update_units_with_pool(
    pool: &SqlitePool,
    updates: &[CatalogUnitUpdate],
) -> Result<(), CatalogDbError> {
    if updates.is_empty() {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    for update in updates {
        let id = update.id.as_str();
        let translated_text = update.translated_text.as_deref();
        let status = crate::catalog_db::models::status_to_db(&update.status);
        sqlx::query_file!(
            "sql/catalog/units/update_translation.sql",
            id,
            translated_text,
            status
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn load_file_catalogs_for_patch(path: &Path) -> Result<Vec<FileCatalog>, CatalogDbError> {
    let loaded = load_catalog(path).await?;
    let mut builders = BTreeMap::<String, FileCatalogBuilder>::new();
    for span in loaded.catalog.spans {
        builders
            .entry(span.file.clone())
            .or_insert_with(|| FileCatalogBuilder::new(span.file.clone(), span.source_kind))
            .spans
            .push(span);
    }
    for unit in loaded.catalog.units {
        if unit.translated_text.is_none()
            && !matches!(unit.status, crate::domain::TranslationStatus::Failed)
        {
            continue;
        }
        let kind = builders
            .get(&unit.context.file)
            .map(|builder| builder.kind)
            .unwrap_or_else(|| infer_kind_from_path(&unit.context.file));
        builders
            .entry(unit.context.file.clone())
            .or_insert_with(|| FileCatalogBuilder::new(unit.context.file.clone(), kind))
            .units
            .push(unit);
    }

    Ok(builders
        .into_values()
        .filter(|builder| !builder.spans.is_empty() || !builder.units.is_empty())
        .map(|builder| FileCatalog {
            file: builder.file,
            kind: builder.kind,
            catalog: TranslationCatalog {
                project: loaded.catalog.project.clone(),
                spans: builder.spans,
                units: builder.units,
            },
        })
        .collect())
}

struct FileCatalogBuilder {
    file: String,
    kind: SourceKind,
    spans: Vec<crate::domain::TranslationSpan>,
    units: Vec<crate::domain::TranslationUnit>,
}

impl FileCatalogBuilder {
    fn new(file: String, kind: SourceKind) -> Self {
        Self {
            file,
            kind,
            spans: Vec::new(),
            units: Vec::new(),
        }
    }
}

fn infer_kind_from_path(path: &str) -> SourceKind {
    if path
        .rsplit('.')
        .next()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("js"))
    {
        SourceKind::Js
    } else {
        SourceKind::Json
    }
}
