use super::request::truncated_error_message;
use super::responses_sse::{ResponsesSseError, parse_sse_payload};
use super::TranslateError;
use crate::domain::{ProjectConfig, WorkflowEventPhase};
use crate::openai_responses;
use crate::prompting;
use crate::workflow_events::WorkflowReporter;
use async_openai::types::responses::CreateResponseArgs;
use futures::StreamExt;
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Instant;

const DEFAULT_RESPONSES_BASE_URL: &str = "https://api.openai.com/v1";

pub(super) async fn request_batch_responses(
    config: &ProjectConfig,
    system_prompt: &str,
    user_prompt: &prompting::PreparedUserPrompt,
    reporter: &WorkflowReporter,
    segment_index: usize,
    batch_index: usize,
) -> Result<BTreeMap<String, String>, TranslateError> {
    let api_key = config
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(TranslateError::MissingApiKey)?;
    let body = build_streaming_request_body(config, system_prompt, user_prompt)?;
    let request_url = responses_request_url(config.base_url.as_deref());
    let client = reqwest::Client::builder()
        .build()
        .map_err(|error| TranslateError::BuildRequest(error.to_string()))?;
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {api_key}"))
            .map_err(|error| TranslateError::BuildRequest(error.to_string()))?,
    );
    headers.insert(ACCEPT, HeaderValue::from_static("application/json, text/event-stream"));

    let started_at = Instant::now();
    let response = client
        .post(request_url)
        .headers(headers)
        .json(&body)
        .send()
        .await
        .map_err(|error| TranslateError::Provider(error.to_string()))?;
    let status = response.status();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let bytes = read_response_stream(response).await?;
    let body_text = String::from_utf8(bytes)
        .map_err(|error| TranslateError::Provider(format!("response body was not valid UTF-8: {error}")))?;

    if !status.is_success() {
        return Err(TranslateError::Provider(format!(
            "status {} content-type {} body {}",
            status,
            content_type,
            truncated_error_message(&body_text)
        )));
    }

    if looks_like_sse(&content_type, &body_text) {
        let parsed = parse_sse_payload(&body_text).map_err(map_sse_error)?;
        reporter.debug(
            WorkflowEventPhase::Translate,
            "Responses SSE batch completed",
            Some(format!(
                "Segment {}, batch {} received {} SSE event(s) in {} ms",
                segment_index,
                batch_index,
                parsed.event_count,
                started_at.elapsed().as_millis()
            )),
            [
                ("transport", "responses_sse".to_owned()),
                (
                    "first_event",
                    parsed.first_event.clone().unwrap_or_else(|| "-".to_owned()),
                ),
                ("event_names", summarize_event_names(&parsed.event_names)),
                ("event_count", parsed.event_count.to_string()),
                (
                    "response_text_chars",
                    parsed
                        .output_text
                        .as_ref()
                        .map(|text| text.chars().count())
                        .or_else(|| {
                            parsed
                                .final_response
                                .as_ref()
                                .and_then(openai_responses::TranslationResponse::output_text)
                                .map(|text| text.chars().count())
                        })
                        .unwrap_or(0)
                        .to_string(),
                ),
                ("elapsed_ms", started_at.elapsed().as_millis().to_string()),
            ],
        );

        let content = parsed
            .output_text
            .or(parsed.done_text)
            .or_else(|| parsed.final_response.as_ref().and_then(|response| response.output_text()));
        let Some(content) = content else {
            reporter.debug(
                WorkflowEventPhase::Translate,
                "Responses SSE batch produced no text",
                Some(truncate_debug_body(&body_text)),
                [
                    ("transport", "responses_sse".to_owned()),
                    (
                        "content_type",
                        if content_type.is_empty() {
                            "-".to_owned()
                        } else {
                            content_type.clone()
                        },
                    ),
                    (
                        "first_event",
                        parsed.first_event.unwrap_or_else(|| "-".to_owned()),
                    ),
                    ("event_names", summarize_event_names(&parsed.event_names)),
                    ("event_count", parsed.event_count.to_string()),
                ],
            );
            return Err(TranslateError::EmptyProviderResponse);
        };
        return user_prompt
            .resolve_translations(&content)
            .map_err(TranslateError::from);
    }

    let response = openai_responses::parse_translation_response(&body_text)
        .map_err(|error| TranslateError::Provider(format!("failed to parse JSON response: {error}")))?;
    if let Some(summary) = response.failure_summary() {
        return Err(TranslateError::Provider(summary));
    }
    let Some(content) = response.output_text() else {
        reporter.debug(
            WorkflowEventPhase::Translate,
            "Responses JSON batch produced no text",
            Some(truncate_debug_body(&body_text)),
            [
                ("transport", "responses_json".to_owned()),
                (
                    "content_type",
                    if content_type.is_empty() {
                        "-".to_owned()
                    } else {
                        content_type.clone()
                    },
                ),
                (
                    "status",
                    response
                        .status_label()
                        .unwrap_or_else(|| "-".to_owned()),
                ),
            ],
        );
        return Err(TranslateError::EmptyProviderResponse);
    };
    user_prompt
        .resolve_translations(&content)
        .map_err(TranslateError::from)
}

fn build_streaming_request_body(
    config: &ProjectConfig,
    system_prompt: &str,
    user_prompt: &prompting::PreparedUserPrompt,
) -> Result<Value, TranslateError> {
    let request = CreateResponseArgs::default()
        .model(&config.model)
        .instructions(system_prompt)
        .input(user_prompt.body())
        .text(openai_responses::translation_response_text_param())
        .build()
        .map_err(|source| TranslateError::BuildRequest(source.to_string()))?;
    let mut body =
        serde_json::to_value(request).map_err(|source| TranslateError::BuildRequest(source.to_string()))?;
    body.as_object_mut()
        .expect("response request should serialize as object")
        .insert("stream".to_owned(), Value::Bool(true));
    Ok(body)
}

fn responses_request_url(base_url: Option<&str>) -> String {
    let base = base_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_RESPONSES_BASE_URL)
        .trim_end_matches('/');
    if base.ends_with("/responses") {
        base.to_owned()
    } else {
        format!("{base}/responses")
    }
}

async fn read_response_stream(response: reqwest::Response) -> Result<Vec<u8>, TranslateError> {
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| TranslateError::Provider(error.to_string()))?;
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn looks_like_sse(content_type: &str, body_text: &str) -> bool {
    content_type.contains("text/event-stream") || body_text.trim_start().starts_with("event:")
}

fn map_sse_error(error: ResponsesSseError) -> TranslateError {
    TranslateError::Provider(error.to_string())
}

fn summarize_event_names(event_names: &[String]) -> String {
    if event_names.is_empty() {
        "-".to_owned()
    } else {
        event_names.join(",")
    }
}

fn truncate_debug_body(body_text: &str) -> String {
    const LIMIT: usize = 1200;
    if body_text.chars().count() <= LIMIT {
        return body_text.to_owned();
    }
    let truncated = body_text.chars().take(LIMIT).collect::<String>();
    format!("{truncated}...")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_responses_path_for_base_url() {
        assert_eq!(
            responses_request_url(Some("https://example.test/v1")),
            "https://example.test/v1/responses"
        );
        assert_eq!(
            responses_request_url(Some("https://example.test/v1/responses")),
            "https://example.test/v1/responses"
        );
    }

    #[test]
    fn detects_sse_from_body_prefix() {
        assert!(looks_like_sse("", "event: response.created\ndata: {}\n\n"));
    }
}
