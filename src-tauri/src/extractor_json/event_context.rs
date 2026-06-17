use crate::domain::TranslationUnit;

pub(super) fn fill_next_context(units: &mut [TranslationUnit], local_units: &[(usize, String)]) {
    for (position, (unit_index, _)) in local_units.iter().enumerate() {
        units[*unit_index].context.next_texts = local_units
            .iter()
            .skip(position + 1)
            .take(3)
            .map(|(_, text)| text.clone())
            .collect();
    }
}

pub(super) fn render_scene_context_line(
    semantic_kind: &str,
    speaker_name: Option<&str>,
    source_text: &str,
) -> String {
    let compact_text = source_text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" / ");

    match semantic_kind {
        "dialogue" => speaker_name
            .map(|speaker| format!("{speaker}: {compact_text}"))
            .unwrap_or_else(|| format!("Dialogue: {compact_text}")),
        "choice" => format!("Choice: {compact_text}"),
        "scroll_text" => format!("Scroll: {compact_text}"),
        _ => compact_text,
    }
}
