use super::batch_segments::{PendingSegment, PendingSegmentKind};
use crate::domain::BatchingStrategy;
use crate::domain::TranslationUnit;
use std::path::Path;

const ORPHAN_MAX_UNITS: usize = 4;
const ORPHAN_MAX_SOURCE_CHARS: usize = 240;

pub(crate) fn segment_pool_candidate_rejection(
    all_units: &[TranslationUnit],
    segment: &PendingSegment,
    strategy: BatchingStrategy,
) -> Option<String> {
    match strategy {
        BatchingStrategy::QualityFirst => {
            if !has_groupable_pool_shape(segment) {
                Some("quality_first_shape".to_owned())
            } else if segment.indices.is_empty() {
                Some("empty_segment".to_owned())
            } else if segment.indices.len() > ORPHAN_MAX_UNITS {
                Some(format!("quality_first_unit_limit>{ORPHAN_MAX_UNITS}"))
            } else if segment.source_char_count > ORPHAN_MAX_SOURCE_CHARS {
                Some(format!("quality_first_char_limit>{ORPHAN_MAX_SOURCE_CHARS}"))
            } else {
                segment
                    .indices
                    .iter()
                    .find_map(|index| quality_first_unit_rejection(&all_units[*index]))
            }
        }
        BatchingStrategy::MaximizeUtilization => {
            if !has_maximize_pool_shape(segment) {
                Some("maximize_shape".to_owned())
            } else if segment.indices.is_empty() {
                Some("empty_segment".to_owned())
            } else {
                segment
                    .indices
                    .iter()
                    .find_map(|index| maximize_unit_rejection(&all_units[*index]))
            }
        }
    }
}

fn has_groupable_pool_shape(segment: &PendingSegment) -> bool {
    if !matches!(segment.kind, PendingSegmentKind::DatabaseLike | PendingSegmentKind::FileWindow) {
        return false;
    }
    true
}

fn has_maximize_pool_shape(segment: &PendingSegment) -> bool {
    match segment.kind {
        PendingSegmentKind::EventScene => is_common_events_file(&segment.file),
        PendingSegmentKind::DatabaseLike | PendingSegmentKind::FileWindow => true,
    }
}

fn quality_first_unit_rejection(unit: &TranslationUnit) -> Option<String> {
    if unit.context.notes.iter().any(|note| {
        matches!(
            note.as_str(),
            "event_dialogue_block" | "event_scroll_text_block" | "choice_group"
        )
    }) {
        return Some("quality_first_event_notes".to_owned());
    }

    if !matches!(
        unit.semantic_kind.as_str(),
        "name" | "description" | "text" | "system"
    ) {
        return Some(format!("semantic_kind={}", unit.semantic_kind));
    }

    if unit
        .context
        .speaker_name
        .as_deref()
        .is_some_and(|speaker| !speaker.trim().is_empty())
    {
        return Some("speaker_name_present".to_owned());
    }

    None
}
fn maximize_unit_rejection(unit: &TranslationUnit) -> Option<String> {
    if unit.context.notes.iter().any(|note| {
        matches!(
            note.as_str(),
            "event_dialogue_block" | "event_scroll_text_block" | "choice_group"
        )
    }) {
        return if matches!(unit.semantic_kind.as_str(), "dialogue" | "text") {
            None
        } else {
            Some(format!("event_semantic_kind={}", unit.semantic_kind))
        };
    }

    if matches!(
        unit.semantic_kind.as_str(),
        "name" | "description" | "text" | "system"
    ) {
        None
    } else {
        Some(format!("semantic_kind={}", unit.semantic_kind))
    }
}

pub(crate) fn segment_pool_key(segment: &PendingSegment, strategy: BatchingStrategy) -> String {
    if matches!(strategy, BatchingStrategy::MaximizeUtilization) {
        return "maximize_utilization#global_pool".to_owned();
    }

    let normalized_file = normalize_file_key(&segment.file);
    match segment.kind {
        PendingSegmentKind::EventScene => format!("{normalized_file}#event_scene"),
        PendingSegmentKind::DatabaseLike | PendingSegmentKind::FileWindow => {
            orphan_pool_directory(&segment.file)
        }
    }
}

