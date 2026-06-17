use super::batching::MAX_BATCH_GLOSSARY_TERMS;
use super::request::{request_batch_with_retries, truncated_error_message};
use super::{TranslateError, apply_batch_translations, unit_updates_for_indices};
use crate::app_state::CancellationFlag;
use crate::catalog_db::{
    self, GlossaryTermRecord, GlossaryTermUpdate, load_pending_glossary_terms,
    resolve_user_glossary_term,
};
use crate::domain::{ProjectConfig, TranslationStatus, TranslationUnit, WorkflowEventPhase};
use crate::prompting::PromptTokenCounter;
use crate::terminology::TermMatchIndex;
use crate::translation_io::TranslationResources;
use crate::workflow_events::WorkflowReporter;
use async_openai::Client;
use async_openai::config::OpenAIConfig;
use futures::future::join_all;
use sqlx::SqlitePool;
use std::collections::HashMap;

const MAX_TERM_ITEMS_PER_REQUEST: usize = 64;

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct TermPretranslateSummary {
    pub(crate) pretranslated_terms: usize,
    pub(crate) failed_terms: usize,
    pub(crate) batches: usize,
    pub(crate) retries: usize,
    pub(crate) cancelled: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedTermBatch {
    pub(crate) term_ids: Vec<i64>,
    pub(crate) source_unit_ids: Vec<String>,
    pub(crate) units: Vec<TranslationUnit>,
}

pub(crate) async fn run_term_pretranslate(
    pool: &SqlitePool,
    catalog_units: &mut [TranslationUnit],
    resources: &TranslationResources,
    config: &ProjectConfig,
    system_prompt: &str,
    client: &Client<OpenAIConfig>,
    cancellation: &CancellationFlag,
    reporter: &WorkflowReporter,
) -> Result<TermPretranslateSummary, TranslateError> {
    let pending_terms = load_pending_glossary_terms(pool).await?;
    if pending_terms.is_empty() {
        reporter.info_key(
            WorkflowEventPhase::Translate,
            "workflow.translate.termPretranslate",
            "Pretranslating glossary terms",
            Some("没有待处理的自动术语，跳过术语预翻译".to_owned()),
        );
        return Ok(TermPretranslateSummary::default());
    }

    reporter.info_key(
        WorkflowEventPhase::Translate,
        "workflow.translate.termPretranslate",
        "Pretranslating glossary terms",
        Some(format!("正在先处理 {} 条固定术语", pending_terms.len())),
    );

    let mut summary = TermPretranslateSummary::default();
    let (glossary_updates, unit_updates, remaining_terms) =
        apply_user_glossary_overrides(&pending_terms, catalog_units, resources);
    if !glossary_updates.is_empty() {
        catalog_db::update_glossary_terms(pool, &glossary_updates).await?;
        catalog_db::update_units_with_pool(pool, &unit_updates).await?;
        summary.pretranslated_terms += glossary_updates.len();
        reporter.info_key(
            WorkflowEventPhase::Translate,
            "workflow.translate.termGlossaryApplied",
            "Applied user glossary to automatic terms",
            Some(format!("根据用户术语表直接解决了 {} 条术语，无需请求模型", glossary_updates.len())),
        );
    }

    if remaining_terms.is_empty() {
        return Ok(summary);
    }

    let term_index = TermMatchIndex::from_terms(&resources.glossary_terms);
    let batches = plan_term_batches(
        &remaining_terms,
        &term_index,
        &config.model,
        system_prompt,
        config.target_input_tokens,
    )?;
    summary.batches += batches.len();
    if batches.is_empty() {
        return Ok(summary);
    }

    let unit_index_by_id = catalog_units
        .iter()
        .enumerate()
        .map(|(index, unit)| (unit.id.clone(), index))
        .collect::<HashMap<_, _>>();
    let max_concurrency = config.max_concurrency.max(1);

    for (wave_index, wave) in batches.chunks(max_concurrency).enumerate() {
        if cancellation.is_cancelled() {
            summary.cancelled = true;
            break;
        }

        let requests = wave.iter().enumerate().map(|(offset, batch)| {
            let glossary_terms = term_index.batch_terms_for_units(&batch.units, MAX_BATCH_GLOSSARY_TERMS);
            async move {
                request_batch_with_retries(
                    client,
                    config,
                    system_prompt,
                    &batch.units,
                    &[],
                    &glossary_terms,
                    cancellation,
                    reporter,
                    0,
                    wave_index * max_concurrency + offset + 1,
                )
                .await
            }
        });

        for (batch, result) in wave.iter().zip(join_all(requests).await) {
            if cancellation.is_cancelled() {
                summary.cancelled = true;
                break;
            }

            match result {
                Ok((translations, used_retries)) => {
                    summary.retries += used_retries;
                    let mut translated_units = batch.units.clone();
                    let translated_indices = (0..translated_units.len()).collect::<Vec<_>>();
                    apply_batch_translations(
                        &mut translated_units,
                        &translated_indices,
                        &translations,
                    );
                    let (glossary_updates, unit_updates, translated_count, failed_count) =
                        collect_batch_updates(&translated_units, batch, catalog_units, &unit_index_by_id);
                    summary.pretranslated_terms += translated_count;
                    summary.failed_terms += failed_count;
                    catalog_db::update_glossary_terms(pool, &glossary_updates).await?;
                    catalog_db::update_units_with_pool(pool, &unit_updates).await?;
                }
                Err(TranslateError::Cancelled) => {
                    summary.cancelled = true;
                    break;
                }
                Err(error) => {
                    reporter.warn(
                        WorkflowEventPhase::Translate,
                        "Glossary term batch failed",
                        Some(truncated_error_message(&error.to_string())),
                    );
                    let (glossary_updates, unit_updates) =
                        mark_batch_failed(batch, catalog_units, &unit_index_by_id);
                    summary.failed_terms += batch.term_ids.len();
                    catalog_db::update_glossary_terms(pool, &glossary_updates).await?;
                    catalog_db::update_units_with_pool(pool, &unit_updates).await?;
                }
            }
        }
    }

    Ok(summary)
}

pub(crate) fn plan_term_batches(
    terms: &[GlossaryTermRecord],
    term_index: &TermMatchIndex,
    model: &str,
    system_prompt: &str,
    max_input_tokens: usize,
) -> Result<Vec<PlannedTermBatch>, TranslateError> {
    let token_counter = PromptTokenCounter::new(model, system_prompt)?;
    let mut batches = Vec::new();
    let mut term_ids = Vec::new();
    let mut source_unit_ids = Vec::new();
    let mut units = Vec::new();

    for term in terms {
        let unit = prompt_unit_for_term(term);
        let would_exceed_items = units.len() >= MAX_TERM_ITEMS_PER_REQUEST;
        let would_exceed_tokens = if units.is_empty() {
            false
        } else {
            let mut candidate_units = units.clone();
            candidate_units.push(unit.clone());
            let glossary_terms =
                term_index.batch_terms_for_units(&candidate_units, MAX_BATCH_GLOSSARY_TERMS);
            token_counter.count_request_tokens_with_terms(&candidate_units, &[], &glossary_terms)?
                > max_input_tokens
        };

        if would_exceed_items || would_exceed_tokens {
            batches.push(PlannedTermBatch {
                term_ids,
                source_unit_ids,
                units,
            });
            term_ids = Vec::new();
            source_unit_ids = Vec::new();
            units = Vec::new();
        }

        units.push(unit);
        term_ids.push(term.id);
        source_unit_ids.push(term.source_unit_id.clone());
    }

    if !units.is_empty() {
        batches.push(PlannedTermBatch {
            term_ids,
            source_unit_ids,
            units,
        });
    }

    Ok(batches)
}

fn apply_user_glossary_overrides(
    pending_terms: &[GlossaryTermRecord],
    catalog_units: &mut [TranslationUnit],
    resources: &TranslationResources,
) -> (Vec<GlossaryTermUpdate>, Vec<catalog_db::CatalogUnitUpdate>, Vec<GlossaryTermRecord>) {
    let unit_index_by_id = catalog_units
        .iter()
        .enumerate()
        .map(|(index, unit)| (unit.id.clone(), index))
        .collect::<HashMap<_, _>>();
    let mut glossary_updates = Vec::new();
    let mut remaining_terms = Vec::new();
    let mut unit_indices = Vec::new();

    for term in pending_terms {
        let Some(override_term) = resolve_user_glossary_term(term, resources) else {
            remaining_terms.push(term.clone());
            continue;
        };
        glossary_updates.push(GlossaryTermUpdate {
            id: term.id,
            target_text: Some(override_term.target.clone()),
            status: "translated".to_owned(),
            conflicted: false,
        });
        if let Some(index) = unit_index_by_id.get(&term.source_unit_id).copied() {
            if let Some(unit) = catalog_units.get_mut(index) {
                unit.translated_text = Some(override_term.target);
                unit.status = TranslationStatus::Translated;
                unit_indices.push(index);
            }
        }
    }

    (
        glossary_updates,
        unit_updates_for_indices(catalog_units, &unit_indices),
        remaining_terms,
    )
}

fn collect_batch_updates(
    translated_units: &[TranslationUnit],
    batch: &PlannedTermBatch,
    catalog_units: &mut [TranslationUnit],
    unit_index_by_id: &HashMap<String, usize>,
) -> (
    Vec<GlossaryTermUpdate>,
    Vec<catalog_db::CatalogUnitUpdate>,
    usize,
    usize,
) {
    let mut glossary_updates = Vec::new();
    let mut unit_indices = Vec::new();
    let mut translated_count = 0usize;
    let mut failed_count = 0usize;

    for ((term_id, source_unit_id), translated_unit) in batch
        .term_ids
        .iter()
        .zip(batch.source_unit_ids.iter())
        .zip(translated_units.iter())
    {
        let translated_text = translated_unit.translated_text.clone();
        let success = translated_text.as_deref().is_some_and(|text| !text.trim().is_empty())
            && matches!(translated_unit.status, TranslationStatus::Translated);
        glossary_updates.push(GlossaryTermUpdate {
            id: *term_id,
            target_text: translated_text.clone(),
            status: if success { "translated" } else { "failed" }.to_owned(),
            conflicted: false,
        });

        if let Some(index) = unit_index_by_id.get(source_unit_id).copied()
            && let Some(unit) = catalog_units.get_mut(index)
        {
            unit.translated_text = translated_text;
            unit.status = if success {
                translated_count += 1;
                TranslationStatus::Translated
            } else {
                failed_count += 1;
                TranslationStatus::Failed
            };
            unit_indices.push(index);
        }
    }

    (
        glossary_updates,
        unit_updates_for_indices(catalog_units, &unit_indices),
        translated_count,
        failed_count,
    )
}

fn mark_batch_failed(
    batch: &PlannedTermBatch,
    catalog_units: &mut [TranslationUnit],
    unit_index_by_id: &HashMap<String, usize>,
) -> (Vec<GlossaryTermUpdate>, Vec<catalog_db::CatalogUnitUpdate>) {
    let mut glossary_updates = Vec::new();
    let mut unit_indices = Vec::new();
    for (term_id, source_unit_id) in batch.term_ids.iter().zip(batch.source_unit_ids.iter()) {
        glossary_updates.push(GlossaryTermUpdate {
            id: *term_id,
            target_text: None,
            status: "failed".to_owned(),
            conflicted: false,
        });
        if let Some(index) = unit_index_by_id.get(source_unit_id).copied()
            && let Some(unit) = catalog_units.get_mut(index)
        {
            unit.status = TranslationStatus::Failed;
            unit_indices.push(index);
        }
    }
    (
        glossary_updates,
        unit_updates_for_indices(catalog_units, &unit_indices),
    )
}

fn prompt_unit_for_term(term: &GlossaryTermRecord) -> TranslationUnit {
    TranslationUnit {
        id: format!("glossary-term-{}", term.id),
        group_id: format!("glossary-term-{}", term.id),
        semantic_kind: term.semantic_kind.clone(),
        context: crate::domain::ContextEnvelope {
            file: term.source_file.clone(),
            json_path: term.source_json_path.clone(),
            map_id: None,
            event_id: None,
            page_id: None,
            command_index: None,
            speaker_name: None,
            prev_texts: Vec::new(),
            next_texts: Vec::new(),
            block_text: None,
            glossary_hits: Vec::new(),
            notes: vec!["glossary_term".to_owned()],
        },
        source_text: term.source_text.clone(),
        translated_text: None,
        status: TranslationStatus::Pending,
        span_ids: Vec::new(),
    }
}
