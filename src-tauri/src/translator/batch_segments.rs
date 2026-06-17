use crate::domain::TranslationUnit;

#[derive(Debug, Clone, PartialEq, Eq)]
enum SegmentKey {
    EventPage {
        file: String,
        map_id: Option<u32>,
        event_id: Option<u32>,
        page_id: Option<u32>,
    },
    DatabaseRecord(String),
    DatabaseFileSemantic {
        file: String,
        semantic_kind: String,
    },
    FileWindow(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PendingSegmentKind {
    EventScene,
    DatabaseLike,
    FileWindow,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingSegment {
    pub(crate) kind: PendingSegmentKind,
    pub(crate) file: String,
    pub(super) indices: Vec<usize>,
    pub(crate) source_segment_count: usize,
    pub(crate) source_char_count: usize,
}

impl PendingSegment {
    pub(crate) fn new(
        kind: PendingSegmentKind,
        file: String,
        indices: Vec<usize>,
        source_char_count: usize,
    ) -> Self {
        Self {
            kind,
            file,
            indices,
            source_segment_count: 1,
            source_char_count,
        }
    }
}

pub(super) fn build_segments(
    all_units: &[TranslationUnit],
    pending_indices: &[usize],
) -> Vec<PendingSegment> {
    let mut segments = Vec::new();
    let mut current_key = None;
    let mut current_indices = Vec::new();
    let mut current_kind = None;
    let mut current_file = None;
    let mut current_source_char_count = 0usize;

    for index in pending_indices {
        let next_descriptor = segment_descriptor(&all_units[*index]);
        let next_key = next_descriptor.key;
        if current_key.as_ref().is_some_and(|key| key != &next_key) && !current_indices.is_empty() {
            segments.push(PendingSegment::new(
                current_kind.expect("current segment kind"),
                current_file.clone().expect("current segment file"),
                std::mem::take(&mut current_indices),
                current_source_char_count,
            ));
            current_source_char_count = 0;
        }

        current_key = Some(next_key);
        current_kind = Some(next_descriptor.kind);
        current_file = Some(next_descriptor.file);
        current_indices.push(*index);
        current_source_char_count += all_units[*index].source_text.chars().count();
    }

    if !current_indices.is_empty() {
        segments.push(PendingSegment::new(
            current_kind.expect("current segment kind"),
            current_file.expect("current segment file"),
            current_indices,
            current_source_char_count,
        ));
    }

    segments
}

struct SegmentDescriptor {
    key: SegmentKey,
    kind: PendingSegmentKind,
    file: String,
}

fn segment_descriptor(unit: &TranslationUnit) -> SegmentDescriptor {
    if is_event_scene_unit(unit) {
        SegmentDescriptor {
            key: SegmentKey::EventPage {
                file: unit.context.file.clone(),
                map_id: unit.context.map_id,
                event_id: unit.context.event_id,
                page_id: unit.context.page_id,
            },
            kind: PendingSegmentKind::EventScene,
            file: unit.context.file.clone(),
        }
    } else if let Some(record_key) = record_key(unit) {
        SegmentDescriptor {
            key: SegmentKey::DatabaseRecord(record_key),
            kind: PendingSegmentKind::DatabaseLike,
            file: unit.context.file.clone(),
        }
    } else if unit.context.json_path.is_some() {
        SegmentDescriptor {
            key: SegmentKey::DatabaseFileSemantic {
                file: unit.context.file.clone(),
                semantic_kind: unit.semantic_kind.clone(),
            },
            kind: PendingSegmentKind::DatabaseLike,
            file: unit.context.file.clone(),
        }
    } else {
        SegmentDescriptor {
            key: SegmentKey::FileWindow(unit.context.file.clone()),
            kind: PendingSegmentKind::FileWindow,
            file: unit.context.file.clone(),
        }
    }
}

fn is_event_scene_unit(unit: &TranslationUnit) -> bool {
    unit.context.notes.iter().any(|note| {
        matches!(
            note.as_str(),
            "event_dialogue_block" | "event_scroll_text_block" | "choice_group"
        )
    })
}

fn record_key(unit: &TranslationUnit) -> Option<String> {
    let path = unit.context.json_path.as_deref()?;
    if !path.starts_with("$[") {
        return None;
    }
    let end = path.find(']')?;
    Some(format!("{}::{}", unit.context.file, &path[..=end]))
}
