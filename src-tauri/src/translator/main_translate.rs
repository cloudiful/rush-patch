use super::batching::{
    build_pending_segments, create_planning_context,
};
use super::orphan_grouping::orphan_pool_directory;
use super::planned_batch::PlannedBatch;
use super::request::{request_prepared_batch_with_retries, truncated_error_message};
use super::streaming_planner::StreamingBatchPlanner;
use super::{TranslateError, apply_batch_translations, unit_updates_for_indices};
use crate::app_state::CancellationFlag;
use crate::catalog_db::{
    self, PlanningBatchRecord, PlanningRun, PlanningSegmentRecord, record_planned_batches,
    record_planned_segments,
};
use crate::domain::{
    ProjectConfig, TranslationCatalog, TranslationStatus, TranslationUnit, WorkflowEventPhase,
};
use crate::terminology::TermMatchIndex;
use crate::workflow_events::WorkflowReporter;
use async_openai::Client;
use async_openai::config::OpenAIConfig;
use futures::future::join_all;
use sqlx::SqlitePool;
use std::collections::BTreeSet;

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct MainTranslationSummary {
    pub(crate) attempted_units: usize,
    pub(crate) translated_units: usize,
    pub(crate) failed_units: usize,
    pub(crate) batches: usize,
    pub(crate) retries: usize,
    pub(crate) cancelled: bool,
}

#[derive(Debug, Clone)]
struct TrackedBatch {
    batch_index: usize,
    batch_order_in_window: usize,
    batch: PlannedBatch,
}

