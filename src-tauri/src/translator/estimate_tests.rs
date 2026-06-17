use super::TranslateError;
use super::estimate::estimate_catalog_tokens;
use crate::app_state::CancellationFlag;
use crate::catalog_db;
use crate::domain::{
    ApiEndpoint, BatchingStrategy, CatalogProject, ContextEnvelope, GameProfile, ProjectConfig,
    SourceKind, TranslationCatalog, TranslationSpan, TranslationStatus, TranslationUnit,
};
use crate::workflow_events::WorkflowReporter;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::test]
async fn estimate_stops_when_cancelled() {
    let root = temp_root("estimate_cancel");
    fs::create_dir_all(root.join("www").join("data")).expect("create data dir");
    let catalog = catalog(&root);
    let path = catalog_db::persist_catalog(&root, &catalog, &WorkflowReporter::noop())
        .await
        .expect("persist catalog");
    let cancellation = CancellationFlag::default();
    cancellation.cancel();
    let (reporter, events) = WorkflowReporter::collector(false);

    let error = estimate_catalog_tokens(&path, &config(&root), &cancellation, &reporter)
        .await
        .expect_err("cancelled estimate should fail");

    assert!(matches!(error, TranslateError::Cancelled));
    assert!(
        events
            .lock()
            .expect("collector lock")
            .iter()
            .any(|event| event.message == "Token estimate cancelled")
    );
    fs::remove_dir_all(root).expect("cleanup");
}

fn temp_root(name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_nanos();
    std::env::temp_dir().join(format!("rush_patch_{name}_{stamp}"))
}

fn config(root: &std::path::Path) -> ProjectConfig {
    ProjectConfig {
        game_root: root.display().to_string(),
        model: "gpt-4.1-mini".to_owned(),
        api_endpoint: ApiEndpoint::Responses,
        api_key: None,
        base_url: None,
        system_prompt: "Translate.".to_owned(),
        glossary_path: None,
        do_not_translate_path: None,
        game_profile: GameProfile::GeneralRpg,
        target_input_tokens: 1_000,
        batching_strategy: BatchingStrategy::MaximizeUtilization,
        debug_logging: false,
        max_concurrency: 1,
        request_timeout_secs: 90,
        source_lang: "Japanese".to_owned(),
        target_lang: "Chinese".to_owned(),
    }
}

fn catalog(root: &std::path::Path) -> TranslationCatalog {
    let file_path = root.join("www").join("data").join("Map001.json");
    let file = file_path.display().to_string();
    let span_id = format!("{file}::$");
    TranslationCatalog {
        project: CatalogProject {
            game_root: root.display().to_string(),
            engine: "MZ".to_owned(),
            generated_at: "0".to_owned(),
        },
        spans: vec![TranslationSpan {
            id: span_id.clone(),
            file: file.clone(),
            source_kind: SourceKind::Json,
            locator: "$".to_owned(),
            source_text: "hello".to_owned(),
            protected_tokens: Vec::new(),
            flags: Vec::new(),
        }],
        units: vec![TranslationUnit {
            id: span_id.clone(),
            group_id: span_id.clone(),
            semantic_kind: "dialogue".to_owned(),
            context: ContextEnvelope {
                file,
                json_path: Some("$".to_owned()),
                map_id: None,
                event_id: None,
                page_id: None,
                command_index: None,
                speaker_name: None,
                prev_texts: Vec::new(),
                next_texts: Vec::new(),
                block_text: Some("hello".to_owned()),
                glossary_hits: Vec::new(),
                notes: Vec::new(),
            },
            source_text: "hello".to_owned(),
            translated_text: None,
            status: TranslationStatus::Pending,
            span_ids: vec![span_id],
        }],
    }
}
