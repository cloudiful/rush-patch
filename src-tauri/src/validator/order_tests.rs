use super::validate_catalog;
use crate::domain::{
    CatalogProject, ContextEnvelope, SourceKind, TranslationCatalog, TranslationSpan,
    TranslationStatus, TranslationUnit,
};

#[test]
fn parallel_validation_preserves_unit_order() {
    let catalog = TranslationCatalog {
        project: CatalogProject {
            game_root: "game".to_owned(),
            engine: "MZ".to_owned(),
            generated_at: "0".to_owned(),
        },
        spans: vec![span("a"), span("b"), span("c")],
        units: vec![unit("a"), unit("b"), unit("c")],
    };

    let ids = validate_catalog(&catalog)
        .into_iter()
        .map(|report| report.unit_id)
        .collect::<Vec<_>>();

    assert_eq!(ids, vec!["a", "b", "c"]);
}

fn span(id: &str) -> TranslationSpan {
    TranslationSpan {
        id: id.to_owned(),
        file: "Map001.json".to_owned(),
        source_kind: SourceKind::Json,
        locator: "$".to_owned(),
        source_text: "source".to_owned(),
        protected_tokens: Vec::new(),
        flags: Vec::new(),
    }
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
        translated_text: Some("target".to_owned()),
        status: TranslationStatus::Translated,
        span_ids: vec![id.to_owned()],
    }
}
