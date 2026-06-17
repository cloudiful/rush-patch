use crate::domain::BatchingStrategy;

const GLOSSARY_TOKEN_RESERVE: usize = 128;

#[derive(Debug, Clone, Copy)]
pub(crate) struct BatchStrategyProfile {
    pub(crate) max_items_per_request: usize,
    pub(crate) max_groups_per_grouped_batch: usize,
    pub(crate) max_grouped_orphan_files: usize,
    pub(crate) orphan_pool_max_wait_segments: usize,
}

impl BatchStrategyProfile {
    pub(crate) fn for_strategy(strategy: BatchingStrategy) -> Self {
        match strategy {
            BatchingStrategy::MaximizeUtilization => Self {
                max_items_per_request: 256,
                max_groups_per_grouped_batch: 96,
                max_grouped_orphan_files: 96,
                orphan_pool_max_wait_segments: 2_048,
            },
            BatchingStrategy::QualityFirst => Self {
                max_items_per_request: 64,
                max_groups_per_grouped_batch: 8,
                max_grouped_orphan_files: 4,
                orphan_pool_max_wait_segments: 48,
            },
        }
    }
}

pub(crate) fn effective_prompt_budget(max_input_tokens: usize, has_terminology: bool) -> usize {
    if has_terminology && max_input_tokens > GLOSSARY_TOKEN_RESERVE {
        max_input_tokens - GLOSSARY_TOKEN_RESERVE
    } else {
        max_input_tokens.max(1)
    }
}

pub(crate) fn hard_prompt_budget(
    target_input_tokens: usize,
    strategy: BatchingStrategy,
    has_terminology: bool,
) -> usize {
    let padded_target = match strategy {
        BatchingStrategy::MaximizeUtilization => {
            target_input_tokens.saturating_add(std::cmp::max(2_048, target_input_tokens / 10))
        }
        BatchingStrategy::QualityFirst => target_input_tokens.saturating_add(512),
    };
    effective_prompt_budget(padded_target, has_terminology)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maximize_profile_is_less_conservative() {
        let maximize = BatchStrategyProfile::for_strategy(BatchingStrategy::MaximizeUtilization);
        let quality = BatchStrategyProfile::for_strategy(BatchingStrategy::QualityFirst);

        assert!(maximize.max_items_per_request > quality.max_items_per_request);
        assert!(maximize.max_groups_per_grouped_batch > quality.max_groups_per_grouped_batch);
        assert!(maximize.max_grouped_orphan_files > quality.max_grouped_orphan_files);
        assert!(maximize.orphan_pool_max_wait_segments > quality.orphan_pool_max_wait_segments);
    }

    #[test]
    fn maximize_hard_cap_allows_more_headroom() {
        let maximize = hard_prompt_budget(32_000, BatchingStrategy::MaximizeUtilization, false);
        let quality = hard_prompt_budget(32_000, BatchingStrategy::QualityFirst, false);

        assert!(maximize > quality);
    }
}
