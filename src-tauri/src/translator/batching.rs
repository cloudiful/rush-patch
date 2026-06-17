use super::batch_strategy::{BatchStrategyProfile, effective_prompt_budget, hard_prompt_budget};
use super::planned_batch::{BatchFlushReason, PlannedBatch, PlannedBatchGroup};
use super::streaming_planner::collect_planned_batches;
use crate::domain::BatchingStrategy;
use crate::domain::WorkflowEventPhase;
use crate::domain::{TranslationSpan, TranslationUnit};
use crate::prompting;
use crate::terminology::{CanonicalTerm, TermMatchIndex};
use crate::workflow_events::WorkflowReporter;
use std::collections::BTreeSet;
use std::time::Instant;

pub(crate) const MAX_BATCH_GLOSSARY_TERMS: usize = 8;

pub(crate) struct PlanningContext {
    token_counter: prompting::PromptTokenCounter,
    prompt_seeds: Vec<prompting::PromptRenderItemSeed>,
    strategy: BatchingStrategy,
    profile: BatchStrategyProfile,
    target_prompt_body_tokens: usize,
    hard_prompt_body_tokens: usize,
    terminology: TermMatchIndex,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct BatchPlanMetrics {
    pub(crate) segment_count: usize,
    pub(crate) planned_batches: usize,
    pub(crate) token_evaluations: usize,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct SegmentPlanMetrics {
    pub(crate) planned_batches: usize,
    pub(crate) token_evaluations: usize,
}

pub(crate) use super::batch_segments::PendingSegment;

pub(crate) fn build_pending_segments(
    all_units: &[TranslationUnit],
    pending_indices: &[usize],
) -> Vec<PendingSegment> {
    super::segment_groups::merge_small_segments(
        all_units,
        super::batch_segments::build_segments(all_units, pending_indices),
    )
}

pub(crate) fn create_planning_context(
    all_units: &[TranslationUnit],
    spans: &[TranslationSpan],
    model: &str,
    system_prompt: &str,
    target_input_tokens: usize,
    batching_strategy: BatchingStrategy,
    terminology: TermMatchIndex,
) -> Result<PlanningContext, prompting::PromptError> {
    let profile = BatchStrategyProfile::for_strategy(batching_strategy);
    let target_prompt_body_tokens =
        effective_prompt_budget(target_input_tokens, terminology.has_terms());
    let hard_prompt_body_tokens = hard_prompt_budget(
        target_input_tokens,
        batching_strategy,
        terminology.has_terms(),
    );
    Ok(PlanningContext {
        token_counter: prompting::PromptTokenCounter::new(model, system_prompt)?,
        prompt_seeds: prompting::build_prompt_render_seeds(all_units, spans),
        strategy: batching_strategy,
        profile,
        target_prompt_body_tokens,
        hard_prompt_body_tokens,
        terminology,
    })
}

#[cfg(test)]
pub(crate) fn plan_translation_batches(
    all_units: &[TranslationUnit],
    spans: &[TranslationSpan],
    pending_indices: &[usize],
    model: &str,
    system_prompt: &str,
    target_input_tokens: usize,
) -> Result<Vec<PlannedBatch>, prompting::PromptError> {
    let (batches, _) = plan_translation_batches_with_terms_and_reporter(
        all_units,
        spans,
        pending_indices,
        model,
        system_prompt,
        target_input_tokens,
        BatchingStrategy::MaximizeUtilization,
        TermMatchIndex::default(),
        None,
    )?;
    Ok(batches)
}

#[allow(dead_code)]
pub(crate) fn plan_translation_batches_with_reporter(
    all_units: &[TranslationUnit],
    spans: &[TranslationSpan],
    pending_indices: &[usize],
    model: &str,
    system_prompt: &str,
    target_input_tokens: usize,
    batching_strategy: BatchingStrategy,
    reporter: Option<(&WorkflowReporter, WorkflowEventPhase)>,
) -> Result<(Vec<PlannedBatch>, BatchPlanMetrics), prompting::PromptError> {
    plan_translation_batches_with_terms_and_reporter(
        all_units,
        spans,
        pending_indices,
        model,
        system_prompt,
        target_input_tokens,
        batching_strategy,
        TermMatchIndex::default(),
        reporter,
    )
}

pub(crate) fn plan_translation_batches_with_terms_and_reporter(
    all_units: &[TranslationUnit],
    spans: &[TranslationSpan],
    pending_indices: &[usize],
    model: &str,
    system_prompt: &str,
    target_input_tokens: usize,
    batching_strategy: BatchingStrategy,
    terminology: TermMatchIndex,
    reporter: Option<(&WorkflowReporter, WorkflowEventPhase)>,
) -> Result<(Vec<PlannedBatch>, BatchPlanMetrics), prompting::PromptError> {
    let segments = build_pending_segments(all_units, pending_indices);
    let raw_segment_count = segments
        .iter()
        .map(|segment| segment.source_segment_count)
        .sum::<usize>();
    let segment_count = segments.len();
    let planning_context = create_planning_context(
        all_units,
        spans,
        model,
        system_prompt,
        target_input_tokens,
        batching_strategy,
        terminology,
    )?;
    if let Some((reporter, phase)) = reporter {
        reporter.info_key(
            phase,
            "workflow.estimate.planBatches",
            "Planning translation batches",
            Some(format!(
                "正在规划正文请求：{} 条待翻译文本，整理为 {} 个规划片段（原始片段 {} 个）",
                pending_indices.len(),
                segment_count,
                raw_segment_count
            )),
        );
    }

    let (batches, metrics) =
        collect_planned_batches(all_units, &segments, &planning_context, reporter)?;
    if let Some((reporter, phase)) = reporter {
        reporter.info_key(
            phase,
            "workflow.estimate.planDone",
            "Batch planning complete",
            Some(format!(
                "正文请求规划完成：{} 个请求，{} 个规划片段，共做 {} 次 Token 试算",
                metrics.planned_batches,
                metrics.segment_count,
                metrics.token_evaluations
            )),
        );
    }

    Ok((batches, metrics))
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn plan_batches_for_segment(
    all_units: &[TranslationUnit],
    segment: &PendingSegment,
    planning_context: &PlanningContext,
    reporter: Option<SegmentReporterContext<'_>>,
) -> Result<(Vec<PlannedBatch>, SegmentPlanMetrics), prompting::PromptError> {
    plan_single_segment_batches(
        all_units,
        segment,
        planning_context,
        reporter,
        reporter
            .map(|context| context.segment_index)
            .unwrap_or_default(),
    )
}

pub(crate) fn plan_single_segment_batches(
    all_units: &[TranslationUnit],
    segment: &PendingSegment,
    planning_context: &PlanningContext,
    reporter: Option<SegmentReporterContext<'_>>,
    segment_index: usize,
) -> Result<(Vec<PlannedBatch>, SegmentPlanMetrics), prompting::PromptError> {
    let started_at = Instant::now();
    let mut metrics = SegmentPlanMetrics::default();
    let mut batches = Vec::new();
    let mut current_indices = Vec::new();
    let mut current_units = Vec::new();
    let mut current_token_count = 0usize;

    emit_segment_start(reporter, segment_index, 1, segment.indices.len(), segment.source_segment_count);

    for index in segment.indices.iter().copied() {
        if current_units.len() >= planning_context.profile.max_items_per_request {
            batches.push(PlannedBatch::single_segment(
                segment_index,
                segment.file.clone(),
                current_indices,
                current_units,
                BatchFlushReason::ItemLimit,
            ));
            current_indices = Vec::new();
            current_units = Vec::new();
        }

        current_indices.push(index);
        current_units.push(all_units[index].clone());
        let candidate_tokens =
            planning_context.count_batch_tokens(&[current_indices.as_slice()], &current_units)?;
        metrics.token_evaluations += 1;
        let should_split_for_hard_cap =
            current_units.len() > 1 && candidate_tokens > planning_context.hard_prompt_body_tokens;
        let should_split_for_target = current_units.len() > 1
            && current_token_count > 0
            && candidate_tokens >= planning_context.target_prompt_body_tokens
            && candidate_tokens.saturating_sub(planning_context.target_prompt_body_tokens)
                > planning_context
                    .target_prompt_body_tokens
                    .saturating_sub(current_token_count);
        if should_split_for_hard_cap || should_split_for_target {
            let overflow_index = current_indices.pop().expect("candidate index present");
            let overflow_unit = current_units.pop().expect("candidate unit present");
            batches.push(PlannedBatch::single_segment(
                segment_index,
                segment.file.clone(),
                current_indices,
                current_units,
                if should_split_for_hard_cap {
                    BatchFlushReason::WouldExceedHardCap
                } else {
                    BatchFlushReason::TargetReached
                },
            ));
            current_indices = vec![overflow_index];
            current_units = vec![overflow_unit];
            current_token_count =
                planning_context.count_batch_tokens(&[current_indices.as_slice()], &current_units)?;
            metrics.token_evaluations += 1;
        } else {
            current_token_count = candidate_tokens;
        }
    }

    if !current_units.is_empty() {
        batches.push(PlannedBatch::single_segment(
            segment_index,
            segment.file.clone(),
            current_indices,
            current_units,
            BatchFlushReason::FinalFlush,
        ));
    }

    metrics.planned_batches = batches.len();
    emit_segment_complete(
        reporter,
        segment_index,
        1,
        segment.indices.len(),
        segment.source_segment_count,
        metrics,
        started_at,
        PlannedBatchDescriptor::SingleSegment,
    );
    Ok((batches, metrics))
}

pub(crate) fn plan_grouped_pool_batches(
    all_units: &[TranslationUnit],
    segments: &[&PendingSegment],
    segment_indexes: &[usize],
    planning_context: &PlanningContext,
    terminal_flush_reason: BatchFlushReason,
    reporter: Option<SegmentReporterContext<'_>>,
) -> Result<(Vec<PlannedBatch>, SegmentPlanMetrics), prompting::PromptError> {
    let started_at = Instant::now();
    let total_units = segments.iter().map(|segment| segment.indices.len()).sum::<usize>();
    let total_raw_segments = segments
        .iter()
        .map(|segment| segment.source_segment_count)
        .sum::<usize>();
    let mut metrics = SegmentPlanMetrics::default();
    let mut batches = Vec::new();
    let mut current_groups = Vec::<PlannedBatchGroup>::new();
    let mut current_token_count = 0usize;

    emit_segment_start(
        reporter,
        reporter.map(|context| context.segment_index).unwrap_or_default(),
        segments.len(),
        total_units,
        total_raw_segments,
    );

    for (offset, segment) in segments.iter().enumerate() {
        let group = PlannedBatchGroup {
            segment_index: segment_indexes.get(offset).copied().unwrap_or(offset),
            file: segment.file.clone(),
            indices: segment.indices.clone(),
            units: segment
                .indices
                .iter()
                .map(|index| all_units[*index].clone())
                .collect(),
        };

        let exceed_items = current_groups
            .iter()
            .flat_map(|planned| planned.units.iter())
            .count()
            + group.units.len()
            > planning_context.profile.max_items_per_request;
        let exceed_groups =
            current_groups.len() >= planning_context.profile.max_groups_per_grouped_batch;
        let exceed_files = unique_files_len(&current_groups, Some(&group))
            > planning_context.profile.max_grouped_orphan_files;
        let candidate_tokens = if current_groups.is_empty() {
            0
        } else {
            let candidate = current_groups
                .iter()
                .cloned()
                .chain(std::iter::once(group.clone()))
                .collect::<Vec<_>>();
            planning_context.count_grouped_tokens(&candidate)?
        };
        if !current_groups.is_empty() {
            metrics.token_evaluations += 1;
        }
        let exceed_hard_cap =
            !current_groups.is_empty() && candidate_tokens > planning_context.hard_prompt_body_tokens;
        let exceed_target = !current_groups.is_empty()
            && current_token_count > 0
            && candidate_tokens >= planning_context.target_prompt_body_tokens
            && candidate_tokens.saturating_sub(planning_context.target_prompt_body_tokens)
                > planning_context
                    .target_prompt_body_tokens
                    .saturating_sub(current_token_count);

        if exceed_items || exceed_groups || exceed_files || exceed_hard_cap || exceed_target {
            let flush_reason = if exceed_items {
                BatchFlushReason::ItemLimit
            } else if exceed_groups {
                BatchFlushReason::GroupLimit
            } else if exceed_files {
                BatchFlushReason::FileLimit
            } else if exceed_hard_cap {
                BatchFlushReason::WouldExceedHardCap
            } else {
                BatchFlushReason::TargetReached
            };
            batches.push(PlannedBatch::grouped_pool(current_groups, flush_reason));
            current_groups = Vec::new();
            current_token_count = 0;
        }
        current_groups.push(group);
        if current_groups.len() == 1 {
            current_token_count = planning_context.count_grouped_tokens(&current_groups)?;
            metrics.token_evaluations += 1;
        } else if !(exceed_items || exceed_groups || exceed_files || exceed_hard_cap || exceed_target) {
            current_token_count = candidate_tokens;
        }
    }

    if !current_groups.is_empty() {
        batches.push(PlannedBatch::grouped_pool(
            current_groups,
            terminal_flush_reason,
        ));
    }

    metrics.planned_batches = batches.len();
    emit_segment_complete(
        reporter,
        reporter.map(|context| context.segment_index).unwrap_or_default(),
        segments.len(),
        total_units,
        total_raw_segments,
        metrics,
        started_at,
        PlannedBatchDescriptor::GroupedPool,
    );
    Ok((batches, metrics))
}

impl PlanningContext {
    pub(crate) fn glossary_terms_for_units(
        &self,
        units: &[TranslationUnit],
    ) -> Vec<CanonicalTerm> {
        self.terminology
            .batch_terms_for_units(units, MAX_BATCH_GLOSSARY_TERMS)
    }

    pub(crate) fn prepared_prompt_for_batch(
        &self,
        batch: &PlannedBatch,
    ) -> Result<prompting::PreparedUserPrompt, prompting::PromptError> {
        let groups = batch
            .groups
            .iter()
            .map(|group| prompting::PromptSeedGroup {
                seeds: group
                    .indices
                    .iter()
                    .map(|index| &self.prompt_seeds[*index])
                    .collect(),
            })
            .collect::<Vec<_>>();
        let glossary_terms = self.glossary_terms_for_units(&batch.units);
        prompting::build_grouped_user_prompt_from_seed_groups_with_terms(&groups, &glossary_terms)
    }

    pub(crate) fn estimated_input_tokens_for_batch(
        &self,
        batch: &PlannedBatch,
    ) -> Result<usize, prompting::PromptError> {
        let prompt = self.prepared_prompt_for_batch(batch)?;
        Ok(self.token_counter.count_rendered_prompt_body(prompt.body()))
    }

    fn count_batch_tokens(
        &self,
        group_indices: &[&[usize]],
        flat_units: &[TranslationUnit],
    ) -> Result<usize, prompting::PromptError> {
        let groups = group_indices
            .iter()
            .map(|indices| prompting::PromptSeedGroup {
                seeds: indices
                    .iter()
                    .map(|index| &self.prompt_seeds[*index])
                    .collect(),
            })
            .collect::<Vec<_>>();
        let glossary_terms = self.glossary_terms_for_units(flat_units);
        let rendered = prompting::render_grouped_prompt_body_from_seed_groups_with_terms(
            &groups,
            &glossary_terms,
        )?;
        Ok(self.token_counter.count_rendered_prompt_body(&rendered))
    }

    pub(crate) fn count_grouped_tokens(
        &self,
        groups: &[PlannedBatchGroup],
    ) -> Result<usize, prompting::PromptError> {
        let group_indices = groups
            .iter()
            .map(|group| group.indices.as_slice())
            .collect::<Vec<_>>();
        let flat_units = groups
            .iter()
            .flat_map(|group| group.units.iter().cloned())
            .collect::<Vec<_>>();
        self.count_batch_tokens(&group_indices, &flat_units)
    }

    pub(crate) fn target_prompt_body_tokens(&self) -> usize {
        self.target_prompt_body_tokens
    }

    pub(crate) fn hard_prompt_body_tokens(&self) -> usize {
        self.hard_prompt_body_tokens
    }

    pub(crate) fn strategy(&self) -> BatchingStrategy {
        self.strategy
    }

    pub(crate) fn profile(&self) -> BatchStrategyProfile {
        self.profile
    }
}

fn unique_files_len(groups: &[PlannedBatchGroup], next: Option<&PlannedBatchGroup>) -> usize {
    groups
        .iter()
        .map(|group| group.file.clone())
        .chain(next.into_iter().map(|group| group.file.clone()))
        .collect::<BTreeSet<_>>()
        .len()
}

fn emit_segment_start(
    reporter: Option<SegmentReporterContext<'_>>,
    segment_index: usize,
    segment_group_count: usize,
    unit_count: usize,
    raw_segments_merged: usize,
) {
    let Some(context) = reporter else {
        return;
    };
    context.reporter.progress_throttled_key(
        "translate-planning-segments",
        context.phase,
        "workflow.translate.planningProgress",
        "Planning translation segments",
        context.segment_index + 1,
        context.segment_count.max(1),
        Some(format!(
            "正在规划第 {} / {} 个片段，当前片段含 {} 条文本，累计已规划 {} 个请求",
            context.segment_index + 1,
            context.segment_count.max(1),
            unit_count,
            context.planned_batches_so_far
        )),
    );
    context.reporter.debug(
        context.phase,
        "Planning segment window",
        Some(format!(
            "Segment {}/{} with {} unit(s) across {} group(s)",
            segment_index + 1,
            context.segment_count.max(1),
            unit_count,
            segment_group_count
        )),
        [
            ("segment_index", (segment_index + 1).to_string()),
            ("segment_count", context.segment_count.to_string()),
            ("segment_units", unit_count.to_string()),
            ("window_group_count", segment_group_count.to_string()),
            ("raw_segments_merged", raw_segments_merged.to_string()),
        ],
    );
}

fn emit_segment_complete(
    reporter: Option<SegmentReporterContext<'_>>,
    segment_index: usize,
    segment_group_count: usize,
    unit_count: usize,
    raw_segments_merged: usize,
    metrics: SegmentPlanMetrics,
    started_at: Instant,
    descriptor: PlannedBatchDescriptor,
) {
    let Some(context) = reporter else {
        return;
    };
    context.reporter.debug(
        context.phase,
        "Planning segment complete",
        Some(format!(
            "Segment {}/{} complete in {} ms",
            segment_index + 1,
            context.segment_count.max(1),
            started_at.elapsed().as_millis()
        )),
        [
            ("segment_index", (segment_index + 1).to_string()),
            ("segment_count", context.segment_count.to_string()),
            ("segment_units", unit_count.to_string()),
            ("window_group_count", segment_group_count.to_string()),
            ("segment_batches", metrics.planned_batches.to_string()),
            ("raw_segments_merged", raw_segments_merged.to_string()),
            ("segment_token_evaluations", metrics.token_evaluations.to_string()),
            ("segment_elapsed_ms", started_at.elapsed().as_millis().to_string()),
            ("window_kind", descriptor.as_str().to_owned()),
        ],
    );
}

#[derive(Clone, Copy)]
enum PlannedBatchDescriptor {
    SingleSegment,
    GroupedPool,
}

impl PlannedBatchDescriptor {
    fn as_str(self) -> &'static str {
        match self {
            PlannedBatchDescriptor::SingleSegment => "single_segment",
            PlannedBatchDescriptor::GroupedPool => "grouped_pool",
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct SegmentReporterContext<'a> {
    pub(crate) reporter: &'a WorkflowReporter,
    pub(crate) phase: WorkflowEventPhase,
    pub(crate) segment_index: usize,
    pub(crate) segment_count: usize,
    pub(crate) planned_batches_so_far: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ContextEnvelope, TranslationStatus};

    #[test]
    fn token_budget_splits_before_hidden_item_limit() {
        let units = vec![
            event_unit("a", Some(1), Some(1), Some(0), &"alpha beta gamma delta ".repeat(80)),
            event_unit("b", Some(1), Some(1), Some(0), &"alpha beta gamma delta ".repeat(80)),
            event_unit("c", Some(1), Some(1), Some(0), &"alpha beta gamma delta ".repeat(80)),
        ];
        let pending = vec![0, 1, 2];

        let batches = plan_translation_batches(&units, &[], &pending, "gpt-4.1-mini", "", 320)
            .expect("plan batches");

        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].indices, vec![0]);
    }

    #[test]
    fn hidden_item_limit_still_caps_tiny_entries() {
        let units = (0..80)
            .map(|index| field_unit(&format!("u{index}"), "Map001.json", "a"))
            .collect::<Vec<_>>();
        let pending = (0..80).collect::<Vec<_>>();

        let batches = plan_translation_batches(&units, &[], &pending, "gpt-4.1-mini", "", 100_000)
            .expect("plan batches");

        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].indices.len(), 80);
    }

    #[test]
    fn quality_first_keeps_smaller_hidden_item_limit() {
        let units = (0..80)
            .map(|index| field_unit(&format!("u{index}"), "Map001.json", "a"))
            .collect::<Vec<_>>();
        let pending = (0..80).collect::<Vec<_>>();

        let (batches, _) = plan_translation_batches_with_terms_and_reporter(
            &units,
            &[],
            &pending,
            "gpt-4.1-mini",
            "",
            100_000,
            BatchingStrategy::QualityFirst,
            TermMatchIndex::default(),
            None,
        )
        .expect("plan batches");

        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].indices.len(), 64);
        assert_eq!(batches[1].indices.len(), 16);
    }

    #[test]
    fn different_event_pages_do_not_share_batch_even_with_large_budget() {
        let units = vec![
            event_unit("a", Some(1), Some(1), Some(0), "A"),
            event_unit("b", Some(1), Some(1), Some(0), "B"),
            event_unit("c", Some(1), Some(2), Some(0), "C"),
        ];
        let pending = vec![0, 1, 2];

        let batches = plan_translation_batches(&units, &[], &pending, "gpt-4.1-mini", "", 100_000)
            .expect("plan batches");

        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].indices, vec![0, 1]);
        assert_eq!(batches[1].indices, vec![2]);
    }

    #[test]
    fn field_entries_from_same_directory_can_share_grouped_batch() {
        let units = vec![
            field_unit("a", "www/data/System.json", "Potion"),
            field_unit("b", "www/data/System.json", "Ether"),
            field_unit("c", "www/data/Items.json", "Sword"),
        ];
        let pending = vec![0, 1, 2];

        let batches = plan_translation_batches(&units, &[], &pending, "gpt-4.1-mini", "", 100_000)
            .expect("plan batches");

        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].group_count(), 2);
        assert_eq!(batches[0].kind.as_str(), "grouped_pool");
    }

    #[test]
    fn maximize_utilization_can_pool_larger_non_orphan_segments() {
        let mut units = Vec::new();
        for index in 0..16 {
            units.push(field_unit(
                &format!("actors-{index}"),
                "www/data/Actors.json",
                &format!("Actor {index}"),
            ));
        }
        for index in 0..16 {
            units.push(field_unit(
                &format!("classes-{index}"),
                "www/data/Classes.json",
                &format!("Class {index}"),
            ));
        }
        let pending = (0..units.len()).collect::<Vec<_>>();

        let batches = plan_translation_batches(&units, &[], &pending, "gpt-4.1-mini", "", 100_000)
            .expect("plan batches");

        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].group_count(), 2);
        assert_eq!(batches[0].kind.as_str(), "grouped_pool");
        assert_eq!(batches[0].source_segment_count(), 2);
    }

    #[test]
    fn quality_first_keeps_larger_non_orphan_segments_separate() {
        let mut units = Vec::new();
        for index in 0..16 {
            units.push(field_unit(
                &format!("actors-{index}"),
                "www/data/Actors.json",
                &format!("Actor {index}"),
            ));
        }
        for index in 0..16 {
            units.push(field_unit(
                &format!("classes-{index}"),
                "www/data/Classes.json",
                &format!("Class {index}"),
            ));
        }
        let pending = (0..units.len()).collect::<Vec<_>>();

        let (batches, _) = plan_translation_batches_with_terms_and_reporter(
            &units,
            &[],
            &pending,
            "gpt-4.1-mini",
            "",
            100_000,
            BatchingStrategy::QualityFirst,
            TermMatchIndex::default(),
            None,
        )
        .expect("plan batches");

        assert_eq!(batches.len(), 2);
        assert!(batches.iter().all(|batch| batch.kind.as_str() == "single_segment"));
    }

    #[test]
    fn maximize_utilization_can_pool_small_common_event_segments() {
        let units = vec![
            common_event_unit(
                "a",
                "www/data/CommonEvents.json",
                Some(0),
                Some(1),
                Some(0),
                "first short block",
            ),
            common_event_unit(
                "b",
                "www/data/CommonEvents.json",
                Some(0),
                Some(2),
                Some(0),
                "second short block",
            ),
        ];
        let pending = vec![0, 1];

        let batches = plan_translation_batches(&units, &[], &pending, "gpt-4.1-mini", "", 100_000)
            .expect("plan batches");

        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].kind.as_str(), "grouped_pool");
        assert_eq!(batches[0].group_count(), 2);
    }

    #[test]
    fn quality_first_keeps_small_common_event_segments_separate() {
        let units = vec![
            common_event_unit(
                "a",
                "www/data/CommonEvents.json",
                Some(0),
                Some(1),
                Some(0),
                "first short block",
            ),
            common_event_unit(
                "b",
                "www/data/CommonEvents.json",
                Some(0),
                Some(2),
                Some(0),
                "second short block",
            ),
        ];
        let pending = vec![0, 1];

        let (batches, _) = plan_translation_batches_with_terms_and_reporter(
            &units,
            &[],
            &pending,
            "gpt-4.1-mini",
            "",
            100_000,
            BatchingStrategy::QualityFirst,
            TermMatchIndex::default(),
            None,
        )
        .expect("plan batches");

        assert_eq!(batches.len(), 2);
        assert!(batches.iter().all(|batch| batch.kind.as_str() == "single_segment"));
    }

    #[test]
    fn scene_segment_dispatches_before_buffered_orphans() {
        let units = vec![
            field_unit("a", "www/data/CommonEvents.json", "3章 猪鹿蝶の梅の間で 0253"),
            event_unit("b", Some(1), Some(1), Some(0), "dialogue block"),
            field_unit("c", "www/data/Actors.json", "ロマ夏"),
        ];
        let pending = vec![0, 1, 2];

        let batches = plan_translation_batches(&units, &[], &pending, "gpt-4.1-mini", "", 100_000)
            .expect("plan batches");

        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].kind.as_str(), "single_segment");
        assert_eq!(batches[0].groups[0].segment_index, 1);
        assert_eq!(batches[1].kind.as_str(), "grouped_pool");
        assert_eq!(batches[1].group_count(), 2);
        assert_eq!(batches[1].groups[0].segment_index, 0);
        assert_eq!(batches[1].groups[1].segment_index, 2);
    }

    fn unit(
        id: &str,
        file: &str,
        source_text: String,
        notes: Vec<&str>,
        map_id: Option<u32>,
        event_id: Option<u32>,
        page_id: Option<u32>,
        semantic_kind: &str,
    ) -> TranslationUnit {
        TranslationUnit {
            id: id.to_owned(),
            group_id: id.to_owned(),
            semantic_kind: semantic_kind.to_owned(),
            context: ContextEnvelope {
                file: file.to_owned(),
                json_path: Some("$.events.1".to_owned()),
                map_id,
                event_id,
                page_id,
                command_index: None,
                speaker_name: None,
                prev_texts: Vec::new(),
                next_texts: Vec::new(),
                block_text: None,
                glossary_hits: Vec::new(),
                notes: notes.into_iter().map(str::to_owned).collect(),
            },
            source_text,
            translated_text: None,
            status: TranslationStatus::Pending,
            span_ids: Vec::new(),
        }
    }

    fn field_unit(id: &str, file: &str, source_text: &str) -> TranslationUnit {
        unit(
            id,
            file,
            source_text.to_owned(),
            vec!["field_extraction"],
            None,
            None,
            None,
            "name",
        )
    }

    fn event_unit(
        id: &str,
        map_id: Option<u32>,
        event_id: Option<u32>,
        page_id: Option<u32>,
        source_text: &str,
    ) -> TranslationUnit {
        unit(
            id,
            "Map001.json",
            source_text.to_owned(),
            vec!["event_dialogue_block"],
            map_id,
            event_id,
            page_id,
            "dialogue",
        )
    }

    fn common_event_unit(
        id: &str,
        file: &str,
        map_id: Option<u32>,
        event_id: Option<u32>,
        page_id: Option<u32>,
        source_text: &str,
    ) -> TranslationUnit {
        unit(
            id,
            file,
            source_text.to_owned(),
            vec!["event_dialogue_block"],
            map_id,
            event_id,
            page_id,
            "dialogue",
        )
    }
}
