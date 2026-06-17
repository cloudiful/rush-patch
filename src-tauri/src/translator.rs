mod batch_segments;
mod batch_strategy;
mod batching;
#[cfg(test)]
mod batching_regression_tests;
mod estimate;
#[cfg(test)]
mod estimate_tests;
mod main_translate;
mod orphan_grouping;
mod planned_batch;
mod request;
mod responses_client;
mod responses_sse;
mod run;
mod segment_groups;
mod streaming_planner;
mod term_pretranslate;
#[cfg(test)]
mod tests;

use crate::app_state::CancellationFlag;
use crate::catalog;
use crate::catalog_db::{self, CatalogUnitUpdate};
use crate::domain::{ProjectConfig, TranslationRunSummary, TranslationStatus, TranslationUnit};
use crate::prompting;
use crate::translation_io;
use crate::workflow_events::WorkflowReporter;
use std::collections::BTreeMap;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TranslateError {
    #[error(transparent)]
    Catalog(#[from] catalog::CatalogError),
    #[error(transparent)]
    CatalogDb(#[from] catalog_db::CatalogDbError),
    #[error(transparent)]
    Io(#[from] translation_io::TranslationIoError),
    #[error(transparent)]
    Prompt(#[from] prompting::PromptError),
    #[error("OpenAI API key is required")]
    MissingApiKey,
    #[error("max_input_tokens must be greater than zero")]
    InvalidMaxInputTokens,
    #[error("failed to build OpenAI request: {0}")]
    BuildRequest(String),
    #[error("OpenAI request timed out after {0} seconds")]
    Timeout(u64),
    #[error("OpenAI request failed: {0}")]
    Provider(String),
    #[error("OpenAI response did not contain text content")]
    EmptyProviderResponse,
    #[error("translation cancelled")]
    Cancelled,
}

#[cfg_attr(test, allow(dead_code))]
pub async fn estimate_catalog(
    catalog_path: &Path,
    config: ProjectConfig,
    cancellation: CancellationFlag,
    reporter: &WorkflowReporter,
) -> Result<crate::domain::CatalogTokenEstimate, TranslateError> {
    estimate::estimate_catalog_tokens(catalog_path, &config, &cancellation, reporter).await
}

pub async fn translate_catalog(
    catalog_path: &Path,
    config: ProjectConfig,
    cancellation: CancellationFlag,
    reporter: &WorkflowReporter,
) -> Result<TranslationRunSummary, TranslateError> {
    run::translate_catalog_impl(catalog_path, config, cancellation, reporter).await
}

pub(super) fn unit_updates_for_indices(
    units: &[TranslationUnit],
    indices: &[usize],
) -> Vec<CatalogUnitUpdate> {
    let mut unique = indices.to_vec();
    unique.sort_unstable();
    unique.dedup();
    unique
        .into_iter()
        .filter_map(|index| units.get(index))
        .map(|unit| CatalogUnitUpdate {
            id: unit.id.clone(),
            translated_text: unit.translated_text.clone(),
            status: unit.status.clone(),
        })
        .collect()
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn pending_unit_indices(units: &[TranslationUnit]) -> Vec<usize> {
    units
        .iter()
        .enumerate()
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

pub(super) fn apply_batch_translations(
    units: &mut [TranslationUnit],
    indices: &[usize],
    translations: &BTreeMap<String, String>,
) {
    for index in indices {
        let unit = &mut units[*index];
        let Some(translated) = translations.get(&unit.id) else {
            unit.status = TranslationStatus::Failed;
            continue;
        };

        if translated.trim().is_empty() {
            unit.status = TranslationStatus::Failed;
            continue;
        }

        unit.translated_text = Some(translated.clone());
        unit.status = TranslationStatus::Translated;
    }
}
