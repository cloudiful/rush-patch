use super::{PreparedUserPrompt, PromptError};
use crate::domain::{TranslationSpan, TranslationUnit};
use crate::terminology::CanonicalTerm;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Serialize)]
struct PromptEnvelope {
    #[serde(rename = "f", skip_serializing_if = "Option::is_none")]
    shared_file: Option<String>,
    #[serde(rename = "sp", skip_serializing_if = "Option::is_none")]
    shared_speaker: Option<String>,
    #[serde(rename = "k", skip_serializing_if = "Option::is_none")]
    shared_kind: Option<String>,
    #[serde(rename = "scene", skip_serializing_if = "Option::is_none")]
    shared_scene: Option<String>,
    #[serde(rename = "record", skip_serializing_if = "Option::is_none")]
    shared_record: Option<String>,
    #[serde(rename = "pre", skip_serializing_if = "Vec::is_empty")]
    precedings: Vec<String>,
    #[serde(rename = "post", skip_serializing_if = "Vec::is_empty")]
    followings: Vec<String>,
    #[serde(rename = "g", skip_serializing_if = "Vec::is_empty")]
    glossary_terms: Vec<PromptGlossaryTerm>,
    #[serde(rename = "i")]
    items: Vec<PromptItem>,
}

#[derive(Debug, Serialize)]
struct PromptGlossaryTerm(String, String);

