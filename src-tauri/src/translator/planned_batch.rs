use crate::domain::TranslationUnit;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlannedBatchKind {
    SingleSegment,
    GroupedPool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BatchFlushReason {
    TargetReached,
    WouldExceedHardCap,
    ItemLimit,
    GroupLimit,
    FileLimit,
    DirectoryBoundary,
    WaitAgeLimit,
    FinalFlush,
}

impl BatchFlushReason {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            BatchFlushReason::TargetReached => "target_reached",
            BatchFlushReason::WouldExceedHardCap => "would_exceed_hard_cap",
            BatchFlushReason::ItemLimit => "item_limit",
            BatchFlushReason::GroupLimit => "group_limit",
            BatchFlushReason::FileLimit => "file_limit",
            BatchFlushReason::DirectoryBoundary => "directory_boundary",
            BatchFlushReason::WaitAgeLimit => "wait_age_limit",
            BatchFlushReason::FinalFlush => "final_flush",
        }
    }
}

impl PlannedBatchKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            PlannedBatchKind::SingleSegment => "single_segment",
            PlannedBatchKind::GroupedPool => "grouped_pool",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedBatchGroup {
    pub(crate) segment_index: usize,
    pub(crate) file: String,
    pub(crate) indices: Vec<usize>,
    pub(crate) units: Vec<TranslationUnit>,
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedBatch {
    pub(crate) kind: PlannedBatchKind,
    pub(crate) flush_reason: BatchFlushReason,
    pub(crate) groups: Vec<PlannedBatchGroup>,
    pub(crate) indices: Vec<usize>,
    pub(crate) units: Vec<TranslationUnit>,
}

impl PlannedBatch {
    pub(crate) fn single_segment(
        segment_index: usize,
        file: String,
        indices: Vec<usize>,
        units: Vec<TranslationUnit>,
        flush_reason: BatchFlushReason,
    ) -> Self {
        Self {
            kind: PlannedBatchKind::SingleSegment,
            flush_reason,
            groups: vec![PlannedBatchGroup {
                segment_index,
                file,
                indices: indices.clone(),
                units: units.clone(),
            }],
            indices,
            units,
        }
    }

    pub(crate) fn grouped_pool(
        groups: Vec<PlannedBatchGroup>,
        flush_reason: BatchFlushReason,
    ) -> Self {
        let mut indices = Vec::new();
        let mut units = Vec::new();
        for group in &groups {
            indices.extend(group.indices.iter().copied());
            units.extend(group.units.iter().cloned());
        }
        Self {
            kind: PlannedBatchKind::GroupedPool,
            flush_reason,
            groups,
            indices,
            units,
        }
    }

    pub(crate) fn group_count(&self) -> usize {
        self.groups.len()
    }

    pub(crate) fn source_segment_count(&self) -> usize {
        self.groups.len()
    }

    pub(crate) fn source_files(&self) -> Vec<String> {
        self.groups
            .iter()
            .map(|group| group.file.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub(crate) fn source_file_count(&self) -> usize {
        self.source_files().len()
    }

    pub(crate) fn anchor_segment_index(&self) -> usize {
        self.groups
            .first()
            .map(|group| group.segment_index)
            .unwrap_or(0)
    }
}
