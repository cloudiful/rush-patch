use super::PromptError;
use std::collections::BTreeMap;

pub(super) fn parse_translation_response(
    raw: &str,
) -> Result<BTreeMap<String, String>, PromptError> {
    let cleaned = strip_markdown_fence(raw);
    let value: serde_json::Value = serde_json::from_str(cleaned).map_err(PromptError::Parse)?;
    let entries = value
        .get("translations")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .or_else(|| value.as_array().cloned())
        .unwrap_or_default();

    let mut map = BTreeMap::new();
    for entry in entries {
        let Some(id) = entry.get("id").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let translated = entry
            .get("translatedText")
            .or_else(|| entry.get("translated_text"))
            .and_then(serde_json::Value::as_str);
        if let Some(translated) = translated {
            map.insert(id.to_owned(), translated.to_owned());
        }
    }

    Ok(map)
}

fn strip_markdown_fence(raw: &str) -> &str {
    let trimmed = raw.trim();
    if !trimmed.starts_with("```") {
        return trimmed;
    }

    let Some(first_newline) = trimmed.find('\n') else {
        return trimmed;
    };
    let body = &trimmed[first_newline + 1..];
    body.strip_suffix("```").unwrap_or(body).trim()
}
