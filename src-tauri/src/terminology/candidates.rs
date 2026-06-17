use crate::domain::{TranslationCatalog, TranslationStatus, TranslationUnit};
use std::collections::BTreeMap;

const CANONICAL_NAME_FILES: &[&str] = &[
    "Actors.json",
    "Armors.json",
    "Classes.json",
    "Enemies.json",
    "Items.json",
    "Skills.json",
    "States.json",
    "System.json",
    "Troops.json",
    "Weapons.json",
];

#[derive(Debug, Clone)]
pub struct AutoGlossarySeed {
    pub source_text: String,
    pub target_text: Option<String>,
    pub term_kind: String,
    pub semantic_kind: String,
    pub source_file: String,
    pub source_unit_id: String,
    pub source_json_path: Option<String>,
    pub priority: i64,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct AutoGlossaryCandidate {
    pub source_text: String,
    pub target_text: Option<String>,
    pub term_kind: String,
    pub semantic_kind: String,
    pub source_file: String,
    pub source_unit_id: String,
    pub source_json_path: Option<String>,
    pub priority: i64,
    pub status: String,
    pub conflicted: bool,
}

pub fn candidate_seeds_from_catalog(catalog: &TranslationCatalog) -> Vec<AutoGlossarySeed> {
    catalog
        .units
        .iter()
        .filter_map(candidate_seed_from_unit)
        .collect()
}

pub fn candidate_seed_from_unit(unit: &TranslationUnit) -> Option<AutoGlossarySeed> {
    if !is_canonical_name_unit(unit) {
        return None;
    }

    let source_text = unit.source_text.trim();
    if source_text.chars().count() < 2 || source_text.chars().all(|ch| ch.is_ascii_whitespace()) {
        return None;
    }

    Some(AutoGlossarySeed {
        source_text: source_text.to_owned(),
        target_text: unit
            .translated_text
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned),
        term_kind: "canonical_name".to_owned(),
        semantic_kind: unit.semantic_kind.clone(),
        source_file: unit.context.file.clone(),
        source_unit_id: unit.id.clone(),
        source_json_path: unit.context.json_path.clone(),
        priority: priority_for_unit(unit),
        status: status_for_unit(unit),
    })
}

pub fn build_auto_glossary_candidates(seeds: &[AutoGlossarySeed]) -> Vec<AutoGlossaryCandidate> {
    let mut by_source = BTreeMap::<(String, String), AutoGlossaryCandidate>::new();

    for seed in seeds {
        let key = (seed.term_kind.clone(), seed.source_text.clone());
        match by_source.get_mut(&key) {
            None => {
                by_source.insert(
                    key,
                    AutoGlossaryCandidate {
                        source_text: seed.source_text.clone(),
                        target_text: seed.target_text.clone(),
                        term_kind: seed.term_kind.clone(),
                        semantic_kind: seed.semantic_kind.clone(),
                        source_file: seed.source_file.clone(),
                        source_unit_id: seed.source_unit_id.clone(),
                        source_json_path: seed.source_json_path.clone(),
                        priority: seed.priority,
                        status: seed.status.clone(),
                        conflicted: false,
                    },
                );
            }
            Some(existing) => merge_seed(existing, seed),
        }
    }

    by_source.into_values().collect()
}

pub fn is_canonical_name_unit(unit: &TranslationUnit) -> bool {
    unit.semantic_kind == "name"
        && unit
            .context
            .json_path
            .as_deref()
            .is_some_and(is_canonical_name_json_path)
        && canonical_name_file(&unit.context.file).is_some()
}

fn merge_seed(existing: &mut AutoGlossaryCandidate, seed: &AutoGlossarySeed) {
    let existing_rank = candidate_rank(existing.priority, &existing.status, existing.target_text.as_deref());
    let incoming_rank = candidate_rank(seed.priority, &seed.status, seed.target_text.as_deref());

    match (&existing.target_text, &seed.target_text) {
        (Some(left), Some(right)) if left != right && existing_rank == incoming_rank => {
            existing.conflicted = true;
            return;
        }
        (Some(left), Some(right)) if left != right && incoming_rank > existing_rank => {
            existing.target_text = Some(right.clone());
            existing.priority = seed.priority;
            existing.status = seed.status.clone();
            existing.semantic_kind = seed.semantic_kind.clone();
            existing.source_file = seed.source_file.clone();
            existing.source_unit_id = seed.source_unit_id.clone();
            existing.source_json_path = seed.source_json_path.clone();
            existing.conflicted = false;
        }
        (None, Some(target)) if incoming_rank >= existing_rank => {
            existing.target_text = Some(target.clone());
            existing.priority = seed.priority;
            existing.status = seed.status.clone();
            existing.semantic_kind = seed.semantic_kind.clone();
            existing.source_file = seed.source_file.clone();
            existing.source_unit_id = seed.source_unit_id.clone();
            existing.source_json_path = seed.source_json_path.clone();
            existing.conflicted = false;
        }
        _ => {
            if incoming_rank > existing_rank {
                existing.priority = seed.priority;
                existing.status = seed.status.clone();
                existing.semantic_kind = seed.semantic_kind.clone();
                existing.source_file = seed.source_file.clone();
                existing.source_unit_id = seed.source_unit_id.clone();
                existing.source_json_path = seed.source_json_path.clone();
            }
        }
    }
}

