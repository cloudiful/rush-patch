mod event_context;
mod events;
#[cfg(test)]
mod order_tests;
#[cfg(test)]
mod tests;

#[cfg(not(test))]
use crate::catalog;
#[cfg(not(test))]
use crate::domain::JsonExtractionPreview;
use crate::domain::WorkflowEventPhase;
use crate::domain::{
    ContextEnvelope, SourceKind, TranslationCatalog, TranslationSpan, TranslationStatus,
    TranslationUnit,
};
use crate::parallel;
use crate::patch_storage::SourceFile;
use crate::scanner;
use crate::text::extract_protected_tokens;
use crate::text_io;
use crate::workflow_events::WorkflowReporter;
use serde_json::Value;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExtractJsonError {
    #[error(transparent)]
    Scan(#[from] scanner::ScanError),
    #[cfg(not(test))]
    #[error("failed to build combined catalog: {0}")]
    CatalogBuild(String),
    #[error("failed to read {path}: {source}")]
    ReadFile {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse JSON {path}: {source}")]
    ParseJson {
        path: String,
        #[source]
        source: serde_json::Error,
    },
}

#[derive(Default)]
pub(super) struct TraversalState {
    pub(super) previous_dialogue_lines: VecDeque<String>,
    pub(super) last_speaker: Option<String>,
}

#[cfg(not(test))]
pub fn extract_project_json(game_root: &str) -> Result<JsonExtractionPreview, ExtractJsonError> {
    let reporter = WorkflowReporter::noop();
    let catalog = catalog::build_catalog(game_root, &reporter).map_err(map_catalog_error)?;

    Ok(JsonExtractionPreview {
        total_units: catalog.units.len(),
        total_spans: catalog.spans.len(),
        sample_units: catalog.units.into_iter().take(12).collect(),
    })
}

pub fn extract_entries(
    data_files: &[SourceFile],
    reporter: &WorkflowReporter,
) -> Result<(Vec<TranslationSpan>, Vec<TranslationUnit>), ExtractJsonError> {
    let total_files = data_files.len();
    let completed = AtomicUsize::new(0);
    let extracted = parallel::ordered_map(data_files, |_, source_file| {
        let extracted = extract_source_file(source_file)?;
        let current = completed.fetch_add(1, Ordering::SeqCst) + 1;
        reporter.progress_throttled_key(
            "catalog-json-files",
            WorkflowEventPhase::Catalog,
            "workflow.catalog.extractJson",
            "Extracting JSON files",
            current,
            total_files.max(1),
            Some(format!("正在提取 JSON 文件 {}/{}", current, total_files)),
        );
        Ok::<_, ExtractJsonError>(extracted)
    })?;
    let span_count = extracted.iter().map(|(spans, _)| spans.len()).sum();
    let unit_count = extracted.iter().map(|(_, units)| units.len()).sum();
    let mut spans = Vec::with_capacity(span_count);
    let mut units = Vec::with_capacity(unit_count);

    for (file_spans, file_units) in extracted {
        spans.extend(file_spans);
        units.extend(file_units);
    }

    Ok((spans, units))
}

fn extract_source_file(
    source_file: &SourceFile,
) -> Result<(Vec<TranslationSpan>, Vec<TranslationUnit>), ExtractJsonError> {
    let content = text_io::read_text(&source_file.read_path).map_err(|source| {
        ExtractJsonError::ReadFile {
            path: source_file.read_path.display().to_string(),
            source,
        }
    })?;
    let file = source_file.logical_path.display().to_string();
    let json =
        serde_json::from_str::<Value>(&content).map_err(|source| ExtractJsonError::ParseJson {
            path: source_file.read_path.display().to_string(),
            source,
        })?;

    let mut spans = Vec::new();
    let mut units = Vec::new();
    let mut traversal = TraversalState::default();
    extract_from_value(
        &file,
        "$",
        &json,
        &mut traversal,
        &mut units,
        &mut spans,
        infer_map_id(&file),
    );
    Ok((spans, units))
}

#[cfg(test)]
pub fn extract_entries_from_paths(
    data_files: &[String],
) -> Result<(Vec<TranslationSpan>, Vec<TranslationUnit>), ExtractJsonError> {
    let sources = data_files
        .iter()
        .map(|file| SourceFile {
            logical_path: Path::new(file).to_path_buf(),
            read_path: Path::new(file).to_path_buf(),
        })
        .collect::<Vec<_>>();
    extract_entries(&sources, &WorkflowReporter::noop())
}

#[cfg(not(test))]
fn map_catalog_error(error: catalog::CatalogError) -> ExtractJsonError {
    match error {
        catalog::CatalogError::Extract(inner) => inner,
        catalog::CatalogError::Scan(inner) => ExtractJsonError::Scan(inner),
        catalog::CatalogError::ExtractJs(inner) => {
            ExtractJsonError::CatalogBuild(inner.to_string())
        }
        other => ExtractJsonError::CatalogBuild(other.to_string()),
    }
}

fn extract_from_value(
    file: &str,
    path: &str,
    value: &Value,
    traversal: &mut TraversalState,
    units: &mut Vec<TranslationUnit>,
    spans: &mut Vec<TranslationSpan>,
    map_id: Option<u32>,
) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let next_path = format!("{path}.{key}");
                match child {
                    Value::String(text)
                        if should_extract_string_field(file, &next_path, key, text) =>
                    {
                        push_field_unit(file, &next_path, key, text, units, spans, map_id);
                    }
                    _ => {
                        extract_from_value(file, &next_path, child, traversal, units, spans, map_id)
                    }
                }
            }
        }
        Value::Array(list) => {
            if path.ends_with(".list")
                && events::extract_event_command_list(
                    file, path, list, traversal, units, spans, map_id,
                )
            {
                return;
            }
            for (index, child) in list.iter().enumerate() {
                let next_path = format!("{path}[{index}]");
                extract_from_value(file, &next_path, child, traversal, units, spans, map_id);
            }
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn push_field_unit(
    file: &str,
    path: &str,
    key: &str,
    text: &str,
    units: &mut Vec<TranslationUnit>,
    spans: &mut Vec<TranslationSpan>,
    map_id: Option<u32>,
) {
    push_unit(
        file,
        path,
        semantic_kind_for_key(key),
        vec![(path.to_owned(), text.to_owned())],
        ContextEnvelope {
            file: file.to_owned(),
            json_path: Some(path.to_owned()),
            map_id,
            event_id: infer_event_id(path),
            page_id: infer_page_id(path),
            command_index: infer_command_index(path),
            speaker_name: None,
            prev_texts: Vec::new(),
            next_texts: Vec::new(),
            block_text: Some(text.to_owned()),
            glossary_hits: Vec::new(),
            notes: vec!["field_extraction".to_owned()],
        },
        units,
        spans,
    );
}

pub(super) fn push_unit(
    file: &str,
    group_locator: &str,
    semantic_kind: &str,
    entries: Vec<(String, String)>,
    context: ContextEnvelope,
    units: &mut Vec<TranslationUnit>,
    spans: &mut Vec<TranslationSpan>,
) {
    let group_id = format!("{file}::{group_locator}");
    let mut span_ids = Vec::new();
    let mut lines = Vec::new();

    for (locator, source_text) in entries {
        let span_id = format!("{file}::{locator}");
        span_ids.push(span_id.clone());
        lines.push(source_text.clone());
        spans.push(TranslationSpan {
            id: span_id,
            file: file.to_owned(),
            source_kind: SourceKind::Json,
            locator,
            protected_tokens: extract_protected_tokens(&source_text),
            flags: collect_flags(&source_text),
            source_text,
        });
    }

    units.push(TranslationUnit {
        id: group_id.clone(),
        group_id,
        semantic_kind: semantic_kind.to_owned(),
        context,
        source_text: lines.join("\n"),
        translated_text: None,
        status: TranslationStatus::Pending,
        span_ids,
    });
}

pub fn apply_catalog_to_json_value(
    file: &Path,
    value: &mut Value,
    catalog: &TranslationCatalog,
) -> usize {
    let mut updates = 0;
    let file_path = file.display().to_string();

    for unit in &catalog.units {
        let Some(translated_text) = unit.translated_text.as_ref() else {
            continue;
        };
        if matches!(unit.status, TranslationStatus::Failed) || translated_text.trim().is_empty() {
            continue;
        }

        let replacements: Vec<&str> = translated_text.lines().collect();
        for (index, span_id) in unit.span_ids.iter().enumerate() {
            let Some(span) = catalog
                .spans
                .iter()
                .find(|span| span.id == *span_id && span.file == file_path)
            else {
                continue;
            };

            let replacement = replacements
                .get(index)
                .map(|value| (*value).to_owned())
                .or_else(|| replacements.last().map(|value| (*value).to_owned()))
                .unwrap_or_else(|| span.source_text.clone());

            if set_json_string(value, &span.locator, replacement) {
                updates += 1;
            }
        }
    }

    updates
}

fn set_json_string(root: &mut Value, locator: &str, replacement: String) -> bool {
    let Some(pointer) = crate::json_pointer::dot_path_to_pointer(locator) else {
        return false;
    };
    pointer.assign(root, Value::String(replacement)).is_ok()
}

fn should_extract_key(key: &str) -> bool {
    matches!(
        key,
        "name"
            | "nickname"
            | "profile"
            | "description"
            | "message1"
            | "message2"
            | "message3"
            | "message4"
            | "note"
            | "text"
            | "displayName"
            | "currencyUnit"
            | "gameTitle"
    )
}

fn should_extract_string_field(file: &str, path: &str, key: &str, text: &str) -> bool {
    should_extract_key(key) && is_translatable(text) && !is_known_asset_reference(file, path)
}

fn is_known_asset_reference(file: &str, path: &str) -> bool {
    file.ends_with("Animations.json") && path.contains(".timings[") && path.ends_with(".se.name")
}

fn semantic_kind_for_key(key: &str) -> &'static str {
    match key {
        "name" | "nickname" | "displayName" | "gameTitle" => "name",
        "description" | "profile" | "message1" | "message2" | "message3" | "message4" => {
            "description"
        }
        "currencyUnit" => "system",
        _ => "text",
    }
}

