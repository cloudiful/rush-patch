use super::*;
use crate::domain::CatalogProject;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_root(name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_nanos();
    std::env::temp_dir().join(format!("rush_patch_extract_{name}_{stamp}"))
}

#[test]
fn groups_event_text_and_adds_ordered_context() {
    let root = temp_root("json");
    let data_dir = root.join("www").join("data");
    fs::create_dir_all(&data_dir).expect("create data dir");

    fs::write(
        data_dir.join("Map001.json"),
        r#"{"events":{"1":{"pages":[{"list":[{"code":101,"parameters":["Hero"]},{"code":401,"parameters":["Hello"]},{"code":401,"parameters":["there"]},{"code":102,"parameters":[["Yes","No"],0,0,2,0]},{"code":105,"parameters":[]},{"code":405,"parameters":["Scroll one"]},{"code":405,"parameters":["Scroll two"]}]}]}}}"#,
    )
    .expect("write map");

    let catalog = crate::catalog::build_catalog(
        root.to_str().expect("utf8 path"),
        &crate::workflow_events::WorkflowReporter::noop(),
    )
    .expect("build catalog");

    assert_eq!(catalog.units.len(), 3);
    assert_eq!(catalog.spans.len(), 6);
    let dialogue = &catalog.units[0];
    assert_eq!(dialogue.semantic_kind, "dialogue");
    assert_eq!(dialogue.context.speaker_name.as_deref(), Some("Hero"));
    assert_eq!(dialogue.source_text, "Hello\nthere");
    assert_eq!(dialogue.span_ids.len(), 2);
    assert_eq!(
        dialogue.context.next_texts,
        vec!["Choice: Yes / No", "Scroll: Scroll one / Scroll two"]
    );

    let choice = &catalog.units[1];
    assert_eq!(choice.semantic_kind, "choice");
    assert_eq!(choice.context.prev_texts, vec!["Hero: Hello / there"]);
    assert_eq!(
        choice.context.next_texts,
        vec!["Scroll: Scroll one / Scroll two"]
    );

    let scroll = &catalog.units[2];
    assert_eq!(scroll.semantic_kind, "scroll_text");
    assert_eq!(
        scroll.context.prev_texts,
        vec!["Hero: Hello / there", "Choice: Yes / No"]
    );
    assert_eq!(scroll.source_text, "Scroll one\nScroll two");

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn does_not_leak_event_context_between_event_lists_or_fields() {
    let root = temp_root("scene_reset");
    let data_dir = root.join("www").join("data");
    fs::create_dir_all(&data_dir).expect("create data dir");

    fs::write(
        data_dir.join("Map001.json"),
        r#"{"displayName":"Town","events":{"1":{"pages":[{"list":[{"code":101,"parameters":["Hero"]},{"code":401,"parameters":["Hello"]}]}]},"2":{"pages":[{"list":[{"code":401,"parameters":["Fresh start"]}]}]}}}"#,
    )
    .expect("write map");

    let catalog = crate::catalog::build_catalog(
        root.to_str().expect("utf8 path"),
        &crate::workflow_events::WorkflowReporter::noop(),
    )
    .expect("build catalog");

    assert_eq!(catalog.units.len(), 3);
    assert_eq!(catalog.units[0].semantic_kind, "name");
    assert!(catalog.units[0].context.prev_texts.is_empty());
    assert!(catalog.units[0].context.speaker_name.is_none());

    let first_event = &catalog.units[1];
    assert_eq!(first_event.source_text, "Hello");
    assert_eq!(first_event.context.speaker_name.as_deref(), Some("Hero"));
    assert!(first_event.context.prev_texts.is_empty());

    let second_event = &catalog.units[2];
    assert_eq!(second_event.source_text, "Fresh start");
    assert!(second_event.context.prev_texts.is_empty());
    assert!(second_event.context.speaker_name.is_none());

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn applies_translated_lines_back_into_json() {
    let file = Path::new("C:\\tmp\\Map001.json");
    let mut value: Value = serde_json::from_str(
        r#"{"events":{"1":{"pages":[{"list":[{"code":401,"parameters":["Hello"]},{"code":401,"parameters":["there"]},{"code":102,"parameters":[["Yes","No"],0,0,2,0]}]}]}}}"#,
    )
    .expect("parse json");

    let catalog = TranslationCatalog {
        project: CatalogProject {
            game_root: "C:\\tmp".to_owned(),
            engine: "MV".to_owned(),
            generated_at: "0".to_owned(),
        },
        spans: vec![
            span(file, "$.events.1.pages[0].list[0].parameters[0]", "Hello"),
            span(file, "$.events.1.pages[0].list[1].parameters[0]", "there"),
            span(file, "$.events.1.pages[0].list[2].parameters[0][0]", "Yes"),
            span(file, "$.events.1.pages[0].list[2].parameters[0][1]", "No"),
        ],
        units: vec![
            unit(
                "dialogue",
                file,
                "Hello\nthere",
                Some("你好\n那里"),
                vec![
                    span_id(file, "$.events.1.pages[0].list[0].parameters[0]"),
                    span_id(file, "$.events.1.pages[0].list[1].parameters[0]"),
                ],
            ),
            unit(
                "choice",
                file,
                "Yes\nNo",
                Some("是\n否"),
                vec![
                    span_id(file, "$.events.1.pages[0].list[2].parameters[0][0]"),
                    span_id(file, "$.events.1.pages[0].list[2].parameters[0][1]"),
                ],
            ),
        ],
    };

    let updated = apply_catalog_to_json_value(file, &mut value, &catalog);

    assert_eq!(updated, 4);
    assert_eq!(
        value["events"]["1"]["pages"][0]["list"][0]["parameters"][0],
        Value::String("你好".to_owned())
    );
    assert_eq!(
        value["events"]["1"]["pages"][0]["list"][1]["parameters"][0],
        Value::String("那里".to_owned())
    );
    assert_eq!(
        value["events"]["1"]["pages"][0]["list"][2]["parameters"][0][0],
        Value::String("是".to_owned())
    );
    assert_eq!(
        value["events"]["1"]["pages"][0]["list"][2]["parameters"][0][1],
        Value::String("否".to_owned())
    );
}

#[test]
fn extracts_utf8_bom_json_files() {
    let root = temp_root("bom");
    let data_dir = root.join("www").join("data");
    fs::create_dir_all(&data_dir).expect("create data dir");

    let file = data_dir.join("System.json");
    let mut payload = vec![0xEF, 0xBB, 0xBF];
    payload.extend_from_slice(br#"{"gameTitle":"BOM title"}"#);
    fs::write(&file, payload).expect("write bom json");

    let (spans, units) =
        extract_entries_from_paths(&[file.display().to_string()]).expect("extract bom json");

    assert_eq!(units.len(), 1);
    assert_eq!(spans.len(), 1);
    assert_eq!(units[0].source_text, "BOM title");

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn skips_animation_sound_effect_asset_names() {
    let root = temp_root("animations");
    let data_dir = root.join("www").join("data");
    fs::create_dir_all(&data_dir).expect("create data dir");

    fs::write(
        data_dir.join("Animations.json"),
        r#"[{"name":"ウイルス攻撃5","timings":[{"se":{"name":"Attack4"}}]}]"#,
    )
    .expect("write animations");

    let (spans, units) =
        extract_entries_from_paths(&[data_dir.join("Animations.json").display().to_string()])
            .expect("extract animations");

    assert_eq!(units.len(), 1);
    assert_eq!(spans.len(), 1);
    assert_eq!(units[0].source_text, "ウイルス攻撃5");

    fs::remove_dir_all(root).expect("cleanup");
}

fn span(file: &Path, locator: &str, source_text: &str) -> TranslationSpan {
    TranslationSpan {
        id: span_id(file, locator),
        file: file.display().to_string(),
        source_kind: SourceKind::Json,
        locator: locator.to_owned(),
        source_text: source_text.to_owned(),
        protected_tokens: Vec::new(),
        flags: Vec::new(),
    }
}

fn unit(
    semantic_kind: &str,
    file: &Path,
    source_text: &str,
    translated_text: Option<&str>,
    span_ids: Vec<String>,
) -> TranslationUnit {
    TranslationUnit {
        id: semantic_kind.to_owned(),
        group_id: semantic_kind.to_owned(),
        semantic_kind: semantic_kind.to_owned(),
        context: ContextEnvelope {
            file: file.display().to_string(),
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
        source_text: source_text.to_owned(),
        translated_text: translated_text.map(str::to_owned),
        status: TranslationStatus::Translated,
        span_ids,
    }
}

fn span_id(file: &Path, locator: &str) -> String {
    format!("{}::{locator}", file.display())
}