fn priority_for_unit(unit: &TranslationUnit) -> i64 {
    let path = unit.context.json_path.as_deref().unwrap_or_default();
    if path.ends_with(".gameTitle") {
        120
    } else if path.ends_with(".name") {
        100
    } else {
        90
    }
}

fn status_for_unit(unit: &TranslationUnit) -> String {
    match unit.status {
        TranslationStatus::Validated => "validated",
        TranslationStatus::Translated => "translated",
        TranslationStatus::Failed => "failed",
        TranslationStatus::Pending => {
            if unit
                .translated_text
                .as_deref()
                .is_some_and(|text| !text.trim().is_empty())
            {
                "translated"
            } else {
                "pending"
            }
        }
    }
    .to_owned()
}

fn candidate_rank(priority: i64, status: &str, target_text: Option<&str>) -> i64 {
    let status_rank = match status {
        "validated" => 3,
        "translated" => 2,
        "failed" => 1,
        _ => 0,
    };
    let has_target = i64::from(target_text.is_some_and(|text| !text.is_empty()));
    priority * 10 + status_rank + has_target
}

fn is_canonical_name_json_path(path: &str) -> bool {
    path.starts_with("$[")
        && (path.ends_with(".name")
            || path.ends_with(".nickname")
            || path.ends_with(".displayName")
            || path.ends_with(".gameTitle"))
}

fn canonical_name_file(file: &str) -> Option<&'static str> {
    let normalized = file.replace('\\', "/");
    let name = normalized.rsplit('/').next()?;
    CANONICAL_NAME_FILES
        .iter()
        .copied()
        .find(|candidate| candidate.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ContextEnvelope;

    #[test]
    fn excludes_animation_resource_names() {
        let unit = unit(
            "anim",
            "www/data/Animations.json",
            "name",
            Some("$[1].timings[0].se.name"),
            "Attack4",
            None,
            TranslationStatus::Pending,
        );

        assert!(candidate_seed_from_unit(&unit).is_none());
    }

    #[test]
    fn extracts_strong_database_name_fields() {
        let unit = unit(
            "actor",
            "www/data/Actors.json",
            "name",
            Some("$[1].name"),
            "ロマーシャ",
            Some("罗玛夏"),
            TranslationStatus::Validated,
        );

        let seed = candidate_seed_from_unit(&unit).expect("candidate");

        assert_eq!(seed.term_kind, "canonical_name");
        assert_eq!(seed.source_text, "ロマーシャ");
        assert_eq!(seed.target_text.as_deref(), Some("罗玛夏"));
        assert_eq!(seed.priority, 100);
        assert_eq!(seed.status, "validated");
    }

    #[test]
    fn marks_conflicts_when_same_source_has_equal_rank_targets() {
        let candidates = build_auto_glossary_candidates(&[
            AutoGlossarySeed {
                source_text: "ロマーシャ".to_owned(),
                target_text: Some("罗玛夏".to_owned()),
                term_kind: "canonical_name".to_owned(),
                semantic_kind: "name".to_owned(),
                source_file: "www/data/Actors.json".to_owned(),
                source_unit_id: "u1".to_owned(),
                source_json_path: Some("$[1].name".to_owned()),
                priority: 100,
                status: "validated".to_owned(),
            },
            AutoGlossarySeed {
                source_text: "ロマーシャ".to_owned(),
                target_text: Some("罗马夏".to_owned()),
                term_kind: "canonical_name".to_owned(),
                semantic_kind: "name".to_owned(),
                source_file: "www/data/Actors.json".to_owned(),
                source_unit_id: "u2".to_owned(),
                source_json_path: Some("$[2].name".to_owned()),
                priority: 100,
                status: "validated".to_owned(),
            },
        ]);

        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].conflicted);
    }

    fn unit(
        id: &str,
        file: &str,
        semantic_kind: &str,
        json_path: Option<&str>,
        source_text: &str,
        translated_text: Option<&str>,
        status: TranslationStatus,
    ) -> TranslationUnit {
        TranslationUnit {
            id: id.to_owned(),
            group_id: id.to_owned(),
            semantic_kind: semantic_kind.to_owned(),
            context: ContextEnvelope {
                file: file.to_owned(),
                json_path: json_path.map(str::to_owned),
                map_id: None,
                event_id: None,
                page_id: None,
                command_index: None,
                speaker_name: None,
                prev_texts: Vec::new(),
                next_texts: Vec::new(),
                block_text: None,
                glossary_hits: Vec::new(),
                notes: vec!["field_extraction".to_owned()],
            },
            source_text: source_text.to_owned(),
            translated_text: translated_text.map(str::to_owned),
            status,
            span_ids: Vec::new(),
        }
    }
}
