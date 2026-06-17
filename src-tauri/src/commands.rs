use crate::app_state::AppState;
use crate::applier;
use crate::catalog;
use crate::domain::WorkflowEventPhase;
use crate::domain::{
    CatalogTokenEstimate, JsonExtractionPreview, LoadedAppSettings, PatchPlan, RestorePlan,
    SaveAppSettingsRequest, SaveAppSettingsSummary, TranslateCatalogRequest, TranslationRunSummary,
};
use crate::extractor_json;
use crate::scanner;
use crate::scanner::ScanSummary;
use crate::settings;
use crate::translator;
use crate::workflow_events::WorkflowReporter;
use std::future::Future;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn scan_project(game_root: String) -> Result<ScanSummary, String> {
    run_blocking(move || scanner::scan_project(&game_root).map_err(|error| error.to_string())).await
}

#[tauri::command]
pub async fn extract_json_preview(game_root: String) -> Result<JsonExtractionPreview, String> {
    run_blocking(move || {
        extractor_json::extract_project_json(&game_root).map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
pub async fn build_catalog(
    app: AppHandle,
    game_root: String,
    debug_logging: bool,
) -> Result<String, String> {
    let reporter = WorkflowReporter::tauri(app, debug_logging);
    let path = catalog::ensure_catalog_path(&game_root, &reporter)
        .await
        .map_err(|error| {
            reporter.error(
                WorkflowEventPhase::Catalog,
                "Catalog build failed",
                Some(error.to_string()),
            );
            error.to_string()
        })?;
    Ok(path.display().to_string())
}

#[tauri::command]
pub async fn estimate_catalog(
    app: AppHandle,
    state: State<'_, AppState>,
    request: TranslateCatalogRequest,
) -> Result<CatalogTokenEstimate, String> {
    let cancellation = state.reset_translation_cancel();
    let TranslateCatalogRequest {
        catalog_path,
        config,
    } = request;
    let reporter = WorkflowReporter::tauri(app, config.debug_logging);
    match translator::estimate_catalog(
        std::path::Path::new(&catalog_path),
        config,
        cancellation,
        &reporter,
    )
    .await
    {
        Ok(estimate) => Ok(estimate),
        Err(translator::TranslateError::Cancelled) => {
            reporter.warn(
                WorkflowEventPhase::Estimate,
                "Token estimate cancelled",
                Some(catalog_path),
            );
            Err("translation cancelled".to_owned())
        }
        Err(error) => {
            reporter.error(
                WorkflowEventPhase::Estimate,
                "Token estimate failed",
                Some(error.to_string()),
            );
            Err(error.to_string())
        }
    }
}

#[tauri::command]
pub fn load_app_settings(app: AppHandle) -> Result<LoadedAppSettings, String> {
    settings::load_app_settings(&app).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_app_settings(
    app: AppHandle,
    request: SaveAppSettingsRequest,
) -> Result<SaveAppSettingsSummary, String> {
    settings::save_app_settings(&app, request).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn apply_translated_patch(
    app: AppHandle,
    game_root: String,
    catalog_path: String,
    debug_logging: bool,
) -> Result<PatchPlan, String> {
    let reporter = WorkflowReporter::tauri(app, debug_logging);
    applier::apply_translated_patch(&game_root, std::path::Path::new(&catalog_path), &reporter)
        .await
        .map_err(|error| {
            reporter.error(
                WorkflowEventPhase::Patch,
                "Patch apply failed",
                Some(error.to_string()),
            );
            error.to_string()
        })
}

#[tauri::command]
pub async fn restore_original_text(game_root: String) -> Result<RestorePlan, String> {
    run_blocking(move || {
        applier::restore_original_text(&game_root).map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
pub async fn translate_catalog(
    app: AppHandle,
    state: State<'_, AppState>,
    request: TranslateCatalogRequest,
) -> Result<TranslationRunSummary, String> {
    let cancellation = state.reset_translation_cancel();
    let TranslateCatalogRequest {
        catalog_path,
        config,
    } = request;
    let reporter = WorkflowReporter::tauri(app, config.debug_logging);
    translator::translate_catalog(
        std::path::Path::new(&catalog_path),
        config,
        cancellation,
        &reporter,
    )
    .await
    .map_err(|error| {
        if matches!(error, translator::TranslateError::Cancelled) {
            reporter.warn(
                WorkflowEventPhase::Translate,
                "Translation cancelled",
                Some("Cancellation requested by user".to_owned()),
            );
        } else {
            reporter.error(
                WorkflowEventPhase::Translate,
                "Translation failed",
                Some(error.to_string()),
            );
        }
        error.to_string()
    })
}

#[tauri::command]
pub fn cancel_translation(state: State<'_, AppState>) -> Result<bool, String> {
    state.cancel_translation();
    Ok(true)
}

async fn run_blocking<T, F>(task: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    blocking_task(task)
        .await
        .map_err(|error| format!("background task failed: {error}"))?
}

fn blocking_task<T, F>(task: F) -> impl Future<Output = Result<Result<T, String>, tauri::Error>>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(task)
}