#[derive(Debug, Serialize)]
struct PromptItem {
    id: String,
    #[serde(rename = "k", skip_serializing_if = "Option::is_none")]
    semantic_kind: Option<String>,
    #[serde(rename = "src")]
    source_text: String,
    #[serde(rename = "ctx", skip_serializing_if = "Option::is_none")]
    context: Option<PromptContext>,
    #[serde(rename = "tok", skip_serializing_if = "Vec::is_empty")]
    protected_tokens: Vec<String>,
    #[serde(rename = "n", skip_serializing_if = "Vec::is_empty")]
    notes: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
struct PromptContext {
    #[serde(rename = "f", skip_serializing_if = "Option::is_none")]
    file: Option<String>,
    #[serde(rename = "p", skip_serializing_if = "Option::is_none")]
    json_path: Option<String>,
    #[serde(rename = "sp", skip_serializing_if = "Option::is_none")]
    speaker_name: Option<String>,
    #[serde(rename = "r", skip_serializing_if = "Option::is_none")]
    record_key: Option<String>,
    #[serde(rename = "sc", skip_serializing_if = "Option::is_none")]
    scene_key: Option<String>,
}

impl PromptContext {
    fn is_empty(&self) -> bool {
        self.file.is_none()
            && self.json_path.is_none()
            && self.speaker_name.is_none()
            && self.record_key.is_none()
            && self.scene_key.is_none()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PromptRenderItemSeed {
    pub(crate) real_id: String,
    pub(crate) semantic_kind: String,
    pub(crate) source_text: String,
    pub(crate) compact_file: Option<String>,
    pub(crate) json_path: Option<String>,
    pub(crate) speaker_name: Option<String>,
    pub(crate) record_key: Option<String>,
    pub(crate) scene_key: Option<String>,
    pub(crate) protected_tokens: Vec<String>,
    pub(crate) notes: Vec<String>,
    pub(crate) prev_texts: Vec<String>,
    pub(crate) next_texts: Vec<String>,
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn build_user_prompt(
    units: &[TranslationUnit],
    spans: &[TranslationSpan],
) -> Result<PreparedUserPrompt, PromptError> {
    build_user_prompt_with_terms(units, spans, &[])
}

pub fn build_user_prompt_with_terms(
    units: &[TranslationUnit],
    spans: &[TranslationSpan],
    glossary_terms: &[CanonicalTerm],
) -> Result<PreparedUserPrompt, PromptError> {
    let seeds = build_prompt_render_seeds(units, spans);
    let alias_to_real_id = seeds
        .iter()
        .enumerate()
        .map(|(index, seed)| (format!("u{}", index + 1), seed.real_id.clone()))
        .collect::<BTreeMap<_, _>>();
    let seed_refs = seeds.iter().collect::<Vec<_>>();
    let body = render_prompt_body_from_seeds_with_terms(&seed_refs, glossary_terms)?;
    Ok(PreparedUserPrompt {
        body,
        alias_to_real_id,
    })
}

pub(crate) fn build_prompt_render_seeds(
    units: &[TranslationUnit],
    spans: &[TranslationSpan],
) -> Vec<PromptRenderItemSeed> {
    let span_lookup = spans
        .iter()
        .map(|span| (span.id.as_str(), span))
        .collect::<HashMap<_, _>>();
    units
        .iter()
        .map(|unit| seed_for_unit(unit, &span_lookup))
        .collect()
}

#[allow(dead_code)]
pub(crate) fn render_prompt_body_from_seeds(
    seeds: &[&PromptRenderItemSeed],
) -> Result<String, PromptError> {
    render_prompt_body_from_seeds_with_terms(seeds, &[])
}

pub(crate) fn render_prompt_body_from_seeds_with_terms(
    seeds: &[&PromptRenderItemSeed],
    glossary_terms: &[CanonicalTerm],
) -> Result<String, PromptError> {
    let shared_file = shared_seed_file_path(seeds);
    let shared_speaker = shared_seed_speaker(seeds);
    let shared_kind = shared_seed_semantic_kind(seeds);
    let shared_scene = shared_seed_scene_key(seeds);
    let shared_record = shared_seed_record_key(seeds);
    let precedings = shared_seed_precedings(seeds);
    let followings = shared_seed_followings(seeds);
    let glossary_terms = glossary_terms
        .iter()
        .map(|term| PromptGlossaryTerm(term.source.clone(), term.target.clone()))
        .collect::<Vec<_>>();
    let items = seeds
        .iter()
        .enumerate()
        .map(|(index, seed)| PromptItem {
            id: format!("u{}", index + 1),
            semantic_kind: seed_semantic_kind(seed, shared_kind.as_deref()),
            source_text: seed.source_text.clone(),
            context: compact_seed_context(
                seed,
                shared_file.as_deref(),
                shared_speaker.as_deref(),
                shared_record.as_deref(),
                shared_scene.as_deref(),
            ),
            protected_tokens: seed.protected_tokens.clone(),
            notes: seed.notes.clone(),
        })
        .collect::<Vec<_>>();

    serde_json::to_string(&PromptEnvelope {
        shared_file,
        shared_speaker,
        shared_kind,
        shared_scene,
        shared_record,
        precedings,
        followings,
        glossary_terms,
        items,
    })
    .map_err(PromptError::from)
}

fn seed_semantic_kind(seed: &PromptRenderItemSeed, shared_kind: Option<&str>) -> Option<String> {
    match shared_kind {
        Some(kind) if kind == seed.semantic_kind => None,
        _ => Some(seed.semantic_kind.clone()),
    }
}

fn compact_seed_context(
    seed: &PromptRenderItemSeed,
    shared_file: Option<&str>,
    shared_speaker: Option<&str>,
    shared_record: Option<&str>,
    shared_scene: Option<&str>,
) -> Option<PromptContext> {
    let context = PromptContext {
        file: match (seed.compact_file.as_deref(), shared_file) {
            (Some(file), Some(shared)) if file == shared => None,
            _ => seed.compact_file.clone(),
        },
        json_path: seed.json_path.clone(),
        speaker_name: match (seed.speaker_name.as_deref(), shared_speaker) {
            (Some(speaker), Some(shared)) if speaker == shared => None,
            (Some(speaker), _) => Some(speaker.to_owned()),
            _ => None,
        },
        record_key: match (seed.record_key.as_deref(), shared_record) {
            (Some(record), Some(shared)) if record == shared => None,
            (Some(record), _) => Some(record.to_owned()),
            _ => None,
        },
        scene_key: match (seed.scene_key.as_deref(), shared_scene) {
            (Some(scene), Some(shared)) if scene == shared => None,
            (Some(scene), _) => Some(scene.to_owned()),
            _ => None,
        },
    };
    (!context.is_empty()).then_some(context)
}

fn include_json_path(unit: &TranslationUnit) -> bool {
    !unit.context.notes.iter().any(|note| {
        matches!(
            note.as_str(),
            "event_dialogue_block" | "event_scroll_text_block" | "choice_group"
        )
    })
}

fn compact_file_path(file: &str) -> Option<String> {
    let normalized = file.replace('\\', "/");
    if let Some(index) = normalized.find("/www/") {
        return Some(normalized[index + 1..].to_owned());
    }
    if let Some(index) = normalized.find("www/") {
        return Some(normalized[index..].to_owned());
    }
    normalized
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

fn seed_for_unit(
    unit: &TranslationUnit,
    span_lookup: &HashMap<&str, &TranslationSpan>,
) -> PromptRenderItemSeed {
    PromptRenderItemSeed {
        real_id: unit.id.clone(),
        semantic_kind: unit.semantic_kind.clone(),
        source_text: unit.source_text.clone(),
        compact_file: compact_file_path(&unit.context.file),
        json_path: include_json_path(unit)
            .then(|| unit.context.json_path.clone())
            .flatten(),
        speaker_name: unit.context.speaker_name.clone(),
        record_key: record_key(unit),
        scene_key: scene_key(unit),
        protected_tokens: protected_tokens_for_unit(unit, span_lookup),
        notes: prompt_notes(unit),
        prev_texts: unit.context.prev_texts.clone(),
        next_texts: unit.context.next_texts.clone(),
    }
}

fn shared_seed_file_path(seeds: &[&PromptRenderItemSeed]) -> Option<String> {
    let first = seeds.first()?.compact_file.clone()?;
    seeds
        .iter()
        .all(|seed| seed.compact_file.as_deref() == Some(first.as_str()))
        .then_some(first)
}

fn shared_seed_speaker(seeds: &[&PromptRenderItemSeed]) -> Option<String> {
    let first = seeds.first()?.speaker_name.as_deref()?;
    seeds
        .iter()
        .all(|seed| seed.speaker_name.as_deref() == Some(first))
        .then(|| first.to_owned())
}

fn shared_seed_semantic_kind(seeds: &[&PromptRenderItemSeed]) -> Option<String> {
    let first = seeds.first()?.semantic_kind.as_str();
    seeds
        .iter()
        .all(|seed| seed.semantic_kind == first)
        .then(|| first.to_owned())
}

fn shared_seed_scene_key(seeds: &[&PromptRenderItemSeed]) -> Option<String> {
    let first = seeds.first()?.scene_key.clone()?;
    seeds
        .iter()
        .all(|seed| seed.scene_key.as_deref() == Some(first.as_str()))
        .then_some(first)
}

fn shared_seed_record_key(seeds: &[&PromptRenderItemSeed]) -> Option<String> {
    let first = seeds.first()?.record_key.clone()?;
    seeds
        .iter()
        .all(|seed| seed.record_key.as_deref() == Some(first.as_str()))
        .then_some(first)
}

fn shared_seed_precedings(seeds: &[&PromptRenderItemSeed]) -> Vec<String> {
    seeds
        .first()
        .map(|seed| seed.prev_texts.clone())
        .unwrap_or_default()
}

fn shared_seed_followings(seeds: &[&PromptRenderItemSeed]) -> Vec<String> {
    seeds
        .last()
        .map(|seed| seed.next_texts.clone())
        .unwrap_or_default()
}

fn protected_tokens_for_unit(
    unit: &TranslationUnit,
    span_lookup: &HashMap<&str, &TranslationSpan>,
) -> Vec<String> {
    let mut tokens = unit
        .span_ids
        .iter()
        .filter_map(|span_id| span_lookup.get(span_id.as_str()).copied())
        .flat_map(|span| span.protected_tokens.iter().cloned())
        .collect::<Vec<_>>();
    tokens.sort_unstable();
    tokens.dedup();
    tokens
}

fn prompt_notes(unit: &TranslationUnit) -> Vec<String> {
    unit.context
        .notes
        .iter()
        .filter(|note| note.as_str() != "field_extraction" && !note.starts_with("quote:"))
        .cloned()
        .collect()
}

fn record_key(unit: &TranslationUnit) -> Option<String> {
    let path = unit.context.json_path.as_deref()?;
    if !path.starts_with("$[") {
        return None;
    }
    let end = path.find(']')?;
    Some(format!(
        "{}::{}",
        compact_file_path(&unit.context.file)?,
        &path[..=end]
    ))
}

fn scene_key(unit: &TranslationUnit) -> Option<String> {
    let is_event_group = unit.context.notes.iter().any(|note| {
        matches!(
            note.as_str(),
            "event_dialogue_block" | "event_scroll_text_block" | "choice_group"
        )
    });
    if !is_event_group {
        return None;
    }
    Some(format!(
        "{}#m{}e{}p{}",
        compact_file_path(&unit.context.file)?,
        unit.context.map_id.unwrap_or(0),
        unit.context.event_id.unwrap_or(0),
        unit.context.page_id.unwrap_or(0)
    ))
}
