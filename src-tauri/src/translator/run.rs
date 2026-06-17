use super::main_translate::run_main_translation;
use super::request::{build_client, sanitized_base_url_host};
use super::term_pretranslate::run_term_pretranslate;
use super::TranslateError;
use crate::app_state::CancellationFlag;
use crate::catalog_db;
use crate::domain::{
    ApiEndpoint, ProjectConfig, TranslationRunSummary, TranslationStatus, ValidationStatus,
    WorkflowEventPhase,
};
use crate::prompting;
use crate::translation_io;
use crate::validator;
use crate::workflow_events::WorkflowReporter;
use std::path::Path;

pub(super) async fn translate_catalog_impl(
    catalog_path: &Path,
    config: ProjectConfig,
    cancellation: CancellationFlag,
    reporter: &WorkflowReporter,
) -> Result<TranslationRunSummary, TranslateError> {
    if config.target_input_tokens == 0 {
        reporter.error_key(
            WorkflowEventPhase::Translate,
            "workflow.translate.invalidTargetTokens",
            "Invalid max input tokens",
            Some("目标输入 Token 必须大于 0".to_owned()),
        );
        return Err(TranslateError::InvalidMaxInputTokens);
    }

    let api_key = config
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            reporter.error_key(
                WorkflowEventPhase::Translate,
                "workflow.translate.missingApiKey",
                "Missing API key",
                Some("请先填写可用的 API Key".to_owned()),
            );
            TranslateError::MissingApiKey
        })?;

    let pool = catalog_db::open_pool(catalog_path, false, 4).await?;
    let loaded = catalog_db::load_catalog_from_pool(&pool).await?;
    let resources = translation_io::load_resources(
        config.glossary_path.as_deref(),
        config.do_not_translate_path.as_deref(),
    )?;
    let system_prompt = prompting::build_system_prompt(&config, &resources);
    let client = build_client(api_key, config.base_url.as_deref());
    let total_units = loaded.catalog.units.len();
    let mut catalog = loaded.catalog;

    reporter.info_key(
        WorkflowEventPhase::Translate,
        "workflow.translate.prepare",
        "Preparing translation workflow",
        Some(format!(
            "已加载 {} 条文本单元和 {} 个写回片段，正在准备翻译",
            catalog.units.len(),
            catalog.spans.len()
        )),
    );
    reporter.debug(
        WorkflowEventPhase::Translate,
        "Translation request configuration",
        Some(format!("model {}", config.model)),
        [
            ("model", config.model.clone()),
            (
                "endpoint",
                match config.api_endpoint {
                    ApiEndpoint::Responses => "responses".to_owned(),
                    ApiEndpoint::ChatCompletions => "chat_completions".to_owned(),
                },
            ),
            (
                "base_url_host",
                sanitized_base_url_host(config.base_url.as_deref()),
            ),
            (
                "batching_strategy",
                config.batching_strategy.as_str().to_owned(),
            ),
            ("target_input_tokens", config.target_input_tokens.to_string()),
            ("timeout_secs", config.request_timeout_secs.to_string()),
            ("max_concurrency", config.max_concurrency.to_string()),
        ],
    );

    let term_summary = run_term_pretranslate(
        &pool,
        &mut catalog.units,
        &resources,
        &config,
        &system_prompt,
        &client,
        &cancellation,
        reporter,
    )
    .await?;
    let excluded_unit_ids = catalog_db::load_glossary_source_unit_ids(&pool).await?;
    let terminology = catalog_db::load_term_match_index(&pool, &resources).await?;

    let main_summary = run_main_translation(
        &pool,
        &mut catalog,
        &config,
        &system_prompt,
        &client,
        &terminology,
        &excluded_unit_ids,
        &cancellation,
        reporter,
    )
    .await?;

    reporter.info_key(
        WorkflowEventPhase::Translate,
        "workflow.translate.validate",
        "Validating translated catalog",
        Some("正在做最终占位符与控制码校验".to_owned()),
    );
    let reports = validator::validate_catalog_with_terms(&catalog, &terminology);
    for report in &reports {
        if matches!(report.status, ValidationStatus::Failed)
            && let Some(unit) = catalog
                .units
                .iter_mut()
                .find(|unit| unit.id == report.unit_id)
        {
            unit.status = TranslationStatus::Failed;
        }
    }
    let all_indices = (0..catalog.units.len()).collect::<Vec<_>>();
    catalog_db::update_units_with_pool(
        &pool,
        &super::unit_updates_for_indices(&catalog.units, &all_indices),
    )
    .await?;

    let validation_failed_units = reports
        .iter()
        .filter(|report| matches!(report.status, ValidationStatus::Failed))
        .count();
    let validation_warning_units = reports
        .iter()
        .filter(|report| matches!(report.status, ValidationStatus::Warning))
        .count();
    if !reports.is_empty() {
        reporter.info_key(
            WorkflowEventPhase::Translate,
            "workflow.translate.validationDone",
            "Validation completed",
            Some(format!(
                "校验完成：{} 条失败，{} 条警告",
                validation_failed_units, validation_warning_units
            )),
        );
    }

    if term_summary.cancelled || main_summary.cancelled {
        reporter.warn_key(
            WorkflowEventPhase::Translate,
            "workflow.translate.cancelled",
            "Translation cancelled",
            Some(format!(
                "已取消翻译；取消前已完成术语批次 {} 个、正文批次 {} 个",
                term_summary.batches, main_summary.batches
            )),
        );
    } else {
        reporter.info_key(
            WorkflowEventPhase::Translate,
            "workflow.translate.done",
            "Translation complete",
            Some(format!(
                "翻译完成：术语预翻译 {} 条，术语失败 {} 条，正文成功 {} 条，正文失败 {} 条，重试 {} 次",
                term_summary.pretranslated_terms,
                term_summary.failed_terms,
                main_summary.translated_units,
                main_summary.failed_units,
                term_summary.retries + main_summary.retries
            )),
        );
    }

    pool.close().await;

    Ok(TranslationRunSummary {
        catalog_path: catalog_path.display().to_string(),
        total_units,
        attempted_units: main_summary.attempted_units
            + term_summary.pretranslated_terms
            + term_summary.failed_terms,
        translated_units: catalog
            .units
            .iter()
            .filter(|unit| {
                matches!(
                    unit.status,
                    TranslationStatus::Translated | TranslationStatus::Validated
                )
            })
            .count(),
        pretranslated_terms: term_summary.pretranslated_terms,
        failed_terms: term_summary.failed_terms,
        failed_units: catalog
            .units
            .iter()
            .filter(|unit| matches!(unit.status, TranslationStatus::Failed))
            .count(),
        validation_failed_units,
        validation_warning_units,
        batches: term_summary.batches + main_summary.batches,
        retries: term_summary.retries + main_summary.retries,
        cancelled: term_summary.cancelled || main_summary.cancelled,
    })
}
