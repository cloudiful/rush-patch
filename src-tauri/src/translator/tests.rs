use super::*;
use crate::app_state::CancellationFlag;
use crate::domain::{ApiEndpoint, BatchingStrategy, GameProfile, ProjectConfig, WorkflowEventLevel};
use crate::domain::{ContextEnvelope, TranslationStatus};
use crate::workflow_events::WorkflowReporter;
use futures::executor::block_on;
use std::path::Path;

#[test]
fn applies_partial_batch_and_marks_missing_ids_failed() {
    let mut units = vec![unit("a"), unit("b")];
    let translations = BTreeMap::from([("a".to_owned(), "translated".to_owned())]);

    apply_batch_translations(&mut units, &[0, 1], &translations);

    assert_eq!(units[0].translated_text.as_deref(), Some("translated"));
    assert!(matches!(units[0].status, TranslationStatus::Translated));
    assert!(matches!(units[1].status, TranslationStatus::Failed));
}

#[test]
fn selects_pending_and_failed_without_retranslating_valid_items() {
    let pending = unit("pending");
    let mut failed = unit("failed");
    failed.status = TranslationStatus::Failed;
    let mut translated = unit("translated");
    translated.status = TranslationStatus::Translated;
    translated.translated_text = Some("done".to_owned());

    let indices = pending_unit_indices(&[pending, failed, translated]);

    assert_eq!(indices, vec![0, 1]);
}

fn unit(id: &str) -> TranslationUnit {
    TranslationUnit {
        id: id.to_owned(),
        group_id: id.to_owned(),
        semantic_kind: "dialogue".to_owned(),
        context: ContextEnvelope {
            file: "Map001.json".to_owned(),
            json_path: None,
            map_id: None,
            event_id: None,
            page_id: None,
            command_index: None,
            speaker_name: None,
            prev_texts: Vec::new(),
            next_texts: Vec::new(),
            block_text: None,
            glossary_hits: Vec::new(),
            notes: Vec::new(),
        },
        source_text: "source".to_owned(),
        translated_text: None,
        status: TranslationStatus::Pending,
        span_ids: Vec::new(),
    }
}

#[test]
fn missing_api_key_emits_error_event_before_returning() {
    let config = ProjectConfig {
        game_root: String::new(),
        model: "gpt-4.1-mini".to_owned(),
        api_endpoint: ApiEndpoint::Responses,
        api_key: None,
        base_url: None,
        system_prompt: String::new(),
        glossary_path: None,
        do_not_translate_path: None,
        game_profile: GameProfile::GeneralRpg,
        target_input_tokens: 6_000,
        batching_strategy: BatchingStrategy::MaximizeUtilization,
        debug_logging: false,
        max_concurrency: 1,
        request_timeout_secs: 90,
        source_lang: "Japanese".to_owned(),
        target_lang: "Chinese".to_owned(),
    };
    let (reporter, events) = WorkflowReporter::collector(false);

    let error = block_on(translate_catalog(
        Path::new("catalog.json"),
        config,
        CancellationFlag::default(),
        &reporter,
    ))
    .expect_err("missing API key should fail");

    assert!(matches!(error, TranslateError::MissingApiKey));
    let events = events.lock().expect("collector lock");
    assert!(events.iter().any(|event| {
        event.level == WorkflowEventLevel::Error && event.message == "Missing API key"
    }));
}
