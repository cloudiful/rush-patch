use super::batch_segments::build_segments;
use super::batching::{
    build_pending_segments, create_planning_context, plan_batches_for_segment,
    plan_translation_batches,
};
use crate::domain::{BatchingStrategy, ContextEnvelope, TranslationSpan, TranslationStatus, TranslationUnit};
use crate::prompting;
use crate::terminology::TermMatchIndex;

const MAX_ITEMS_PER_REQUEST: usize = 64;

#[test]
fn optimized_planner_keeps_reference_batch_boundaries() {
    let units = vec![
        unit("a", "Map001.json", "long source ".repeat(160), Some(1)),
        unit("b", "Map001.json", "long source ".repeat(160), Some(1)),
        unit("c", "Map001.json", "long source ".repeat(160), Some(1)),
        unit("d", "Map001.json", "short".to_owned(), Some(2)),
    ];
    let pending = (0..units.len()).collect::<Vec<_>>();
    let model = "gpt-4.1-mini";
    let system_prompt = "Translate.";
    let max_input_tokens = 280;

    let optimized = plan_translation_batches(
        &units,
        &[],
        &pending,
        model,
        system_prompt,
        max_input_tokens,
    )
    .expect("optimized plan")
    .into_iter()
    .map(|batch| batch.indices)
    .collect::<Vec<_>>();
    let reference = reference_plan_indices(
        &units,
        &[],
        &pending,
        model,
        system_prompt,
        max_input_tokens,
    )
    .expect("reference plan");

    assert_eq!(optimized, reference);
}

#[test]
fn segment_level_planner_matches_whole_planner_boundaries() {
    let units = vec![
        unit("a", "Map001.json", "long source ".repeat(160), Some(1)),
        unit("b", "Map001.json", "long source ".repeat(160), Some(1)),
        unit("c", "Map001.json", "long source ".repeat(160), Some(2)),
        unit("d", "System.json", "Potion".to_owned(), None),
        unit("e", "System.json", "Ether".to_owned(), None),
    ];
    let pending = (0..units.len()).collect::<Vec<_>>();
    let model = "gpt-4.1-mini";
    let system_prompt = "Translate.";
    let max_input_tokens = 280;

    let whole = plan_translation_batches(
        &units,
        &[],
        &pending,
        model,
        system_prompt,
        max_input_tokens,
    )
    .expect("whole plan")
    .into_iter()
    .map(|batch| batch.indices)
    .collect::<Vec<_>>();
    let context = create_planning_context(
        &units,
        &[],
        model,
        system_prompt,
        max_input_tokens,
        BatchingStrategy::MaximizeUtilization,
        TermMatchIndex::default(),
    )
    .expect("planning context");
    let streamed = build_pending_segments(&units, &pending)
        .into_iter()
        .flat_map(|segment| {
            plan_batches_for_segment(&units, &segment, &context, None)
                .expect("segment plan")
                .0
                .into_iter()
                .map(|batch| batch.indices)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(streamed, whole);
}

fn reference_plan_indices(
    all_units: &[TranslationUnit],
    spans: &[TranslationSpan],
    pending_indices: &[usize],
    model: &str,
    system_prompt: &str,
    max_input_tokens: usize,
) -> Result<Vec<Vec<usize>>, prompting::PromptError> {
    let mut batches = Vec::new();
    for segment in build_segments(all_units, pending_indices) {
        let mut indices = Vec::new();
        let mut units = Vec::new();

        for index in segment.indices {
            let unit = &all_units[index];
            let would_exceed_items = units.len() >= MAX_ITEMS_PER_REQUEST;
            let mut candidate_units = units.clone();
            candidate_units.push(unit.clone());
            let candidate_tokens =
                prompting::count_request_tokens(model, system_prompt, &candidate_units, spans)?;
            let would_exceed_tokens = !units.is_empty() && candidate_tokens > max_input_tokens;

            if would_exceed_items || would_exceed_tokens {
                batches.push(indices);
                indices = Vec::new();
                units = Vec::new();
            }

            indices.push(index);
            units.push(unit.clone());
        }

        if !units.is_empty() {
            batches.push(indices);
        }
    }
    Ok(batches)
}

fn unit(id: &str, file: &str, source_text: String, event_id: Option<u32>) -> TranslationUnit {
    TranslationUnit {
        id: id.to_owned(),
        group_id: id.to_owned(),
        semantic_kind: "dialogue".to_owned(),
        context: ContextEnvelope {
            file: file.to_owned(),
            json_path: None,
            map_id: Some(1),
            event_id,
            page_id: Some(0),
            command_index: None,
            speaker_name: None,
            prev_texts: Vec::new(),
            next_texts: Vec::new(),
            block_text: Some(source_text.clone()),
            glossary_hits: Vec::new(),
            notes: vec!["event_dialogue_block".to_owned()],
        },
        source_text,
        translated_text: None,
        status: TranslationStatus::Pending,
        span_ids: Vec::new(),
    }
}
