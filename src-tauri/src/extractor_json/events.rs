use super::{
    TraversalState, clear_dialogue_context, event_context::fill_next_context,
    event_context::render_scene_context_line, infer_event_id, infer_page_id, is_translatable,
    previous_dialogue_lines, push_unit, remember_dialogue,
};
use crate::domain::{ContextEnvelope, TranslationSpan, TranslationUnit};
use serde_json::Value;

pub(super) fn extract_event_command_list(
    file: &str,
    path: &str,
    list: &[Value],
    traversal: &mut TraversalState,
    units: &mut Vec<TranslationUnit>,
    spans: &mut Vec<TranslationSpan>,
    map_id: Option<u32>,
) -> bool {
    if !list.iter().any(is_event_command) {
        return false;
    }

    clear_dialogue_context(traversal);
    let mut local_units = Vec::new();
    let mut index = 0usize;
    while index < list.len() {
        let Some(code) = command_code(&list[index]) else {
            index += 1;
            continue;
        };

        match code {
            101 => {
                if let Some(speaker) = command_speaker(&list[index]) {
                    traversal.last_speaker = Some(speaker);
                }
                index += 1;
            }
            401 => {
                let end = collect_text_commands(list, index, 401);
                push_event_text_block(
                    file,
                    path,
                    index,
                    &list[index..end],
                    "dialogue",
                    "event_dialogue_block",
                    traversal,
                    units,
                    spans,
                    map_id,
                    &mut local_units,
                );
                index = end;
            }
            105 => {
                let start = index + 1;
                let end = collect_text_commands(list, start, 405);
                if start < end {
                    push_event_text_block(
                        file,
                        path,
                        index,
                        &list[start..end],
                        "scroll_text",
                        "event_scroll_text_block",
                        traversal,
                        units,
                        spans,
                        map_id,
                        &mut local_units,
                    );
                    index = end;
                } else {
                    index += 1;
                }
            }
            405 => {
                let end = collect_text_commands(list, index, 405);
                push_event_text_block(
                    file,
                    path,
                    index,
                    &list[index..end],
                    "scroll_text",
                    "event_scroll_text_block",
                    traversal,
                    units,
                    spans,
                    map_id,
                    &mut local_units,
                );
                index = end;
            }
            102 => {
                push_choice_group(
                    file,
                    path,
                    index,
                    &list[index],
                    traversal,
                    units,
                    spans,
                    map_id,
                    &mut local_units,
                );
                index += 1;
            }
            _ => index += 1,
        }
    }

    fill_next_context(units, &local_units);
    clear_dialogue_context(traversal);
    true
}

#[allow(clippy::too_many_arguments)]
fn push_event_text_block(
    file: &str,
    list_path: &str,
    command_index: usize,
    commands: &[Value],
    semantic_kind: &str,
    note: &str,
    traversal: &mut TraversalState,
    units: &mut Vec<TranslationUnit>,
    spans: &mut Vec<TranslationSpan>,
    map_id: Option<u32>,
    local_units: &mut Vec<(usize, String)>,
) {
    let entries = commands
        .iter()
        .enumerate()
        .filter_map(|(offset, command)| {
            let text = command_parameter_text(command, 0)?;
            if !is_translatable(&text) {
                return None;
            }
            let locator = format!("{list_path}[{}].parameters[0]", command_index + offset);
            Some((locator, text))
        })
        .collect::<Vec<_>>();
    if entries.is_empty() {
        return;
    }

    let source_text = entries
        .iter()
        .map(|(_, text)| text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let group_locator = format!("{list_path}[{command_index}]#{semantic_kind}");
    let unit_index = units.len();
    let rendered_context = render_scene_context_line(
        semantic_kind,
        traversal.last_speaker.as_deref(),
        &source_text,
    );
    push_unit(
        file,
        &group_locator,
        semantic_kind,
        entries,
        event_context(
            file,
            list_path,
            command_index,
            traversal,
            map_id,
            note,
            &source_text,
        ),
        units,
        spans,
    );
    remember_dialogue(traversal, &rendered_context);
    local_units.push((unit_index, rendered_context));
}

#[allow(clippy::too_many_arguments)]
fn push_choice_group(
    file: &str,
    list_path: &str,
    command_index: usize,
    command: &Value,
    traversal: &mut TraversalState,
    units: &mut Vec<TranslationUnit>,
    spans: &mut Vec<TranslationSpan>,
    map_id: Option<u32>,
    local_units: &mut Vec<(usize, String)>,
) {
    let Some(choices) = command
        .get("parameters")
        .and_then(Value::as_array)
        .and_then(|parameters| parameters.first())
        .and_then(Value::as_array)
    else {
        return;
    };

    let entries = choices
        .iter()
        .enumerate()
        .filter_map(|(choice_index, choice)| {
            let text = choice.as_str()?.to_owned();
            if !is_translatable(&text) {
                return None;
            }
            Some((
                format!("{list_path}[{command_index}].parameters[0][{choice_index}]"),
                text,
            ))
        })
        .collect::<Vec<_>>();
    if entries.is_empty() {
        return;
    }

    let source_text = entries
        .iter()
        .map(|(_, text)| text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let group_locator = format!("{list_path}[{command_index}]#choice");
    let unit_index = units.len();
    let rendered_context = render_scene_context_line("choice", None, &source_text);
    push_unit(
        file,
        &group_locator,
        "choice",
        entries,
        event_context(
            file,
            list_path,
            command_index,
            traversal,
            map_id,
            "choice_group",
            &source_text,
        ),
        units,
        spans,
    );
    remember_dialogue(traversal, &rendered_context);
    local_units.push((unit_index, rendered_context));
}

fn event_context(
    file: &str,
    list_path: &str,
    command_index: usize,
    traversal: &TraversalState,
    map_id: Option<u32>,
    note: &str,
    source_text: &str,
) -> ContextEnvelope {
    ContextEnvelope {
        file: file.to_owned(),
        json_path: Some(format!("{list_path}[{command_index}]")),
        map_id,
        event_id: infer_event_id(list_path),
        page_id: infer_page_id(list_path),
        command_index: Some(command_index as u32),
        speaker_name: traversal.last_speaker.clone(),
        prev_texts: previous_dialogue_lines(traversal),
        next_texts: Vec::new(),
        block_text: Some(source_text.to_owned()),
        glossary_hits: Vec::new(),
        notes: vec![note.to_owned()],
    }
}

fn collect_text_commands(list: &[Value], start: usize, code: i64) -> usize {
    let mut end = start;
    while end < list.len() && command_code(&list[end]) == Some(code) {
        end += 1;
    }
    end
}

fn is_event_command(value: &Value) -> bool {
    command_code(value).is_some()
}

fn command_code(value: &Value) -> Option<i64> {
    value.get("code")?.as_i64()
}

fn command_parameter_text(command: &Value, index: usize) -> Option<String> {
    command
        .get("parameters")?
        .as_array()?
        .get(index)?
        .as_str()
        .map(str::to_owned)
}

fn command_speaker(command: &Value) -> Option<String> {
    command_parameter_text(command, 4)
        .or_else(|| command_parameter_text(command, 0))
        .filter(|text| is_translatable(text))
}