pub(crate) fn orphan_pool_directory(file: &str) -> String {
    Path::new(&normalize_file_key(file))
        .parent()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn normalize_file_key(file: &str) -> String {
    file.replace('\\', "/").to_ascii_lowercase()
}

fn is_common_events_file(file: &str) -> bool {
    normalize_file_key(file).ends_with("/commonevents.json")
        || normalize_file_key(file) == "commonevents.json"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ContextEnvelope, TranslationStatus};

    #[test]
    fn normalizes_pool_directory_across_path_separators() {
        assert_eq!(
            orphan_pool_directory(r"www\data\CommonEvents.json"),
            "www/data"
        );
    }

    #[test]
    fn event_scene_segment_is_not_groupable_orphan() {
        let units = vec![event_unit("u1", "www/data/Map001.json", "B")];
        let segment = PendingSegment::new(
            PendingSegmentKind::EventScene,
            "www/data/Map001.json".to_owned(),
            vec![0],
            1,
        );

        assert_eq!(
            segment_pool_candidate_rejection(&units, &segment, BatchingStrategy::QualityFirst),
            Some("quality_first_shape".to_owned())
        );
    }

    #[test]
    fn maximize_utilization_allows_common_event_scene_pooling_with_speaker() {
        let units = vec![event_unit("u1", "www/data/CommonEvents.json", "B")];
        let segment = PendingSegment::new(
            PendingSegmentKind::EventScene,
            "www/data/CommonEvents.json".to_owned(),
            vec![0],
            1,
        );

        assert_eq!(
            segment_pool_candidate_rejection(
                &units,
                &segment,
                BatchingStrategy::MaximizeUtilization
            ),
            None
        );
        assert_eq!(
            segment_pool_candidate_rejection(&units, &segment, BatchingStrategy::QualityFirst),
            Some("quality_first_shape".to_owned())
        );
        assert_eq!(
            segment_pool_key(&segment, BatchingStrategy::QualityFirst),
            "www/data/commonevents.json#event_scene"
        );
        assert_eq!(
            segment_pool_key(&segment, BatchingStrategy::MaximizeUtilization),
            "maximize_utilization#global_pool"
        );
    }

    #[test]
    fn maximize_utilization_allows_choice_groups_for_common_events() {
        let mut unit = event_unit("u1", "www/data/CommonEvents.json", "A");
        unit.semantic_kind = "dialogue".to_owned();
        unit.context.notes = vec!["choice_group".to_owned()];
        let segment = PendingSegment::new(
            PendingSegmentKind::EventScene,
            "www/data/CommonEvents.json".to_owned(),
            vec![0],
            1,
        );

        assert_eq!(
            segment_pool_candidate_rejection(
                &[unit.clone()],
                &segment,
                BatchingStrategy::MaximizeUtilization
            ),
            None
        );
        assert_eq!(
            segment_pool_candidate_rejection(&[unit], &segment, BatchingStrategy::QualityFirst),
            Some("quality_first_shape".to_owned())
        );
    }

    fn event_unit(id: &str, file: &str, source_text: &str) -> TranslationUnit {
        TranslationUnit {
            id: id.to_owned(),
            group_id: id.to_owned(),
            semantic_kind: "dialogue".to_owned(),
            context: ContextEnvelope {
                file: file.to_owned(),
                json_path: None,
                map_id: Some(1),
                event_id: Some(1),
                page_id: Some(0),
                command_index: None,
                speaker_name: Some("npc".to_owned()),
                prev_texts: Vec::new(),
                next_texts: Vec::new(),
                block_text: Some(source_text.to_owned()),
                glossary_hits: Vec::new(),
                notes: vec!["event_dialogue_block".to_owned()],
            },
            source_text: source_text.to_owned(),
            translated_text: None,
            status: TranslationStatus::Pending,
            span_ids: Vec::new(),
        }
    }
}
