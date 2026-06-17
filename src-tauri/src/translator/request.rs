use super::TranslateError;
use super::responses_client;
use crate::app_state::CancellationFlag;
use crate::domain::{ApiEndpoint, ProjectConfig, TranslationUnit, WorkflowEventPhase};
use crate::prompting;
use crate::terminology::CanonicalTerm;
use crate::workflow_events::WorkflowReporter;
use async_openai::Client;
use async_openai::config::OpenAIConfig;
use async_openai::types::chat::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestUserMessage, CreateChatCompletionRequestArgs,
};
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::time::timeout;

const DEFAULT_RETRY_ATTEMPTS: usize = 2;

pub(super) fn build_client(api_key: &str, base_url: Option<&str>) -> Client<OpenAIConfig> {
    let mut config = OpenAIConfig::new().with_api_key(api_key);
    if let Some(base_url) = base_url.map(str::trim).filter(|value| !value.is_empty()) {
        config = config.with_api_base(base_url);
    }
    Client::with_config(config)
}

pub(super) async fn request_batch_with_retries(
    client: &Client<OpenAIConfig>,
    config: &ProjectConfig,
    system_prompt: &str,
    units: &[TranslationUnit],
    spans: &[crate::domain::TranslationSpan],
    glossary_terms: &[CanonicalTerm],
    cancellation: &CancellationFlag,
    reporter: &WorkflowReporter,
    segment_index: usize,
    batch_index: usize,
) -> Result<(BTreeMap<String, String>, usize), TranslateError> {
    let prepared_prompt = prompting::build_user_prompt_with_terms(units, spans, glossary_terms)?;
    request_prepared_batch_with_retries(
        client,
        config,
        system_prompt,
        prepared_prompt,
        units.len(),
        cancellation,
        reporter,
        segment_index,
        batch_index,
    )
    .await
}

pub(super) async fn request_prepared_batch_with_retries(
    client: &Client<OpenAIConfig>,
    config: &ProjectConfig,
    system_prompt: &str,
    prepared_prompt: prompting::PreparedUserPrompt,
    unit_count: usize,
    cancellation: &CancellationFlag,
    reporter: &WorkflowReporter,
    segment_index: usize,
    batch_index: usize,
) -> Result<(BTreeMap<String, String>, usize), TranslateError> {
    let mut last_error = None;

    for attempt in 0..=DEFAULT_RETRY_ATTEMPTS {
        if cancellation.is_cancelled() {
            return Err(TranslateError::Cancelled);
        }

        reporter.debug(
            WorkflowEventPhase::Translate,
            "Submitting translation batch",
            Some(format!(
                "Segment {}, batch {}, attempt {}/{}",
                segment_index,
                batch_index,
                attempt + 1,
                DEFAULT_RETRY_ATTEMPTS + 1
            )),
            [
                ("segment_index", segment_index.to_string()),
                ("batch_index", batch_index.to_string()),
                ("attempt", (attempt + 1).to_string()),
                ("unit_count", unit_count.to_string()),
            ],
        );

        match request_prepared_batch(
            client,
            config,
            system_prompt,
            &prepared_prompt,
            reporter,
            segment_index,
            batch_index,
        )
        .await
        {
            Ok(translations) => return Ok((translations, attempt)),
            Err(error) => {
                reporter.warn(
                    WorkflowEventPhase::Translate,
                    "Translation batch attempt failed",
                    Some(format!(
                        "Segment {}, batch {}, attempt {}/{}: {}",
                        segment_index,
                        batch_index,
                        attempt + 1,
                        DEFAULT_RETRY_ATTEMPTS + 1,
                        truncated_error_message(&error.to_string())
                    )),
                );
                last_error = Some(error);
            }
        }
    }

    Err(last_error.expect("at least one attempt"))
}

pub(super) fn sanitized_base_url_host(base_url: Option<&str>) -> String {
    let Some(url) = base_url.map(str::trim).filter(|value| !value.is_empty()) else {
        return "default".to_owned();
    };
    let without_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    without_scheme
        .split('/')
        .next()
        .unwrap_or(without_scheme)
        .to_owned()
}

pub(super) fn truncated_error_message(message: &str) -> String {
    const LIMIT: usize = 500;
    if message.chars().count() <= LIMIT {
        return message.to_owned();
    }
    let truncated = message.chars().take(LIMIT).collect::<String>();
    format!("{truncated}...")
}

async fn request_prepared_batch(
    client: &Client<OpenAIConfig>,
    config: &ProjectConfig,
    system_prompt: &str,
    user_prompt: &prompting::PreparedUserPrompt,
    reporter: &WorkflowReporter,
    segment_index: usize,
    batch_index: usize,
) -> Result<BTreeMap<String, String>, TranslateError> {
    match config.api_endpoint {
        ApiEndpoint::Responses => {
            let timeout_secs = config.request_timeout_secs.max(1);
            timeout(
                Duration::from_secs(timeout_secs),
                responses_client::request_batch_responses(
                    config,
                    system_prompt,
                    user_prompt,
                    reporter,
                    segment_index,
                    batch_index,
                ),
            )
            .await
            .map_err(|_| TranslateError::Timeout(timeout_secs))?
        }
        ApiEndpoint::ChatCompletions => {
            request_batch_chat_completions(client, config, system_prompt, user_prompt).await
        }
    }
}

async fn request_batch_chat_completions(
    client: &Client<OpenAIConfig>,
    config: &ProjectConfig,
    system_prompt: &str,
    user_prompt: &prompting::PreparedUserPrompt,
) -> Result<BTreeMap<String, String>, TranslateError> {
    let messages: Vec<ChatCompletionRequestMessage> = vec![
        ChatCompletionRequestSystemMessage::from(system_prompt).into(),
        ChatCompletionRequestUserMessage::from(user_prompt.body()).into(),
    ];
    let request = CreateChatCompletionRequestArgs::default()
        .model(&config.model)
        .messages(messages)
        .build()
        .map_err(|source| TranslateError::BuildRequest(source.to_string()))?;

    let timeout_secs = config.request_timeout_secs.max(1);
    let response = timeout(
        Duration::from_secs(timeout_secs),
        client.chat().create(request),
    )
    .await
    .map_err(|_| TranslateError::Timeout(timeout_secs))?
    .map_err(|source| TranslateError::Provider(source.to_string()))?;

    let content = response
        .choices
        .first()
        .and_then(|choice| choice.message.content.as_deref())
        .ok_or(TranslateError::EmptyProviderResponse)?;
    user_prompt
        .resolve_translations(content)
        .map_err(TranslateError::from)
}
