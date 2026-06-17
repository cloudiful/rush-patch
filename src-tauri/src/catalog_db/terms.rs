use crate::catalog_db::CatalogDbError;
use crate::terminology::{
    AutoGlossaryCandidate, CanonicalTerm, TermMatchIndex, build_auto_glossary_candidates,
    candidate_seeds_from_catalog,
};
use crate::translation_io::TranslationResources;
use sqlx::FromRow;
use sqlx::SqlitePool;
use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, FromRow)]
pub struct GlossaryTermRecord {
    pub id: i64,
    pub source_text: String,
    pub target_text: Option<String>,
    pub term_kind: String,
    pub semantic_kind: String,
    pub source_file: String,
    pub source_unit_id: String,
    pub source_json_path: Option<String>,
    pub priority: i64,
    pub status: String,
    pub conflicted: i64,
}

#[derive(Debug, Clone)]
pub struct GlossaryTermUpdate {
    pub id: i64,
    pub target_text: Option<String>,
    pub status: String,
    pub conflicted: bool,
}

pub async fn insert_glossary_terms(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    catalog: &crate::domain::TranslationCatalog,
) -> Result<usize, CatalogDbError> {
    let candidates = build_auto_glossary_candidates(&candidate_seeds_from_catalog(catalog));
    let updated_at = timestamp_string();
    for candidate in &candidates {
        insert_candidate(tx, candidate, &updated_at).await?;
    }
    Ok(candidates.len())
}

pub async fn load_glossary_terms(pool: &SqlitePool) -> Result<Vec<GlossaryTermRecord>, CatalogDbError> {
    sqlx::query_file_as!(
        GlossaryTermRecord,
        "sql/catalog/queries/load_glossary_terms.sql"
    )
    .fetch_all(pool)
    .await
    .map_err(CatalogDbError::from)
}

pub async fn load_pending_glossary_terms(
    pool: &SqlitePool,
) -> Result<Vec<GlossaryTermRecord>, CatalogDbError> {
    sqlx::query_file_as!(
        GlossaryTermRecord,
        "sql/catalog/queries/select_pending_glossary_terms.sql"
    )
    .fetch_all(pool)
    .await
    .map_err(CatalogDbError::from)
}

pub async fn update_glossary_terms(
    pool: &SqlitePool,
    updates: &[GlossaryTermUpdate],
) -> Result<(), CatalogDbError> {
    if updates.is_empty() {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    let updated_at = timestamp_string();
    for update in updates {
        let target_text = update.target_text.as_deref();
        let conflicted = i64::from(update.conflicted);
        sqlx::query_file!(
            "sql/catalog/terms/update_translation.sql",
            target_text,
            update.status,
            conflicted,
            updated_at,
            update.id
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn load_glossary_source_unit_ids(
    pool: &SqlitePool,
) -> Result<BTreeSet<String>, CatalogDbError> {
    let rows = sqlx::query_file_scalar!("sql/catalog/queries/select_glossary_source_unit_ids.sql")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().collect())
}

pub async fn load_term_match_index(
    pool: &SqlitePool,
    resources: &TranslationResources,
) -> Result<TermMatchIndex, CatalogDbError> {
    let mut index = TermMatchIndex::from_terms(&resources.glossary_terms);
    let auto_terms = load_glossary_terms(pool).await?
        .into_iter()
        .filter(|term| !term.conflicted_as_bool())
        .filter_map(|term| {
            if !matches!(term.status.as_str(), "translated" | "validated") {
                return None;
            }
            Some((
                CanonicalTerm {
                    source: term.source_text,
                    target: term.target_text?,
                },
                term.priority,
            ))
        })
        .collect::<Vec<_>>();
    index.extend(auto_terms);
    Ok(index)
}

pub fn resolve_user_glossary_term(
    term: &GlossaryTermRecord,
    resources: &TranslationResources,
) -> Option<CanonicalTerm> {
    resources
        .glossary_terms
        .iter()
        .find(|candidate| candidate.source == term.source_text)
        .cloned()
}

impl GlossaryTermRecord {
    pub fn conflicted_as_bool(&self) -> bool {
        self.conflicted != 0
    }
}

async fn insert_candidate(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    candidate: &AutoGlossaryCandidate,
    updated_at: &str,
) -> Result<(), CatalogDbError> {
    let target_text = candidate.target_text.as_deref();
    let source_json_path = candidate.source_json_path.as_deref();
    let conflicted = i64::from(candidate.conflicted);
    sqlx::query_file!(
        "sql/catalog/terms/insert.sql",
        candidate.source_text,
        target_text,
        candidate.term_kind,
        candidate.semantic_kind,
        candidate.source_file,
        candidate.source_unit_id,
        source_json_path,
        candidate.priority,
        candidate.status,
        conflicted,
        updated_at
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn timestamp_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_owned())
}
