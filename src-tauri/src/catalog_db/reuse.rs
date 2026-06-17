use crate::catalog_db::models::status_from_db;
use crate::catalog_db::{CatalogDbError, ReusableTranslation};
use crate::patch_storage;
use sqlx::query_file_as;
use std::collections::BTreeMap;
use std::path::Path;

pub async fn load_reusable_translations(
    path: &Path,
) -> Result<BTreeMap<(String, String), ReusableTranslation>, CatalogDbError> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let pool = super::open_pool(path, false, 1).await?;
    let rows = query_file_as!(
        crate::catalog_db::models::ReusableTranslationRow,
        "sql/catalog/queries/select_reusable_translations.sql"
    )
    .fetch_all(&pool)
    .await?;
    pool.close().await;

    rows.into_iter()
        .map(|row| {
            Ok((
                (row.id, row.source_text),
                ReusableTranslation {
                    translated_text: row.translated_text,
                    status: status_from_db(&row.status)?,
                },
            ))
        })
        .collect()
}

pub(crate) fn legacy_catalog_dir(game_root: &Path) -> std::path::PathBuf {
    patch_storage::work_dir(game_root).join("catalog")
}
