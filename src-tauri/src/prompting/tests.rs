use super::{
    PromptSeedGroup, build_grouped_user_prompt_from_seed_groups_with_terms, build_prompt_render_seeds,
    build_user_prompt, build_user_prompt_with_terms,
};
use crate::domain::{
    ContextEnvelope, SourceKind, TranslationSpan, TranslationStatus, TranslationUnit,
};
use crate::terminology::CanonicalTerm;

#[test]
fn prompt_is_compact_and_includes_useful_context() {
    let unit = test_unit(
        "unit-1",
        r"Hello \N[1]",
        vec!["Before"],
        vec![],
        Some("Hero"),
        "field_extraction",
        "dialogue",
    );
    let span = TranslationSpan {
        id: "span-1".to_owned(),
        file: "Map001.json".to_owned(),
        source_kind: SourceKind::Json,
        locator: "$.events.1".to_owned(),
        source_text: r"Hello \N[1]".to_owned(),
        protected_tokens: vec![r"\N[1]".to_owned()],
        flags: Vec::new(),
    };

    let prompt = build_user_prompt(&[unit], &[span]).expect("prompt");

    assert!(prompt.body().contains(r#""id":"u1""#));
    assert!(prompt.body().contains(r#""f":"www/data/Map001.json""#));
    assert!(prompt.body().contains(r#""pre":["Before"]"#));
    assert!(prompt.body().contains(r#""sp":"Hero""#));
    assert!(prompt.body().contains(r#""tok":["\\N[1]"]"#));
    assert!(!prompt.body().contains('\n'));
    assert!(!prompt.body().contains("notes"));
}

#[test]
fn batch_prompt_shares_file_kind_and_speaker_once() {
    let first = test_unit(
        "unit-1",
        "A",
        vec!["Before"],
        vec![],
        Some("Hero"),
        "event_dialogue_block",
        "dialogue",
    );
    let second = test_unit(
        "unit-2",
        "B",
        vec!["A"],
        vec!["After"],
        Some("Hero"),
        "event_dialogue_block",
        "dialogue",
    );

    let prompt = build_user_prompt(&[first, second], &[]).expect("prompt");

    assert_eq!(
        prompt
            .body()
            .matches(r#""f":"www/data/Map001.json""#)
            .count(),
        1
    );
    assert_eq!(prompt.body().matches(r#""sp":"Hero""#).count(), 1);
    assert_eq!(prompt.body().matches(r#""k":"dialogue""#).count(), 1);
    assert!(prompt.body().contains(r#""pre":["Before"]"#));
    assert!(prompt.body().contains(r#""post":["After"]"#));
    assert!(
        !prompt
            .body()
            .contains(r#""ctx":{"f":"www/data/Map001.json""#)
    );
    assert!(!prompt.body().contains(r#""ctx":{"p":"$.events.1""#));
}

#[test]
fn keeps_item_kind_when_batch_is_mixed() {
    let first = test_unit(
        "unit-1",
        "A",
        vec![],
        vec![],
        Some("Hero"),
        "event_dialogue_block",
        "dialogue",
    );
    let second = test_unit(
        "unit-2",
        "Choice",
        vec![],
        vec![],
        Some("Hero"),
        "choice_group",
        "choice",
    );

    let prompt = build_user_prompt(&[first, second], &[]).expect("prompt");

    assert!(!prompt.body().contains(r#""k":"dialogue","pre""#));
    assert!(prompt.body().contains(r#""id":"u1","k":"dialogue""#));
    assert!(prompt.body().contains(r#""id":"u2","k":"choice""#));
}

#[test]
fn resolves_aliases_back_to_real_ids() {
    let unit = test_unit(
        "real-id",
        "hello",
        vec![],
        vec![],
        None,
        "field_extraction",
        "text",
    );

    let prompt = build_user_prompt(&[unit], &[]).expect("prompt");
    let parsed = prompt
        .resolve_translations(r#"{"translations":[{"id":"u1","translatedText":"translated"}]}"#)
        .expect("parse response");

    assert_eq!(
        parsed.get("real-id").map(String::as_str),
        Some("translated")
    );
}

#[test]
fn prompt_includes_shared_glossary_hits_once() {
    let unit = test_unit(
        "unit-1",
        "ロマーシャは来た",
        vec![],
        vec![],
        None,
        "field_extraction",
        "text",
    );

    let prompt = build_user_prompt_with_terms(
        &[unit],
        &[],
        &[CanonicalTerm {
            source: "ロマーシャ".to_owned(),
            target: "罗玛夏".to_owned(),
        }],
    )
    .expect("prompt");

    assert!(prompt.body().contains(r#""g":[["ロマーシャ","罗玛夏"]]"#));
    assert_eq!(prompt.body().matches(r#""g":["#).count(), 1);
}

#[test]
fn grouped_prompt_keeps_segment_boundaries() {
    let first = test_unit(
        "unit-1",
        "Potion",
        vec!["Before"],
        vec![],
        None,
        "field_extraction",
        "name",
    );
    let second = test_unit(
        "unit-2",
        "Armor",
        vec![],
        vec!["After"],
        None,
        "field_extraction",
        "name",
    );
    let seeds = build_prompt_render_seeds(&[first, second], &[]);
    let prompt = build_grouped_user_prompt_from_seed_groups_with_terms(
        &[
            PromptSeedGroup {
                seeds: vec![&seeds[0]],
            },
            PromptSeedGroup {
                seeds: vec![&seeds[1]],
            },
        ],
        &[],
    )
    .expect("grouped prompt");

    assert!(prompt.body().contains(r#""groups":["#));
    assert_eq!(prompt.body().matches(r#""i":["#).count(), 2);
    assert!(prompt.body().contains(r#""id":"u1""#));
    assert!(prompt.body().contains(r#""id":"u2""#));
}

fn test_unit(
    id: &str,
    source_text: &str,
    prev_texts: Vec<&str>,
    next_texts: Vec<&str>,
    speaker_name: Option<&str>,
    note: &str,
    semantic_kind: &str,
) -> TranslationUnit {
    TranslationUnit {
        id: id.to_owned(),
        group_id: id.to_owned(),
        semantic_kind: semantic_kind.to_owned(),
        context: ContextEnvelope {
            file: r"\\server\share\game\www\data\Map001.json".to_owned(),
            json_path: Some("$.events.1".to_owned()),
            map_id: Some(1),
            event_id: Some(1),
            page_id: Some(0),
            command_index: Some(3),
            speaker_name: speaker_name.map(str::to_owned),
            prev_texts: prev_texts.into_iter().map(str::to_owned).collect(),
            next_texts: next_texts.into_iter().map(str::to_owned).collect(),
            block_text: None,
            glossary_hits: Vec::new(),
            notes: vec![note.to_owned()],
        },
        source_text: source_text.to_owned(),
        translated_text: None,
        status: TranslationStatus::Pending,
        span_ids: if source_text.contains(r"\N[1]") {
            vec!["span-1".to_owned()]
        } else {
            Vec::new()
        },
    }
}
