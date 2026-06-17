use super::batching::{
    MAX_BATCH_GLOSSARY_TERMS, create_planning_context,
    plan_translation_batches_with_terms_and_reporter,
};
use super::term_pretranslate::plan_term_batches;
use super::TranslateError;
use crate::app_state::CancellationFlag;
use crate::catalog_db;
use crate::domain::{CatalogTokenEstimate, ProjectConfig, WorkflowEventPhase};
use crate::prompting;
use crate::terminology::TermMatchIndex;
use crate::translation_io;
use crate::workflow_events::WorkflowReporter;
use rayon::prelude::*;
use std::collections::BTreeSet;
use std::path::Path;

pub(super) async fn estimate_catalog_tokens(
    catalog_path: &Path,
    config: &ProjectConfig,
    cancellation: &CancellationFlag,
    reporter: &WorkflowReporter,
) -> Result<CatalogTokenEstimate, TranslateError> {
    if config.target_input_tokens == 0 {
        reporter.error_key(
            WorkflowEventPhase::Estimate,
            "workflow.estimate.invalidTargetTokens",
            "Invalid max input tokens",
            Some("目标输入 Token 必须大于 0".to_owned()),
        );
        return Err(TranslateError::InvalidMaxInputTokens);
    }

    let pool = catalog_db::open_pool(catalog_path, false, 4).await?;
    let result = async {
        let loaded = catalog_db::load_catalog_from_pool(&pool).await?;
        let resources = translation_io::load_resources(
            config.glossary_path.as_deref(),
            config.do_not_translate_path.as_deref(),
        )?;
        let system_prompt = prompting::build_system_prompt(config, &resources);
        let response_counter = prompting::PromptTokenCounter::new(&config.model, &system_prompt)?;
        reporter.info_key(
            WorkflowEventPhase::Estimate,
            "workflow.estimate.start",
            "Estimating catalog tokens",
            Some(format!("正在估算 {} 条文本的大致请求成本", loaded.catalog.units.len())),
        );
        if cancellation.is_cancelled() {
            reporter.warn_key(
                WorkflowEventPhase::Estimate,
                "workflow.estimate.cancelled",
                "Token estimate cancelled",
                Some("已在开始估算前收到取消请求".to_owned()),
            );
            return Err(TranslateError::Cancelled);
        }

        let pending_terms = catalog_db::load_pending_glossary_terms(&pool).await?;
        let user_term_index = TermMatchIndex::from_terms(&resources.glossary_terms);
        let pending_api_terms = pending_terms
            .iter()
            .filter(|term| catalog_db::resolve_user_glossary_term(term, &resources).is_none())
            .cloned()
            .collect::<Vec<_>>();
        let planned_term_batches = plan_term_batches(
            &pending_api_terms,
            &user_term_index,
            &config.model,
            &system_prompt,
            config.target_input_tokens,
        )?;
        let (estimated_term_input_tokens, estimated_term_output_tokens) = estimate_term_batches(
            &planned_term_batches,
            &user_term_index,
            &response_counter,
            cancellation,
        )?;
        let glossary_source_unit_ids = catalog_db::load_glossary_source_unit_ids(&pool).await?;
        let terminology = catalog_db::load_term_match_index(&pool, &resources).await?;
        let pending_term_unit_ids = pending_api_terms
            .iter()
            .map(|term| term.source_unit_id.clone())
            .collect::<BTreeSet<_>>();
        let pending_indices = loaded
            .catalog
            .units
            .iter()
            .enumerate()
            .filter(|(_, unit)| !glossary_source_unit_ids.contains(&unit.id))
            .filter(|(_, unit)| {
                unit.translated_text.is_none()
                    || matches!(
                        unit.status,
                        crate::domain::TranslationStatus::Pending
                            | crate::domain::TranslationStatus::Failed
                    )
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let (planned_main_batches, _metrics) = plan_translation_batches_with_terms_and_reporter(
            &loaded.catalog.units,
            &loaded.catalog.spans,
            &pending_indices,
            &config.model,
            &system_prompt,
            config.target_input_tokens,
            config.batching_strategy,
            terminology.clone(),
            Some((reporter, WorkflowEventPhase::Estimate)),
        )?;
        let planning_context = create_planning_context(
            &loaded.catalog.units,
            &loaded.catalog.spans,
            &config.model,
            &system_prompt,
            config.target_input_tokens,
            config.batching_strategy,
            terminology.clone(),
        )?;
        let mut estimated_main_input_tokens = 0usize;
        let mut estimated_main_output_tokens = 0usize;
        for result in planned_main_batches
            .par_iter()
            .map(|batch| {
                if cancellation.is_cancelled() {
                    return Err(TranslateError::Cancelled);
                }
                let prepared_prompt = planning_context.prepared_prompt_for_batch(batch)?;
                let input = response_counter.count_rendered_prompt_body(prepared_prompt.body());
                let output = response_counter.count_response_tokens(&batch.units)?;
                Ok::<_, TranslateError>((input, output))
            })
            .collect::<Vec<_>>()
        {
            let (input, output) = result?;
            estimated_main_input_tokens += input;
            estimated_main_output_tokens += output;
        }

        let estimated_batches = planned_term_batches.len() + planned_main_batches.len();
        let estimated_input_tokens = estimated_term_input_tokens + estimated_main_input_tokens;
        let estimated_output_tokens = estimated_term_output_tokens + estimated_main_output_tokens;
        let pending_units_total = pending_indices.len() + pending_term_unit_ids.len();
        let estimated_scene_batches = planned_main_batches
            .iter()
            .filter(|batch| batch.kind.as_str() == "single_segment")
            .count();
        let estimated_orphan_pool_batches = planned_main_batches
            .iter()
            .filter(|batch| batch.kind.as_str() == "grouped_pool")
            .count();
        let average_main_input_utilization_pct = if planned_main_batches.is_empty()
            || config.target_input_tokens == 0
        {
            0
        } else {
            ((estimated_main_input_tokens as f64
                / (planned_main_batches.len() as f64 * config.target_input_tokens as f64))
                * 100.0)
                .round()
                .clamp(0.0, 999.0) as usize
        };
        let estimated_average_input_tokens = if planned_main_batches.is_empty() {
            0
        } else {
            estimated_main_input_tokens / planned_main_batches.len()
        };
        reporter.info_key(
            WorkflowEventPhase::Estimate,
            "workflow.estimate.done",
            "Token estimate complete",
            Some(format!(
                "预计正文 {} 批、术语 {} 批，输入约 {} Token，输出约 {} Token，正文平均利用率约 {}%",
                planned_main_batches.len(),
                planned_term_batches.len(),
                estimated_input_tokens,
                estimated_output_tokens,
                average_main_input_utilization_pct
            )),
        );
        Ok(CatalogTokenEstimate {
            catalog_path: catalog_path.display().to_string(),
            total_units: loaded.catalog.units.len(),
            pending_units: pending_units_total,
            reused_units: loaded.catalog.units.len().saturating_sub(pending_units_total),
            target_input_tokens: config.target_input_tokens,
            batching_strategy: config.batching_strategy,
            estimated_term_batches: planned_term_batches.len(),
            estimated_term_input_tokens,
            estimated_term_output_tokens,
            estimated_main_batches: planned_main_batches.len(),
            estimated_scene_batches,
            estimated_orphan_pool_batches,
            estimated_main_input_tokens,
            estimated_main_output_tokens,
            estimated_batches,
            estimated_input_tokens,
            estimated_output_tokens,
            estimated_total_tokens: estimated_input_tokens + estimated_output_tokens,
            estimated_average_input_tokens,
            estimated_average_input_utilization_pct: average_main_input_utilization_pct,
            average_main_input_utilization_pct,
        })
    }
    .await;
    pool.close().await;
    result
}

fn estimate_term_batches(
    batches: &[super::term_pretranslate::PlannedTermBatch],
    term_index: &TermMatchIndex,
    response_counter: &prompting::PromptTokenCounter,
    cancellation: &CancellationFlag,
) -> Result<(usize, usize), TranslateError> {
    let estimates = batches
        .par_iter()
        .map(|batch| {
            if cancellation.is_cancelled() {
                return Err(TranslateError::Cancelled);
            }
            let glossary_terms = term_index.batch_terms_for_units(&batch.units, MAX_BATCH_GLOSSARY_TERMS);
            let input =
                response_counter.count_request_tokens_with_terms(&batch.units, &[], &glossary_terms)?;
            let output = response_counter.count_response_tokens(&batch.units)?;
            Ok::<_, TranslateError>((input, output))
        })
        .collect::<Vec<_>>();
    let mut input_total = 0usize;
    let mut output_total = 0usize;
    for result in estimates {
        let (input, output) = result?;
        input_total += input;
        output_total += output;
    }
    Ok((input_total, output_total))
}
