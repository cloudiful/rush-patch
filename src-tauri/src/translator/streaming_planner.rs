use super::batch_segments::PendingSegment;
use super::batching::{
    BatchPlanMetrics, PlanningContext, SegmentPlanMetrics, SegmentReporterContext,
    plan_grouped_pool_batches, plan_single_segment_batches,
};
use super::orphan_grouping::{segment_pool_candidate_rejection, segment_pool_key};
use super::planned_batch::PlannedBatchGroup;
use super::planned_batch::{BatchFlushReason, PlannedBatch};
use crate::domain::{TranslationUnit, WorkflowEventPhase};
use crate::prompting;
use crate::workflow_events::WorkflowReporter;

#[derive(Debug, Clone)]
pub(crate) struct PlannedDispatch {
    pub(crate) anchor_segment_index: usize,
    pub(crate) consumed_segment_indexes: Vec<usize>,
    pub(crate) batches: Vec<PlannedBatch>,
    pub(crate) metrics: SegmentPlanMetrics,
}

pub(crate) struct StreamingBatchPlanner<'a> {
    all_units: &'a [TranslationUnit],
    segments: &'a [PendingSegment],
    planning_context: &'a PlanningContext,
    reporter: Option<(&'a WorkflowReporter, WorkflowEventPhase)>,
    next_segment_index: usize,
    orphan_buffer: Vec<usize>,
    orphan_pool_directory: Option<String>,
    emitted_batches: usize,
    metrics: BatchPlanMetrics,
}

impl<'a> StreamingBatchPlanner<'a> {
    pub(crate) fn new(
        all_units: &'a [TranslationUnit],
        segments: &'a [PendingSegment],
        planning_context: &'a PlanningContext,
        reporter: Option<(&'a WorkflowReporter, WorkflowEventPhase)>,
    ) -> Self {
        Self {
            all_units,
            segments,
            planning_context,
            reporter,
            next_segment_index: 0,
            orphan_buffer: Vec::new(),
            orphan_pool_directory: None,
            emitted_batches: 0,
            metrics: BatchPlanMetrics {
                segment_count: segments.len(),
                ..BatchPlanMetrics::default()
            },
        }
    }

    pub(crate) fn metrics(&self) -> BatchPlanMetrics {
        BatchPlanMetrics {
            segment_count: self.metrics.segment_count,
            planned_batches: self.metrics.planned_batches,
            token_evaluations: self.metrics.token_evaluations,
        }
    }

    pub(crate) fn next_dispatch(
        &mut self,
    ) -> Result<Option<PlannedDispatch>, prompting::PromptError> {
        loop {
            if self.next_segment_index >= self.segments.len() {
                return self.flush_orphan_buffer(BatchFlushReason::FinalFlush);
            }

            let segment_index = self.next_segment_index;
            let segment = &self.segments[segment_index];
            if let Some(reason) = self.buffer_rejection_reason(segment_index)? {
                self.log_pool_rejection(segment_index, &reason);
                self.next_segment_index += 1;
                return self.plan_single_segment_dispatch(segment_index);
            }

            let current_directory = segment_pool_key(segment, self.planning_context.strategy());
            if let Some(pool_directory) = self.orphan_pool_directory.as_deref() {
                let oldest_segment_index = self.orphan_buffer.first().copied().unwrap_or(segment_index);
                let wait_age = segment_index.saturating_sub(oldest_segment_index);
                if pool_directory != current_directory {
                    return self.flush_orphan_buffer(BatchFlushReason::DirectoryBoundary);
                }
                if wait_age >= self.planning_context.profile().orphan_pool_max_wait_segments {
                    return self.flush_orphan_buffer(BatchFlushReason::WaitAgeLimit);
                }
            }

            if self.orphan_buffer.is_empty() {
                self.orphan_pool_directory = Some(current_directory);
            }
            self.orphan_buffer.push(segment_index);
            self.next_segment_index += 1;

            if self.should_flush_orphan_buffer()? {
                return self.flush_orphan_buffer(BatchFlushReason::TargetReached);
            }
        }
    }

    fn should_flush_orphan_buffer(&mut self) -> Result<bool, prompting::PromptError> {
        if self.orphan_buffer.is_empty() {
            return Ok(false);
        }
        let token_count = self
            .planning_context
            .count_grouped_tokens(&self.buffered_groups())?;
        self.metrics.token_evaluations += 1;
        Ok(token_count >= self.planning_context.target_prompt_body_tokens())
    }

    fn flush_orphan_buffer(
        &mut self,
        flush_reason: BatchFlushReason,
    ) -> Result<Option<PlannedDispatch>, prompting::PromptError> {
        if self.orphan_buffer.is_empty() {
            return Ok(None);
        }

        let consumed_segment_indexes = std::mem::take(&mut self.orphan_buffer);
        self.orphan_pool_directory = None;
        let grouped_segments = consumed_segment_indexes
            .iter()
            .map(|index| &self.segments[*index])
            .collect::<Vec<_>>();
        let anchor_segment_index = consumed_segment_indexes[0];
        let (batches, metrics) = plan_grouped_pool_batches(
            self.all_units,
            &grouped_segments,
            &consumed_segment_indexes,
            self.planning_context,
            flush_reason,
            self.reporter_context(anchor_segment_index),
        )?;
        self.metrics.token_evaluations += metrics.token_evaluations;
        self.metrics.planned_batches += batches.len();
        self.emitted_batches += batches.len();
        Ok(Some(PlannedDispatch {
            anchor_segment_index,
            consumed_segment_indexes,
            batches,
            metrics,
        }))
    }

    fn plan_single_segment_dispatch(
        &mut self,
        segment_index: usize,
    ) -> Result<Option<PlannedDispatch>, prompting::PromptError> {
        let segment = &self.segments[segment_index];
        let (batches, metrics) = plan_single_segment_batches(
            self.all_units,
            segment,
            self.planning_context,
            self.reporter_context(segment_index),
            segment_index,
        )?;
        self.metrics.token_evaluations += metrics.token_evaluations;
        self.metrics.planned_batches += batches.len();
        self.emitted_batches += batches.len();
        Ok(Some(PlannedDispatch {
            anchor_segment_index: segment_index,
            consumed_segment_indexes: vec![segment_index],
            batches,
            metrics,
        }))
    }

    fn reporter_context(
        &self,
        segment_index: usize,
    ) -> Option<SegmentReporterContext<'a>> {
        self.reporter.map(|(reporter, phase)| SegmentReporterContext {
            reporter,
            phase,
            segment_index,
            segment_count: self.segments.len(),
            planned_batches_so_far: self.emitted_batches,
        })
    }

