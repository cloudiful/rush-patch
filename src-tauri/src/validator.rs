use crate::domain::{
    TranslationCatalog, TranslationSpan, TranslationStatus, TranslationUnit, ValidationReport,
    ValidationStatus,
};
use crate::terminology::TermMatchIndex;
use crate::text::{extract_protected_tokens, token_multiset};
use rayon::prelude::*;

#[cfg(test)]
mod order_tests;

pub fn validate_catalog(catalog: &TranslationCatalog) -> Vec<ValidationReport> {
    validate_catalog_with_terms(catalog, &TermMatchIndex::default())
}

pub fn validate_catalog_with_terms(
    catalog: &TranslationCatalog,
    terminology: &TermMatchIndex,
) -> Vec<ValidationReport> {
    catalog
        .units
        .par_iter()
        .map(|unit| validate_unit_with_terminology(unit, &catalog.spans, terminology))
        .collect()
}

pub fn validated_catalog(
    catalog: &TranslationCatalog,
) -> (TranslationCatalog, Vec<ValidationReport>) {
    let reports = validate_catalog(catalog);
    let mut validated = catalog.clone();

    for (unit, report) in validated.units.iter_mut().zip(&reports) {
        match report.status {
            ValidationStatus::Passed | ValidationStatus::Warning => {
                if unit.translated_text.is_some()
                    && !matches!(unit.status, TranslationStatus::Failed)
                {
                    unit.status = TranslationStatus::Validated;
                }
            }
            ValidationStatus::Failed => {
                unit.status = TranslationStatus::Failed;
            }
        }
    }

    (validated, reports)
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn validate_unit(unit: &TranslationUnit, spans: &[TranslationSpan]) -> ValidationReport {
    validate_unit_with_terminology(unit, spans, &TermMatchIndex::default())
}

fn validate_unit_with_terminology(
    unit: &TranslationUnit,
    spans: &[TranslationSpan],
    terminology: &TermMatchIndex,
) -> ValidationReport {
    let relevant_spans = spans_for_unit(unit, spans);
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut token_diff = Vec::new();

    let Some(translated_text) = unit.translated_text.as_ref() else {
        warnings.push("translated_text_missing".to_owned());
        return ValidationReport {
            unit_id: unit.id.clone(),
            status: ValidationStatus::Warning,
            errors,
            warnings,
            token_diff,
        };
    };

    if translated_text.trim().is_empty() {
        errors.push("translated_text_empty".to_owned());
    }

    if unit.span_ids.len() > 1 {
        let line_count = translated_text.lines().count();
        if line_count != unit.span_ids.len() {
            errors.push(format!(
                "line_count_mismatch:{}:{}",
                line_count,
                unit.span_ids.len()
            ));
        }
    }

    let source_tokens = extract_protected_tokens(&unit.source_text);
    let translated_tokens = extract_protected_tokens(translated_text);
    let source_multiset = token_multiset(&source_tokens);
    let translated_multiset = token_multiset(&translated_tokens);
    if source_multiset != translated_multiset {
        errors.push("protected_tokens_mismatch".to_owned());
        token_diff.push(format!("source={source_multiset:?}"));
        token_diff.push(format!("translated={translated_multiset:?}"));
    }

    let source_length = unit
        .source_text
        .chars()
        .filter(|char| !char.is_whitespace())
        .count();
    let translated_length = translated_text
        .chars()
        .filter(|char| !char.is_whitespace())
        .count();
    if source_length >= 4 {
        let ratio = translated_length as f32 / source_length as f32;
        if !(0.25..=4.0).contains(&ratio) {
            warnings.push(format!("suspicious_length_ratio:{ratio:.2}"));
        }
    }

    if relevant_spans
        .iter()
        .any(|span| span.source_text.trim().is_empty())
    {
        warnings.push("source_span_empty".to_owned());
    }

    warnings.extend(canonical_term_warnings(unit, translated_text, terminology));

    let status = if !errors.is_empty() {
        ValidationStatus::Failed
    } else if !warnings.is_empty() {
        ValidationStatus::Warning
    } else {
        ValidationStatus::Passed
    };

    ValidationReport {
        unit_id: unit.id.clone(),
        status,
        errors,
        warnings,
        token_diff,
    }
}

fn canonical_term_warnings(
    unit: &TranslationUnit,
    translated_text: &str,
    terminology: &TermMatchIndex,
) -> Vec<String> {
    terminology
        .source_text_terms(&unit.source_text, 3)
        .into_iter()
        .filter_map(|term| {
            if translated_text.contains(term.target) {
                return None;
            }
            let warning = if unit.semantic_kind == "name"
                && unit
                    .context
                    .json_path
                    .as_deref()
                    .is_some_and(|path| path.ends_with(".name"))
            {
                "canonical_name_mismatch"
            } else {
                "canonical_term_missing"
            };
            Some(format!("{warning}:{}=>{}", term.source, term.target))
        })
        .collect()
}

fn spans_for_unit<'a>(
    unit: &TranslationUnit,
    spans: &'a [TranslationSpan],
) -> Vec<&'a TranslationSpan> {
    spans
        .iter()
        .filter(|span| unit.span_ids.iter().any(|id| id == &span.id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ContextEnvelope, SourceKind};

    #[test]
    fn fails_when_placeholder_is_lost() {
        let span = TranslationSpan {
            id: "span".to_owned(),
            file: "Map001.json".to_owned(),
            source_kind: SourceKind::Json,
            locator: "$.events[0]".to_owned(),
            source_text: r"Hello %1$s \N[1]".to_owned(),
            protected_tokens: vec!["%1$s".to_owned(), r"\N[1]".to_owned()],
            flags: vec!["has_control_code".to_owned(), "has_placeholder".to_owned()],
        };
        let unit = TranslationUnit {
            id: "unit".to_owned(),
            group_id: "group".to_owned(),
            semantic_kind: "dialogue".to_owned(),
            context: empty_context(),
            source_text: r"Hello %1$s \N[1]".to_owned(),
            translated_text: Some("浣犲ソ".to_owned()),
            status: TranslationStatus::Translated,
            span_ids: vec!["span".to_owned()],
        };

        let report = validate_unit(&unit, &[span]);

        assert!(matches!(report.status, ValidationStatus::Failed));
        assert!(
            report
                .errors
                .iter()
                .any(|error| error == "protected_tokens_mismatch")
        );
    }

    #[test]
    fn warns_when_translation_missing() {
        let span = TranslationSpan {
            id: "span".to_owned(),
            file: "Map001.json".to_owned(),
            source_kind: SourceKind::Json,
            locator: "$.events[0]".to_owned(),
            source_text: "銇亜".to_owned(),
            protected_tokens: Vec::new(),
            flags: Vec::new(),
        };
        let unit = TranslationUnit {
            id: "unit".to_owned(),
            group_id: "group".to_owned(),
            semantic_kind: "choice".to_owned(),
            context: empty_context(),
            source_text: "銇亜".to_owned(),
            translated_text: None,
            status: TranslationStatus::Pending,
            span_ids: vec!["span".to_owned()],
        };

        let report = validate_unit(&unit, &[span]);

        assert!(matches!(report.status, ValidationStatus::Warning));
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning == "translated_text_missing")
        );
    }

    #[test]
    fn warns_when_canonical_name_drifts() {
        let canonical = TranslationUnit {
            id: "actor".to_owned(),
            group_id: "actor".to_owned(),
            semantic_kind: "name".to_owned(),
            context: ContextEnvelope {
                file: "www/data/Actors.json".to_owned(),
                json_path: Some("$[1].name".to_owned()),
                map_id: None,
                event_id: None,
                page_id: None,
                command_index: None,
                speaker_name: None,
                prev_texts: Vec::new(),
                next_texts: Vec::new(),
                block_text: None,
                glossary_hits: Vec::new(),
                notes: vec!["field_extraction".to_owned()],
            },
            source_text: "Romasha".to_owned(),
            translated_text: Some("罗玛夏".to_owned()),
            status: TranslationStatus::Validated,
            span_ids: Vec::new(),
        };
        let story = TranslationUnit {
            id: "story".to_owned(),
            group_id: "story".to_owned(),
            semantic_kind: "text".to_owned(),
            context: empty_context(),
            source_text: "Romasha story".to_owned(),
            translated_text: Some("罗马霞的故事".to_owned()),
            status: TranslationStatus::Translated,
            span_ids: Vec::new(),
        };
        let term = crate::terminology::CanonicalTerm {
            source: canonical.source_text.clone(),
            target: canonical
                .translated_text
                .clone()
                .expect("canonical translation"),
        };
        let catalog = TranslationCatalog {
            project: crate::domain::CatalogProject {
                game_root: String::new(),
                engine: "mz".to_owned(),
                generated_at: String::new(),
            },
            spans: Vec::new(),
            units: vec![canonical, story],
        };

        let reports = validate_catalog_with_terms(&catalog, &TermMatchIndex::from_terms(&[term]));
        let story_report = reports
            .iter()
            .find(|report| report.unit_id == "story")
            .expect("story report");

        assert!(matches!(story_report.status, ValidationStatus::Warning));
        assert!(
            story_report
                .warnings
                .iter()
                .any(|warning| warning.starts_with("canonical_term_missing:"))
        );
    }

    fn empty_context() -> ContextEnvelope {
        ContextEnvelope {
            file: "Map001.json".to_owned(),
            json_path: None,
            map_id: None,
            event_id: None,
            page_id: None,
            command_index: None,
            speaker_name: None,
            prev_texts: Vec::new(),
            next_texts: Vec::new(),
            block_text: None,
            glossary_hits: Vec::new(),
            notes: Vec::new(),
        }
    }
}
