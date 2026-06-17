use crate::catalog_db::CatalogDbError;
use crate::domain::{
    ContextEnvelope, SourceKind, TranslationCatalog, TranslationSpan, TranslationStatus,
    TranslationUnit,
};
use serde::de::DeserializeOwned;
use sqlx::FromRow;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct LoadedCatalog {
    pub catalog: TranslationCatalog,
    pub planning: Vec<UnitPlanningMeta>,
}

#[derive(Debug, Clone)]
pub struct UnitPlanningMeta {
    pub batch_group_kind: String,
    pub batch_group_key: String,
    pub record_key: Option<String>,
    pub scene_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FileCatalog {
    pub file: String,
    pub kind: SourceKind,
    pub catalog: TranslationCatalog,
}

#[derive(Debug, FromRow)]
pub(crate) struct StoredUnitRow {
    pub id: String,
    pub group_id: String,
    pub file: String,
    pub semantic_kind: String,
    pub status: String,
    pub source_text: String,
    pub translated_text: Option<String>,
    pub json_path: Option<String>,
    pub map_id: Option<i64>,
    pub event_id: Option<i64>,
    pub page_id: Option<i64>,
    pub command_index: Option<i64>,
    pub speaker_name: Option<String>,
    pub prev_texts_json: String,
    pub next_texts_json: String,
    pub block_text: Option<String>,
    pub notes_json: String,
    pub glossary_hits_json: String,
    pub record_key: Option<String>,
    pub scene_key: Option<String>,
    pub batch_group_kind: String,
    pub batch_group_key: String,
}

#[derive(Debug, FromRow)]
pub(crate) struct StoredSpanRow {
    pub id: String,
    pub file: String,
    pub source_kind: String,
    pub locator: String,
    pub source_text: String,
    pub protected_tokens_json: String,
    pub flags_json: String,
}

#[derive(Debug, FromRow)]
pub(crate) struct StoredUnitSpanRow {
    pub unit_id: String,
    pub span_id: String,
    pub span_order: i64,
}

#[derive(Debug, FromRow)]
pub(crate) struct ReusableTranslationRow {
    pub id: String,
    pub source_text: String,
    pub translated_text: String,
    pub status: String,
}

pub(crate) fn build_catalog(
    project: crate::domain::CatalogProject,
    unit_rows: Vec<StoredUnitRow>,
    span_rows: Vec<StoredSpanRow>,
    unit_span_rows: Vec<StoredUnitSpanRow>,
) -> Result<LoadedCatalog, CatalogDbError> {
    let mut span_ids_by_unit = HashMap::<String, Vec<(i64, String)>>::new();
    for row in unit_span_rows {
        span_ids_by_unit
            .entry(row.unit_id)
            .or_default()
            .push((row.span_order, row.span_id));
    }
    for span_ids in span_ids_by_unit.values_mut() {
        span_ids.sort_by_key(|(order, _)| *order);
    }

    let spans = span_rows
        .into_iter()
        .map(|row| {
            Ok(TranslationSpan {
                id: row.id,
                file: row.file,
                source_kind: source_kind_from_db(&row.source_kind)?,
                locator: row.locator,
                source_text: row.source_text,
                protected_tokens: decode_json_vec(&row.protected_tokens_json)?,
                flags: decode_json_vec(&row.flags_json)?,
            })
        })
        .collect::<Result<Vec<_>, CatalogDbError>>()?;

    let mut units = Vec::with_capacity(unit_rows.len());
    let mut planning = Vec::with_capacity(unit_rows.len());

    for row in unit_rows {
        let span_ids = span_ids_by_unit
            .remove(&row.id)
            .unwrap_or_default()
            .into_iter()
            .map(|(_, span_id)| span_id)
            .collect::<Vec<_>>();
        units.push(TranslationUnit {
            id: row.id.clone(),
            group_id: row.group_id,
            semantic_kind: row.semantic_kind,
            context: ContextEnvelope {
                file: row.file,
                json_path: row.json_path,
                map_id: row.map_id.map(|value| value as u32),
                event_id: row.event_id.map(|value| value as u32),
                page_id: row.page_id.map(|value| value as u32),
                command_index: row.command_index.map(|value| value as u32),
                speaker_name: row.speaker_name,
                prev_texts: decode_json_vec(&row.prev_texts_json)?,
                next_texts: decode_json_vec(&row.next_texts_json)?,
                block_text: row.block_text,
                glossary_hits: decode_json_vec(&row.glossary_hits_json)?,
                notes: decode_json_vec(&row.notes_json)?,
            },
            source_text: row.source_text,
            translated_text: row.translated_text,
            status: status_from_db(&row.status)?,
            span_ids,
        });
        planning.push(UnitPlanningMeta {
            batch_group_kind: row.batch_group_kind,
            batch_group_key: row.batch_group_key,
            record_key: row.record_key,
            scene_key: row.scene_key,
        });
    }

    Ok(LoadedCatalog {
        catalog: TranslationCatalog {
            project,
            spans,
            units,
        },
        planning,
    })
}

pub(crate) fn status_to_db(status: &TranslationStatus) -> &'static str {
    match status {
        TranslationStatus::Pending => "pending",
        TranslationStatus::Translated => "translated",
        TranslationStatus::Validated => "validated",
        TranslationStatus::Failed => "failed",
    }
}

pub(crate) fn status_from_db(value: &str) -> Result<TranslationStatus, CatalogDbError> {
    match value {
        "pending" => Ok(TranslationStatus::Pending),
        "translated" => Ok(TranslationStatus::Translated),
        "validated" => Ok(TranslationStatus::Validated),
        "failed" => Ok(TranslationStatus::Failed),
        other => Err(CatalogDbError::InvalidStatus(other.to_owned())),
    }
}

pub(crate) fn source_kind_to_db(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Json => "json",
        SourceKind::Js => "js",
    }
}

pub(crate) fn source_kind_from_db(value: &str) -> Result<SourceKind, CatalogDbError> {
    match value {
        "json" => Ok(SourceKind::Json),
        "js" => Ok(SourceKind::Js),
        other => Err(CatalogDbError::InvalidSourceKind(other.to_owned())),
    }
}

pub(crate) fn encode_json<T: serde::Serialize>(value: &T) -> Result<String, CatalogDbError> {
    Ok(serde_json::to_string(value)?)
}

fn decode_json_vec<T: DeserializeOwned>(raw: &str) -> Result<Vec<T>, CatalogDbError> {
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_str(raw)?)
}