pub(super) fn previous_dialogue_lines(traversal: &TraversalState) -> Vec<String> {
    traversal.previous_dialogue_lines.iter().cloned().collect()
}

pub(super) fn clear_dialogue_context(traversal: &mut TraversalState) {
    traversal.previous_dialogue_lines.clear();
    traversal.last_speaker = None;
}

pub(super) fn remember_dialogue(traversal: &mut TraversalState, text: &str) {
    traversal.previous_dialogue_lines.push_back(text.to_owned());
    while traversal.previous_dialogue_lines.len() > 4 {
        traversal.previous_dialogue_lines.pop_front();
    }
}

fn collect_flags(text: &str) -> Vec<String> {
    let mut flags = Vec::new();
    if text.contains('\\') {
        flags.push("has_control_code".to_owned());
    }
    if text.contains('%') || text.contains('{') {
        flags.push("has_placeholder".to_owned());
    }
    if text.lines().count() > 3 {
        flags.push("multi_line".to_owned());
    }
    flags
}

pub(super) fn is_translatable(text: &str) -> bool {
    let trimmed = text.trim();
    !trimmed.is_empty() && trimmed.chars().any(|char| !char.is_ascii_punctuation())
}

fn infer_map_id(file: &str) -> Option<u32> {
    let path = Path::new(file);
    let name = path.file_stem()?.to_str()?;
    let suffix = name.strip_prefix("Map")?;
    suffix.parse::<u32>().ok()
}

pub(super) fn infer_event_id(path: &str) -> Option<u32> {
    extract_number_after(path, ".events.")
}

pub(super) fn infer_page_id(path: &str) -> Option<u32> {
    extract_index_after(path, ".pages[")
}

fn infer_command_index(path: &str) -> Option<u32> {
    extract_index_after(path, ".list[")
}

fn extract_number_after(path: &str, marker: &str) -> Option<u32> {
    let start = path.find(marker)? + marker.len();
    let rest = &path[start..];
    let digits: String = rest
        .chars()
        .take_while(|char| char.is_ascii_digit())
        .collect();
    digits.parse::<u32>().ok()
}

fn extract_index_after(path: &str, marker: &str) -> Option<u32> {
    let start = path.find(marker)? + marker.len();
    let rest = &path[start..];
    let digits: String = rest
        .chars()
        .take_while(|char| char.is_ascii_digit())
        .collect();
    digits.parse::<u32>().ok()
}
