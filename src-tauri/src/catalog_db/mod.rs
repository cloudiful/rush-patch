mod migrate;
mod models;
mod persist;
mod planning;
mod pool;
mod queries;
mod reuse;
mod terms;

use crate::domain::TranslationStatus;
pub use models::{FileCatalog, LoadedCatalog, UnitPlanningMeta};
pub use persist::{CATALOG_DB_FILE_NAME, FORMAT_VERSION, catalog_path, persist_catalog};
pub use planning::{
    PlanningBatchRecord, PlanningRun, PlanningSegmentRecord, mark_batch_failed,
    mark_batch_translated, mark_run_status, mark_segment_translated, record_planned_batches,
    record_planned_segments, start_planning_run,
};
#[cfg(test)]
pub use planning::{PlanningRunSnapshot, load_latest_run_snapshot};
pub use pool::open_pool;
pub use queries::{
    is_catalog_valid, load_catalog, load_catalog_from_pool, load_file_catalogs_for_patch,
    update_units, update_units_with_pool,
};
pub use reuse::load_reusable_translations;
pub use terms::{
    GlossaryTermRecord, GlossaryTermUpdate, insert_glossary_terms, load_glossary_source_unit_ids,
    load_glossary_terms, load_pending_glossary_terms, load_term_match_index,
    resolve_user_glossary_term, update_glossary_terms,
};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct ReusableTranslation {
    pub translated_text: String,
    pub status: TranslationStatus,
}

#[derive(Debug, Clone)]
pub struct CatalogUnitUpdate {
    pub id: String,
    pub translated_text: Option<String>,
    pub status: TranslationStatus,
}

#[derive(Debug, Error)]
pub enum CatalogDbError {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("failed to create directory {path}: {source}")]
    CreateDir {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to remove path {path}: {source}")]
    RemovePath {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("catalog database missing: {0}")]
    MissingDatabase(String),
    #[error("unsupported catalog format version: {0}")]
    UnsupportedFormat(String),
    #[error("invalid translation status: {0}")]
    InvalidStatus(String),
    #[error("invalid source kind: {0}")]
    InvalidSourceKind(String),
    #[error("invalid glossary term status: {0}")]
    InvalidGlossaryTermStatus(String),
    #[error("unit_span references unknown unit {unit_id}")]
    MissingUnitForSpan { unit_id: String },
}

pub fn catalog_path_string(path: &PathBuf) -> String {
    path.display().to_string()
}
