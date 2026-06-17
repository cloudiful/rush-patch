use crate::domain::WorkflowEventPhase;
use crate::domain::{
    ContextEnvelope, SourceKind, TranslationSpan, TranslationStatus, TranslationUnit,
};
use crate::js_strings::{escape_for_quote, extract_js_strings};
use crate::parallel;
use crate::patch_storage::SourceFile;
use crate::text::extract_protected_tokens;
use crate::text_io;
use crate::workflow_events::WorkflowReporter;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use thiserror::Error;

#[cfg(test)]
mod order_tests;

#[derive(Debug, Error)]
pub enum ExtractJsError {
    #[error("failed to read {path}: {source}")]
    ReadFile {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid replacement plan for {file}")]
    InvalidReplacementPlan { file: String },
}

pub fn extract_entries(
    plugin_files: &[SourceFile],
    reporter: &WorkflowReporter,
) -> Result<(Vec<TranslationSpan>, Vec<TranslationUnit>), ExtractJsError> {
    let total_files = plugin_files.len();
    let completed = AtomicUsize::new(0);
    let extracted = parallel::ordered_map(plugin_files, |_, source_file| {
        let extracted = extract_source_file(source_file)?;
        let current = completed.fetch_add(1, Ordering::SeqCst) + 1;
        reporter.progress_throttled_key(
            "catalog-js-files",
            WorkflowEventPhase::Catalog,
            "workflow.catalog.extractJs",
            "Extracting JS files",
            current,
            total_files.max(1),
            Some(format!("正在提取 JS 文件 {}/{}", current, total_files)),
        );
        Ok::<_, ExtractJsError>(extracted)
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
) -> Result<(Vec<TranslationSpan>, Vec<TranslationUnit>), ExtractJsError> {
    let raw =
        text_io::read_text(&source_file.read_path).map_err(|source| ExtractJsError::ReadFile {
            path: source_file.read_path.display().to_string(),
            source,
        })?;
    let file = source_file.logical_path.display().to_string();
    let mut spans = Vec::new();
    let mut units = Vec::new();

    for candidate in extract_js_strings(&source_file.logical_path, &raw) {
        let span_id = format!("{file}::{}", candidate.locator);
        spans.push(TranslationSpan {
            id: span_id.clone(),
            file: file.clone(),
            source_kind: SourceKind::Js,
            locator: candidate.locator.clone(),
            source_text: candidate.decoded_text.clone(),
            protected_tokens: extract_protected_tokens(&candidate.decoded_text),
            flags: collect_flags(&candidate.decoded_text),
        });
        units.push(TranslationUnit {
            id: span_id.clone(),
            group_id: span_id.clone(),
            semantic_kind: "js_string".to_owned(),
            context: ContextEnvelope {
                file: file.clone(),
                json_path: None,
                map_id: None,
                event_id: None,
                page_id: None,
                command_index: None,
                speaker_name: None,
                prev_texts: Vec::new(),
                next_texts: Vec::new(),
                block_text: Some(candidate.decoded_text.clone()),
                glossary_hits: Vec::new(),
                notes: vec![
                    "js_plugin_string".to_owned(),
                    format!("quote:{}", candidate.quote),
                ],
            },
            source_text: candidate.decoded_text,
            translated_text: None,
            status: TranslationStatus::Pending,
            span_ids: vec![span_id],
        });
    }

    Ok((spans, units))
}

#[cfg(test)]
pub fn extract_entries_from_paths(
    plugin_files: &[String],
) -> Result<(Vec<TranslationSpan>, Vec<TranslationUnit>), ExtractJsError> {
    let sources = plugin_files
        .iter()
        .map(|file| SourceFile {
            logical_path: Path::new(file).to_path_buf(),
            read_path: Path::new(file).to_path_buf(),
        })
        .collect::<Vec<_>>();
    extract_entries(&sources, &WorkflowReporter::noop())
}

pub fn apply_catalog_to_js(
    file: &Path,
    raw: &str,
    catalog: &crate::domain::TranslationCatalog,
) -> Result<Option<String>, ExtractJsError> {
    let file_string = file.display().to_string();
    let candidates = extract_js_strings(file, raw);
    if candidates.is_empty() {
        return Ok(None);
    }

    let mut replacements = Vec::new();
    for unit in &catalog.units {
        let Some(translated_text) = unit.translated_text.as_ref() else {
            continue;
        };
        if translated_text.trim().is_empty()
            || matches!(unit.status, TranslationStatus::Failed)
            || unit.span_ids.len() != 1
        {
            continue;
        }

        let Some(span_id) = unit.span_ids.first() else {
            continue;
        };
        let Some(span) = catalog.spans.iter().find(|span| {
            span.id == *span_id && span.file == file_string && span.source_kind == SourceKind::Js
        }) else {
            continue;
        };
        let Some(candidate) = candidates
            .iter()
            .find(|candidate| candidate.locator == span.locator)
        else {
            return Err(ExtractJsError::InvalidReplacementPlan {
                file: file_string.clone(),
            });
        };

        replacements.push((
            candidate.content_start,
            candidate.content_end,
            escape_for_quote(translated_text, candidate.quote),
        ));
    }

    if replacements.is_empty() {
        return Ok(None);
    }

    replacements.sort_by_key(|(start, _, _)| *start);

    let mut rendered = String::with_capacity(raw.len());
    let mut cursor = 0usize;
    for (start, end, replacement) in replacements {
        if start < cursor || end > raw.len() || start > end {
            return Err(ExtractJsError::InvalidReplacementPlan {
                file: file_string.clone(),
            });
        }
        rendered.push_str(&raw[cursor..start]);
        rendered.push_str(&replacement);
        cursor = end;
    }
    rendered.push_str(&raw[cursor..]);

    if rendered == raw {
        Ok(None)
    } else {
        Ok(Some(rendered))
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
    if text.lines().count() > 1 {
        flags.push("multi_line".to_owned());
    }
    flags
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{CatalogProject, TranslationCatalog};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        std::env::temp_dir().join(format!("rush_patch_js_apply_{name}_{stamp}"))
    }

    #[test]
    fn rewrites_only_targeted_js_literals() {
        let root = temp_root("apply");
        let plugin_dir = root.join("www").join("js").join("plugins");
        fs::create_dir_all(&plugin_dir).expect("create plugin dir");

        let file = plugin_dir.join("Example.js");
        let raw = "const message = \"Hello hero\";\nconst untouched = \"DEBUG_FLAG\";\n";
        fs::write(&file, raw).expect("write plugin");

        let (spans, mut units) =
            extract_entries_from_paths(&[file.display().to_string()]).expect("extract");
        let target = units
            .iter_mut()
            .find(|unit| unit.source_text == "Hello hero")
            .expect("message unit");
        target.translated_text = Some("\\u4f60\\u597d\\uff0c\\u52c7\\u8005".to_owned());
        target.status = TranslationStatus::Translated;

        let catalog = TranslationCatalog {
            project: CatalogProject {
                game_root: root.display().to_string(),
                engine: "MV".to_owned(),
                generated_at: "0".to_owned(),
            },
            spans,
            units,
        };

        let updated = apply_catalog_to_js(&file, raw, &catalog)
            .expect("apply js")
            .expect("updated js");

        assert!(
            updated.contains("const message = \"\\\\u4f60\\\\u597d\\\\uff0c\\\\u52c7\\\\u8005\";")
        );
        assert!(updated.contains("const untouched = \"DEBUG_FLAG\";"));

        fs::remove_dir_all(root).expect("cleanup");
    }
}
