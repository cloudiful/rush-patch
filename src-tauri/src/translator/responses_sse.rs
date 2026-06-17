use crate::openai_responses::TranslationResponse;
use serde_json::Value;

#[derive(Debug, Clone)]
pub(crate) struct ParsedSseResponse {
    pub(crate) output_text: Option<String>,
    pub(crate) done_text: Option<String>,
    pub(crate) first_event: Option<String>,
    pub(crate) event_names: Vec<String>,
    pub(crate) event_count: usize,
    pub(crate) final_response: Option<TranslationResponse>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ResponsesSseError {
    #[error("invalid SSE payload: {0}")]
    InvalidPayload(String),
    #[error("response failed: {0}")]
    Failed(String),
    #[error("response incomplete: {0}")]
    Incomplete(String),
}

pub(crate) fn parse_sse_payload(payload: &str) -> Result<ParsedSseResponse, ResponsesSseError> {
    let mut first_event = None;
    let mut event_names = Vec::new();
    let mut event_count = 0usize;
    let mut output_text = String::new();
    let mut done_text = None;
    let mut final_response = None;

    let normalized = payload.replace("\r\n", "\n");
    for raw_event in normalized.split("\n\n") {
        let trimmed = raw_event.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut event_name = None;
        let mut data_lines = Vec::new();
        for line in raw_event.lines() {
            if let Some(value) = line.strip_prefix("event:") {
                event_name = Some(value.trim().to_owned());
            } else if let Some(value) = line.strip_prefix("data:") {
                data_lines.push(value.trim_start().to_owned());
            }
        }

        let data = data_lines.join("\n");
        if data.is_empty() || data == "[DONE]" {
            continue;
        }

        let value: Value = serde_json::from_str(&data)
            .map_err(|error| ResponsesSseError::InvalidPayload(format!("{error}: {data}")))?;
        let event_name = event_name
            .or_else(|| value.get("type").and_then(Value::as_str).map(str::to_owned))
            .unwrap_or_else(|| "message".to_owned());
        if first_event.is_none() {
            first_event = Some(event_name.clone());
        }
        if event_names.len() < 12 && !event_names.iter().any(|name| name == &event_name) {
            event_names.push(event_name.clone());
        }
        event_count += 1;
        match event_name.as_str() {
            "response.output_text.delta" => {
                if let Some(delta) = extract_delta_text(&value) {
                    output_text.push_str(delta);
                }
            }
            "response.output_text.done" => {
                if let Some(text) = value.get("text").and_then(Value::as_str) {
                    done_text = Some(text.to_owned());
                }
            }
            "response.completed" => {
                final_response = Some(parse_translation_response_value(&value)?);
            }
            "response.failed" => {
                return Err(ResponsesSseError::Failed(response_summary(&value)));
            }
            "response.incomplete" => {
                return Err(ResponsesSseError::Incomplete(response_summary(&value)));
            }
            _ => {}
        }
    }

    Ok(ParsedSseResponse {
        output_text: (!output_text.is_empty()).then_some(output_text),
        done_text,
        first_event,
        event_names,
        event_count,
        final_response,
    })
}

fn extract_delta_text(value: &Value) -> Option<&str> {
    value
        .get("delta")
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .get("delta")
                .and_then(|delta| delta.get("text"))
                .and_then(Value::as_str)
        })
}

fn parse_translation_response_value(value: &Value) -> Result<TranslationResponse, ResponsesSseError> {
    let response_value = value.get("response").cloned().unwrap_or_else(|| value.clone());
    serde_json::from_value(response_value)
        .map_err(|error| ResponsesSseError::InvalidPayload(error.to_string()))
}

fn response_summary(value: &Value) -> String {
    value
        .get("response")
        .and_then(Value::as_object)
        .and_then(|response| response.get("error").or_else(|| response.get("incomplete_details")))
        .cloned()
        .or_else(|| value.get("error").cloned())
        .map(|error| error.to_string())
        .unwrap_or_else(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_delta_and_completed_events() {
        let payload = concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"status\":\"in_progress\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"{\\\"translations\\\":[]}\"}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"{\\\"translations\\\":[]}\"}]}]}}\n\n"
        );

        let parsed = parse_sse_payload(payload).expect("parse sse payload");

        assert_eq!(parsed.first_event.as_deref(), Some("response.created"));
        assert_eq!(
            parsed.event_names,
            vec![
                "response.created".to_owned(),
                "response.output_text.delta".to_owned(),
                "response.completed".to_owned()
            ]
        );
        assert_eq!(parsed.event_count, 3);
        assert_eq!(parsed.output_text.as_deref(), Some("{\"translations\":[]}"));
        assert_eq!(parsed.done_text, None);
        assert_eq!(
            parsed
                .final_response
                .as_ref()
                .and_then(TranslationResponse::output_text)
                .as_deref(),
            Some("{\"translations\":[]}")
        );
    }

    #[test]
    fn parses_event_stream_body_without_sse_content_type_hint() {
        let payload = concat!(
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n"
        );

        let parsed = parse_sse_payload(payload).expect("parse sse payload");
        assert_eq!(parsed.output_text.as_deref(), Some("ok"));
        assert_eq!(parsed.event_names, vec!["response.output_text.delta".to_owned()]);
    }

    #[test]
    fn parses_done_event_without_delta() {
        let payload = concat!(
            "event: response.output_text.done\n",
            "data: {\"type\":\"response.output_text.done\",\"text\":\"{\\\"translations\\\":[]}\"}\n\n"
        );

        let parsed = parse_sse_payload(payload).expect("parse sse payload");

        assert_eq!(parsed.output_text, None);
        assert_eq!(parsed.done_text.as_deref(), Some("{\"translations\":[]}"));
    }

    #[test]
    fn parses_data_only_sse_events_using_json_type() {
        let payload = concat!(
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"{\\\"translations\\\":[]\"}\n\n",
            "data: {\"type\":\"response.output_text.done\",\"text\":\"{\\\"translations\\\":[]}\"}\n\n"
        );

        let parsed = parse_sse_payload(payload).expect("parse sse payload");

        assert_eq!(
            parsed.event_names,
            vec![
                "response.output_text.delta".to_owned(),
                "response.output_text.done".to_owned()
            ]
        );
        assert_eq!(parsed.first_event.as_deref(), Some("response.output_text.delta"));
        assert_eq!(parsed.output_text.as_deref(), Some("{\"translations\":[]"));
        assert_eq!(parsed.done_text.as_deref(), Some("{\"translations\":[]}"));
    }

    #[test]
    fn parses_nested_delta_text_from_compatible_provider() {
        let payload = concat!(
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":{\"text\":\"ok\"}}\n\n"
        );

        let parsed = parse_sse_payload(payload).expect("parse sse payload");
        assert_eq!(parsed.output_text.as_deref(), Some("ok"));
    }

    #[test]
    fn surfaces_failed_event_summary() {
        let payload = concat!(
            "event: response.failed\n",
            "data: {\"type\":\"response.failed\",\"error\":{\"message\":\"boom\"}}\n\n"
        );

        let error = parse_sse_payload(payload).expect_err("failed event");
        assert!(error.to_string().contains("boom"));
    }
}
