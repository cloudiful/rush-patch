mod app_state;
mod applier;
mod catalog;
pub mod catalog_db;
#[cfg(not(test))]
mod commands;
mod domain;
mod extractor_js;
mod extractor_json;
mod js_strings;
mod json_pointer;
mod openai_responses;
mod parallel;
mod patch_storage;
mod prompting;
mod scanner;
#[cfg(not(test))]
mod settings;
mod terminology;
mod text;
mod text_io;
mod translation_io;
mod translator;
mod validator;
mod workflow_events;

#[cfg(not(test))]
use commands::{
    apply_translated_patch, build_catalog, cancel_translation, estimate_catalog,
    extract_json_preview, load_app_settings, restore_original_text, save_app_settings,
    scan_project, translate_catalog,
};
#[cfg(not(test))]
use tauri::{LogicalSize, Manager, WebviewWindow};

#[cfg(not(test))]
const DEFAULT_WINDOW_WIDTH: f64 = 1360.0;
#[cfg(not(test))]
const DEFAULT_WINDOW_HEIGHT: f64 = 920.0;
#[cfg(not(test))]
const MIN_WINDOW_WIDTH: f64 = 960.0;
#[cfg(not(test))]
const MIN_WINDOW_HEIGHT: f64 = 680.0;
#[cfg(not(test))]
const SCREEN_USAGE_RATIO: f64 = 0.9;

#[cfg(not(test))]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(app_state::AppState::default())
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                fit_window_to_current_monitor(&window);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            scan_project,
            load_app_settings,
            save_app_settings,
            extract_json_preview,
            build_catalog,
            estimate_catalog,
            translate_catalog,
            cancel_translation,
            apply_translated_patch,
            restore_original_text
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Rush Patch");
}

#[cfg(not(test))]
fn fit_window_to_current_monitor(window: &WebviewWindow) {
    let Ok(Some(monitor)) = window.current_monitor() else {
        return;
    };

    let work_area = monitor.work_area();
    let scale_factor = monitor.scale_factor().max(1.0);
    let max_width =
        (work_area.size.width as f64 / scale_factor * SCREEN_USAGE_RATIO).max(MIN_WINDOW_WIDTH);
    let max_height =
        (work_area.size.height as f64 / scale_factor * SCREEN_USAGE_RATIO).max(MIN_WINDOW_HEIGHT);
    let width = DEFAULT_WINDOW_WIDTH.min(max_width);
    let height = DEFAULT_WINDOW_HEIGHT.min(max_height);

    if window.set_size(LogicalSize::new(width, height)).is_ok() {
        let _ = window.center();
    }
}