pub(crate) async fn run_main_translation(
    pool: &SqlitePool,
    catalog: &mut TranslationCatalog,
    config: &ProjectConfig,
    system_prompt: &str,
    client: &Client<OpenAIConfig>,
    terminology: &TermMatchIndex,
    excluded_unit_ids: &BTreeSet<String>,
    cancellation: &CancellationFlag,
    reporter: &WorkflowReporter,
) -> Result<MainTranslationSummary, TranslateError> {
    let pending_indices = pending_main_unit_indices(&catalog.units, excluded_unit_ids);
    reporter.debug(
        WorkflowEventPhase::Translate,
        "Collected pending translation units",
        Some(format!(
            "{} pending main unit(s), {} glossary-owned unit(s) excluded",
            pending_indices.len(),
            excluded_unit_ids.len()
        )),
        [
            ("pending_units", pending_indices.len().to_string()),
            ("excluded_units", excluded_unit_ids.len().to_string()),
        ],
    );

    let segments = build_pending_segments(&catalog.units, &pending_indices);
    let planning_run = start_planning(pool, config, system_prompt, pending_indices.len(), &segments)
        .await?;
    if pending_indices.is_empty() {
        reporter.info_key(
            WorkflowEventPhase::Translate,
            "workflow.translate.noPending",
            "No pending main translation units",
            Some("没有待翻译的正文内容，跳过正文翻译".to_owned()),
        );
        return Ok(MainTranslationSummary::default());
    }

    reporter.info_key(
        WorkflowEventPhase::Translate,
        "workflow.translate.main",
        "Main translation",
        Some(format!(
            "正文待翻译 {} 条，整理为 {} 个规划片段（原始片段 {} 个）",
            pending_indices.len(),
            segments.len(),
            segments
                .iter()
                .map(|segment| segment.source_segment_count)
                .sum::<usize>()
        )),
    );

    let planning_units = catalog.units.clone();
    let planning_context = create_planning_context(
        &planning_units,
        &catalog.spans,
        &config.model,
        system_prompt,
        config.target_input_tokens,
        config.batching_strategy,
        terminology.clone(),
    )?;
    let mut summary = MainTranslationSummary::default();
    let max_concurrency = config.max_concurrency.max(1);
    let mut completed_batches = 0usize;
    let mut next_batch_index = 1usize;
    let mut completed_segments = vec![false; segments.len()];
    let mut planner = StreamingBatchPlanner::new(
        &planning_units,
        &segments,
        &planning_context,
        Some((reporter, WorkflowEventPhase::Translate)),
    );

    while let Some(dispatch) = planner.next_dispatch()? {
        if cancellation.is_cancelled() {
            summary.cancelled = true;
            break;
        }

        let tracked_batches = dispatch
            .batches
            .into_iter()
            .enumerate()
            .map(|(offset, batch)| TrackedBatch {
                batch_index: next_batch_index + offset,
                batch_order_in_window: offset + 1,
                batch,
            })
            .collect::<Vec<_>>();
        next_batch_index += tracked_batches.len();
        summary.batches += tracked_batches.len();

        let segment_records =
            build_segment_records(&dispatch.consumed_segment_indexes, &segments, &tracked_batches);
        let batch_records = build_batch_records(&tracked_batches, &planning_context, config)?;
        record_planned_segments(pool, planning_run, &segment_records).await?;
        record_planned_batches(pool, planning_run.id, &batch_records).await?;

        reporter.debug(
            WorkflowEventPhase::Translate,
            "Segment planning complete",
            Some(format!(
                "Segment {}/{} window consumed {} segment(s); {} batch(es); {} token evaluation(s); dispatching immediately",
                dispatch.anchor_segment_index + 1,
                planning_run.total_segments.max(1),
                dispatch.consumed_segment_indexes.len(),
                tracked_batches.len(),
                dispatch.metrics.token_evaluations,
            )),
            [
                ("segment_index", (dispatch.anchor_segment_index + 1).to_string()),
                ("consumed_segments", dispatch.consumed_segment_indexes.len().to_string()),
                ("planned_batches", tracked_batches.len().to_string()),
                ("token_evaluations", dispatch.metrics.token_evaluations.to_string()),
            ],
        );

        for (wave_index, wave) in tracked_batches.chunks(max_concurrency).enumerate() {
            if cancellation.is_cancelled() {
                summary.cancelled = true;
                break;
            }

            reporter.debug(
                WorkflowEventPhase::Translate,
                "Dispatching translation wave",
                Some(format!(
                    "Segment {}, wave {} with {} batch(es)",
                    dispatch.anchor_segment_index + 1,
                    wave_index + 1,
                    wave.len()
                )),
                [
                    ("segment_index", (dispatch.anchor_segment_index + 1).to_string()),
                    ("wave_index", (wave_index + 1).to_string()),
                    ("batch_count", wave.len().to_string()),
                ],
            );

            summary.attempted_units += wave
                .iter()
                .map(|batch| batch.batch.units.len())
                .sum::<usize>();
            let prepared_requests = wave
                .iter()
                .map(|tracked_batch| {
                    planning_context
                        .prepared_prompt_for_batch(&tracked_batch.batch)
                        .map(|prompt| (tracked_batch, prompt))
                })
                .collect::<Result<Vec<_>, _>>()?;
            for (tracked_batch, prepared_prompt) in &prepared_requests {
                let glossary_terms = planning_context.glossary_terms_for_units(&tracked_batch.batch.units);
                let estimated_input_tokens = planning_context
                    .estimated_input_tokens_for_batch(&tracked_batch.batch)?;
                reporter.debug(
                    WorkflowEventPhase::Translate,
                    "Prepared translation batch preview",
                    Some(format!(
                        "Batch {} ready: {} item(s), {} group(s), ~{} input token(s)",
                        tracked_batch.batch_index,
                        tracked_batch.batch.units.len(),
                        tracked_batch.batch.group_count(),
                        estimated_input_tokens
                    )),
                    [
                        ("batch_index", tracked_batch.batch_index.to_string()),
                        ("batch_kind", tracked_batch.batch.kind.as_str().to_owned()),
                        (
                            "batching_strategy",
                            planning_context.strategy().as_str().to_owned(),
                        ),
                        ("estimated_input_tokens", estimated_input_tokens.to_string()),
                        (
                            "target_input_tokens",
                            config.target_input_tokens.to_string(),
                        ),
                        (
                            "target_prompt_body_tokens",
                            planning_context.target_prompt_body_tokens().to_string(),
                        ),
                        (
                            "hard_prompt_body_tokens",
                            planning_context.hard_prompt_body_tokens().to_string(),
                        ),
                        (
                            "flush_reason",
                            tracked_batch.batch.flush_reason.as_str().to_owned(),
                        ),
                        (
                            "source_segments",
                            tracked_batch.batch.source_segment_count().to_string(),
                        ),
                        ("group_count", tracked_batch.batch.group_count().to_string()),
                        (
                            "item_count",
                            tracked_batch.batch.units.len().to_string(),
                        ),
                        (
                            "source_file_count",
                            tracked_batch.batch.source_file_count().to_string(),
                        ),
                        (
                            "source_files",
                            tracked_batch.batch.source_files().join(","),
                        ),
                        (
                            "pool_directory",
                            batch_pool_directory(&tracked_batch.batch).unwrap_or_else(|| "-".to_owned()),
                        ),
                        ("glossary_term_count", glossary_terms.len().to_string()),
                        (
                            "glossary_terms_preview",
                            glossary_terms
                                .iter()
                                .take(6)
                                .map(|term| format!("{}=>{}", term.source, term.target))
                                .collect::<Vec<_>>()
                                .join(" | "),
                        ),
                        ("request_body_bytes", prepared_prompt.body().len().to_string()),
                    ],
                );
            }
            let requests = prepared_requests.iter().map(|(tracked_batch, prepared_prompt)| async move {
                request_prepared_batch_with_retries(
                    client,
                    config,
                    system_prompt,
                    prepared_prompt.to_owned(),
                    tracked_batch.batch.units.len(),
                    cancellation,
                    reporter,
                    dispatch.anchor_segment_index + 1,
                    tracked_batch.batch_index,
                )
                .await
            });

            let mut wave_cancelled = false;
            for ((tracked_batch, _), result) in prepared_requests.iter().zip(join_all(requests).await) {
                match result {
                    Ok((translations, used_retries)) => {
                        summary.retries += used_retries;
                        apply_batch_translations(
                            &mut catalog.units,
                            &tracked_batch.batch.indices,
                            &translations,
                        );
                        catalog_db::update_units_with_pool(
                            pool,
                            &unit_updates_for_indices(&catalog.units, &tracked_batch.batch.indices),
                        )
                        .await?;
                        catalog_db::mark_batch_translated(
                            pool,
                            planning_run.id,
                            tracked_batch.batch_index,
                            used_retries,
                        )
                        .await?;
                        completed_batches += 1;
                        reporter.progress_throttled_key(
                            "translate-batches",
                            WorkflowEventPhase::Translate,
                            "workflow.translate.batchProgress",
                            "Completed translation batches",
                            completed_batches,
                            summary.batches.max(1),
                            Some(format!(
                                "已完成第 {} 个请求，共 {} 个；本次重试 {} 次",
                                tracked_batch.batch_index,
                                summary.batches.max(1),
                                used_retries
                            )),
                        );
                    }
                    Err(TranslateError::Cancelled) => {
                        summary.cancelled = true;
                        wave_cancelled = true;
                    }
                    Err(error) => {
                        catalog_db::mark_batch_failed(pool, planning_run.id, tracked_batch.batch_index, 0)
                            .await?;
                        catalog_db::mark_run_status(pool, planning_run.id, "failed").await?;
                        reporter.error_key(
                            WorkflowEventPhase::Translate,
                            "workflow.translate.requestFailed",
                            "Translation request failed",
                            Some(format!("请求失败：{}", truncated_error_message(&error.to_string()))),
                        );
                        return Err(error);
                    }
                }
            }

            if wave_cancelled || cancellation.is_cancelled() {
                summary.cancelled = true;
                break;
            }
        }

        if summary.cancelled {
            break;
        }

        for planned_segment_index in &dispatch.consumed_segment_indexes {
            catalog_db::mark_segment_translated(pool, planning_run.id, planned_segment_index + 1)
                .await?;
            if let Some(done) = completed_segments.get_mut(*planned_segment_index) {
                *done = true;
            }
        }
        let translated_segments = completed_segments.iter().filter(|done| **done).count();
        reporter.progress_throttled_key(
            "translate-segments",
            WorkflowEventPhase::Translate,
            "workflow.translate.segmentProgress",
            "Translated segments",
            translated_segments,
            planning_run.total_segments.max(1),
            Some(format!(
                "已推进到第 {} / {} 个规划片段，累计规划 {} 个请求",
                translated_segments,
                planning_run.total_segments.max(1),
                summary.batches,
            )),
        );
    }

    if summary.cancelled {
        catalog_db::mark_run_status(pool, planning_run.id, "cancelled").await?;
    } else {
        catalog_db::mark_run_status(pool, planning_run.id, "completed").await?;
    }

    summary.translated_units = catalog
        .units
        .iter()
        .filter(|unit| matches!(unit.status, TranslationStatus::Translated | TranslationStatus::Validated))
        .count();
    summary.failed_units = catalog
        .units
        .iter()
        .filter(|unit| matches!(unit.status, TranslationStatus::Failed))
        .count();

    Ok(summary)
}