    fn buffered_groups(&self) -> Vec<super::planned_batch::PlannedBatchGroup> {
        self.orphan_buffer
            .iter()
            .map(|segment_index| {
                let segment = &self.segments[*segment_index];
                super::planned_batch::PlannedBatchGroup {
                    segment_index: *segment_index,
                    file: segment.file.clone(),
                    indices: segment.indices.clone(),
                    units: segment
                        .indices
                        .iter()
                        .map(|index| self.all_units[*index].clone())
                        .collect(),
                }
            })
            .collect()
    }

    fn buffer_rejection_reason(
        &mut self,
        segment_index: usize,
    ) -> Result<Option<String>, prompting::PromptError> {
        let segment = &self.segments[segment_index];
        if let Some(reason) = segment_pool_candidate_rejection(
            self.all_units,
            segment,
            self.planning_context.strategy(),
        ) {
            return Ok(Some(reason));
        }
        if segment.indices.len() > self.planning_context.profile().max_items_per_request {
            return Ok(Some(format!(
                "item_limit>{}",
                self.planning_context.profile().max_items_per_request
            )));
        }

        let segment_tokens = self
            .planning_context
            .count_grouped_tokens(&[self.segment_group(segment_index)])?;
        self.metrics.token_evaluations += 1;
        if segment_tokens > self.planning_context.hard_prompt_body_tokens() {
            return Ok(Some(format!(
                "segment_tokens>{}",
                self.planning_context.hard_prompt_body_tokens()
            )));
        }

        Ok(None)
    }

    fn segment_group(&self, segment_index: usize) -> PlannedBatchGroup {
        let segment = &self.segments[segment_index];
        PlannedBatchGroup {
            segment_index,
            file: segment.file.clone(),
            indices: segment.indices.clone(),
            units: segment
                .indices
                .iter()
                .map(|index| self.all_units[*index].clone())
                .collect(),
        }
    }

    fn log_pool_rejection(&self, segment_index: usize, reason: &str) {
        let Some((reporter, phase)) = self.reporter else {
            return;
        };
        let segment = &self.segments[segment_index];
        reporter.debug(
            phase,
            "Segment not eligible for grouped pool",
            Some(format!(
                "Segment {}/{} will stay single_segment",
                segment_index + 1,
                self.segments.len().max(1)
            )),
            [
                ("segment_index", (segment_index + 1).to_string()),
                ("reason", reason.to_owned()),
                ("segment_kind", format!("{:?}", segment.kind)),
                ("unit_count", segment.indices.len().to_string()),
                ("source_chars", segment.source_char_count.to_string()),
                ("file", segment.file.clone()),
            ],
        );
    }
}

pub(crate) fn collect_planned_batches(
    all_units: &[TranslationUnit],
    segments: &[PendingSegment],
    planning_context: &PlanningContext,
    reporter: Option<(&WorkflowReporter, WorkflowEventPhase)>,
) -> Result<(Vec<PlannedBatch>, BatchPlanMetrics), prompting::PromptError> {
    let mut planner = StreamingBatchPlanner::new(all_units, segments, planning_context, reporter);
    let mut batches = Vec::new();
    while let Some(dispatch) = planner.next_dispatch()? {
        batches.extend(dispatch.batches);
    }
    Ok((batches, planner.metrics()))
}
