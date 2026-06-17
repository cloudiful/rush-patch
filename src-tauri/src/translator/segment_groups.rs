use super::batch_segments::{PendingSegment, PendingSegmentKind};
use crate::domain::TranslationUnit;

const SMALL_SEGMENT_MAX_UNITS: usize = 12;
const MAX_GROUPED_SEGMENT_UNITS: usize = 48;
const MAX_GROUPED_SEGMENT_SOURCE_CHARS: usize = 6_000;

pub(super) fn merge_small_segments(
    all_units: &[TranslationUnit],
    raw_segments: Vec<PendingSegment>,
) -> Vec<PendingSegment> {
    let mut grouped = Vec::new();
    let mut current: Option<PendingSegment> = None;

    for next in raw_segments {
        match current.as_mut() {
            Some(active) if can_merge_segments(all_units, active, &next) => {
                active.indices.extend(next.indices.iter().copied());
                active.source_segment_count += next.source_segment_count;
                active.source_char_count += next.source_char_count;
            }
            Some(_) => {
                grouped.push(current.take().expect("active segment present"));
                current = Some(next);
            }
            None => current = Some(next),
        }
    }

    if let Some(active) = current {
        grouped.push(active);
    }

    grouped
}

fn can_merge_segments(
    all_units: &[TranslationUnit],
    current: &PendingSegment,
    next: &PendingSegment,
) -> bool {
    if !matches!(current.kind, PendingSegmentKind::DatabaseLike)
        || !matches!(next.kind, PendingSegmentKind::DatabaseLike)
    {
        return false;
    }
    if current.file != next.file {
        return false;
    }

    let current_is_small =
        current.indices.len() <= SMALL_SEGMENT_MAX_UNITS || current.source_segment_count > 1;
    let next_is_small = next.indices.len() <= SMALL_SEGMENT_MAX_UNITS;
    if !(current_is_small && next_is_small) {
        return false;
    }

    if current.indices.len() + next.indices.len() > MAX_GROUPED_SEGMENT_UNITS {
        return false;
    }
    if current.source_char_count + next.source_char_count > MAX_GROUPED_SEGMENT_SOURCE_CHARS {
        return false;
    }

    database_like_segment(all_units, current) && database_like_segment(all_units, next)
}

fn database_like_segment(all_units: &[TranslationUnit], segment: &PendingSegment) -> bool {
    segment
        .indices
        .iter()
        .all(|index| all_units[*index].context.json_path.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ContextEnvelope, TranslationStatus};

    #[test]
    fn merges_adjacent_small_database_segments_from_same_file() {
        let units = vec![
            field_unit("u1", "Armors.json", "$[1].name", "Potion"),
            field_unit("u2", "Armors.json", "$[2].name", "Ether"),
        ];
        let raw = vec![
            PendingSegment::new(
                PendingSegmentKind::DatabaseLike,
                "Armors.json".to_owned(),
                vec![0],
                source_chars(&units, &[0]),
            ),
            PendingSegment::new(
                PendingSegmentKind::DatabaseLike,
                "Armors.json".to_owned(),
                vec![1],
                source_chars(&units, &[1]),
            ),
        ];

        let grouped = merge_small_segments(&units, raw);

        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped[0].indices, vec![0, 1]);
        assert_eq!(grouped[0].source_segment_count, 2);
    }

    #[test]
    fn does_not_merge_event_segments() {
        let units = vec![
            event_unit("u1", Some(1), Some(1), Some(0), "A"),
            event_unit("u2", Some(1), Some(2), Some(0), "B"),
        ];
        let raw = vec![
            PendingSegment::new(
                PendingSegmentKind::EventScene,
                "Map001.json".to_owned(),
                vec![0],
                source_chars(&units, &[0]),
            ),
            PendingSegment::new(
                PendingSegmentKind::EventScene,
                "Map001.json".to_owned(),
                vec![1],
                source_chars(&units, &[1]),
            ),
        ];

        let grouped = merge_small_segments(&units, raw);

        assert_eq!(grouped.len(), 2);
    }

    fn source_chars(units: &[TranslationUnit], indices: &[usize]) -> usize {
        indices
            .iter()
            .map(|index| units[*index].source_text.chars().count())
            .sum()
    }

    fn unit(
        id: &str,
        file: &str,
        json_path: Option<&str>,
        source_text: &str,
        notes: Vec<&str>,
        map_id: Option<u32>,
        event_id: Option<u32>,
        page_id: Option<u32>,
    ) -> TranslationUnit {
        TranslationUnit {
            id: id.to_owned(),
            group_id: id.to_owned(),
            semantic_kind: "dialogue".to_owned(),
            context: ContextEnvelope {
                file: file.to_owned(),
                json_path: json_path.map(str::to_owned),
                map_id,
                event_id,
                page_id,
                command_index: None,
                speaker_name: None,
                prev_texts: Vec::new(),
                next_texts: Vec::new(),
                block_text: None,
                glossary_hits: Vec::new(),
                notes: notes.into_iter().map(str::to_owned).collect(),
            },
            source_text: source_text.to_owned(),
            translated_text: None,
            status: TranslationStatus::Pending,
            span_ids: Vec::new(),
        }
    }

    fn field_unit(id: &str, file: &str, json_path: &str, source_text: &str) -> TranslationUnit {
        unit(
            id,
            file,
            Some(json_path),
            source_text,
            vec!["field_extraction"],
            None,
            None,
            None,
        )
    }

    fn event_unit(
        id: &str,
        map_id: Option<u32>,
        event_id: Option<u32>,
        page_id: Option<u32>,
        source_text: &str,
    ) -> TranslationUnit {
        unit(
            id,
            "Map001.json",
            None,
            source_text,
            vec!["event_dialogue_block"],
            map_id,
            event_id,
            page_id,
        )
    }
}