async fn start_planning(
    pool: &SqlitePool,
    config: &ProjectConfig,
    system_prompt: &str,
    pending_units: usize,
    segments: &[super::batching::PendingSegment],
) -> Result<PlanningRun, TranslateError> {
    Ok(catalog_db::start_planning_run(
        pool,
        &config.model,
        config.target_input_tokens,
        config.batching_strategy.as_str(),
        system_prompt,
        pending_units,
        segments.len(),
    )
    .await?)
}

fn build_segment_records(
    planned_segment_indexes: &[usize],
    segments: &[super::batching::PendingSegment],
    tracked_batches: &[TrackedBatch],
) -> Vec<PlanningSegmentRecord> {
    planned_segment_indexes
        .iter()
        .copied()
        .map(|absolute_segment_index| {
            PlanningSegmentRecord {
                segment_index: absolute_segment_index + 1,
                unit_count: segments[absolute_segment_index].indices.len(),
                batch_count: tracked_batches
                    .iter()
                    .filter(|batch| {
                        batch.batch.groups.iter().any(|group| group.segment_index == absolute_segment_index)
                    })
                    .count(),
            }
        })
        .collect()
}

fn build_batch_records(
    tracked_batches: &[TrackedBatch],
    planning_context: &super::batching::PlanningContext,
    config: &ProjectConfig,
) -> Result<Vec<PlanningBatchRecord>, TranslateError> {
    tracked_batches
        .iter()
        .map(|tracked_batch| {
            Ok(PlanningBatchRecord {
                batch_index: tracked_batch.batch_index,
                segment_index: tracked_batch.batch.anchor_segment_index() + 1,
                batch_order_in_segment: tracked_batch.batch_order_in_window,
                unit_count: tracked_batch.batch.units.len(),
                batch_kind: tracked_batch.batch.kind.as_str().to_owned(),
                source_segments: tracked_batch.batch.source_segment_count(),
                group_count: tracked_batch.batch.group_count(),
                estimated_input_tokens: planning_context
                    .estimated_input_tokens_for_batch(&tracked_batch.batch)?,
                target_input_tokens: config.target_input_tokens,
                batching_strategy: config.batching_strategy.as_str().to_owned(),
                hard_prompt_body_tokens: planning_context.hard_prompt_body_tokens(),
                flush_reason: tracked_batch.batch.flush_reason.as_str().to_owned(),
                pool_directory: batch_pool_directory(&tracked_batch.batch),
                source_files_json: serde_json::to_string(&tracked_batch.batch.source_files())
                    .expect("source files should serialize"),
            })
        })
        .collect()
}

fn batch_pool_directory(batch: &PlannedBatch) -> Option<String> {
    if batch.kind.as_str() != "grouped_pool" {
        return None;
    }
    batch.groups
        .first()
        .map(|group| orphan_pool_directory(&group.file))
}

fn pending_main_unit_indices(
    units: &[TranslationUnit],
    excluded_unit_ids: &BTreeSet<String>,
) -> Vec<usize> {
    units
        .iter()
        .enumerate()
        .filter(|(_, unit)| !excluded_unit_ids.contains(&unit.id))
        .filter(|(_, unit)| {
            unit.translated_text.is_none()
                || matches!(
                    unit.status,
                    TranslationStatus::Pending | TranslationStatus::Failed
                )
        })
        .map(|(index, _)| index)
        .collect()
}
