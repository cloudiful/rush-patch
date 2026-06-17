use super::{PreparedUserPrompt, PromptError};
use crate::terminology::CanonicalTerm;
use serde::Serialize;
use std::collections::BTreeMap;

use super::schema::PromptRenderItemSeed;

#[derive(Debug, Clone)]
pub(crate) struct PromptSeedGroup<'a> {
    pub(crate) seeds: Vec<&'a PromptRenderItemSeed>,
}

#[derive(Debug, Serialize)]
struct GroupedPromptEnvelope {
    #[serde(rename = "g", skip_serializing_if = "Vec::is_empty")]
    glossary_terms: Vec<PromptGlossaryTerm>,
    #[serde(rename = "groups")]
    groups: Vec<PromptGroupEnvelope>,
}

#[derive(Debug, Serialize)]
struct PromptGroupEnvelope {
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

pub(crate) fn build_grouped_user_prompt_from_seed_groups_with_terms(
    groups: &[PromptSeedGroup<'_>],
    glossary_terms: &[CanonicalTerm],
) -> Result<PreparedUserPrompt, PromptError> {
    let mut alias_to_real_id = BTreeMap::new();
    let mut alias_index = 1usize;
    for group in groups {
        for seed in &group.seeds {
            alias_to_real_id.insert(format!("u{alias_index}"), seed.real_id.clone());
            alias_index += 1;
        }
    }

    let body = render_grouped_prompt_body_from_seed_groups_with_terms(groups, glossary_terms)?;
    Ok(PreparedUserPrompt { body, alias_to_real_id })
}

pub(crate) fn render_grouped_prompt_body_from_seed_groups_with_terms(
    groups: &[PromptSeedGroup<'_>],
    glossary_terms: &[CanonicalTerm],
) -> Result<String, PromptError> {
    let mut alias_index = 1usize;
    let groups = groups
        .iter()
        .map(|group| {
            let shared_file = shared_seed_file_path(&group.seeds);
            let shared_speaker = shared_seed_speaker(&group.seeds);
            let shared_kind = shared_seed_semantic_kind(&group.seeds);
            let shared_scene = shared_seed_scene_key(&group.seeds);
            let shared_record = shared_seed_record_key(&group.seeds);
            let precedings = shared_seed_precedings(&group.seeds);
            let followings = shared_seed_followings(&group.seeds);
            let items = group
                .seeds
                .iter()
                .map(|seed| {
                    let item = PromptItem {
                        id: format!("u{alias_index}"),
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
                    };
                    alias_index += 1;
                    item
                })
                .collect::<Vec<_>>();

            PromptGroupEnvelope {
                shared_file,
                shared_speaker,
                shared_kind,
                shared_scene,
                shared_record,
                precedings,
                followings,
                items,
            }
        })
        .collect::<Vec<_>>();

    let glossary_terms = glossary_terms
        .iter()
        .map(|term| PromptGlossaryTerm(term.source.clone(), term.target.clone()))
        .collect::<Vec<_>>();

    serde_json::to_string(&GroupedPromptEnvelope {
        glossary_terms,
        groups,
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
