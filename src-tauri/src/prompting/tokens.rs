use super::{PromptError, build_user_prompt, build_user_prompt_with_terms};
use crate::domain::{TranslationSpan, TranslationUnit};
use crate::terminology::CanonicalTerm;
use serde::Serialize;
use tiktoken_rs::{CoreBPE, bpe_for_model, bpe_for_tokenizer, tokenizer::Tokenizer};

pub struct PromptTokenCounter {
    bpe: CoreBPE,
    system_prompt_tokens: usize,
}

impl PromptTokenCounter {
    pub fn new(model: &str, system_prompt: &str) -> Result<Self, PromptError> {
        let bpe = resolve_bpe(model)?;
        let system_prompt_tokens = count_tokens_with_bpe(&bpe, system_prompt);
        Ok(Self {
            bpe,
            system_prompt_tokens,
        })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn count_request_tokens(
        &self,
        units: &[TranslationUnit],
        spans: &[TranslationSpan],
    ) -> Result<usize, PromptError> {
        let user_prompt = build_user_prompt(units, spans)?;
        Ok(self.count_rendered_prompt_body(user_prompt.body()))
    }

    pub fn count_request_tokens_with_terms(
        &self,
        units: &[TranslationUnit],
        spans: &[TranslationSpan],
        glossary_terms: &[CanonicalTerm],
    ) -> Result<usize, PromptError> {
        let user_prompt = build_user_prompt_with_terms(units, spans, glossary_terms)?;
        Ok(self.count_rendered_prompt_body(user_prompt.body()))
    }

    pub fn count_response_tokens(&self, units: &[TranslationUnit]) -> Result<usize, PromptError> {
        let payload = ResponseEnvelope {
            translations: units
                .iter()
                .enumerate()
                .map(|(index, unit)| ResponseItem {
                    id: format!("u{}", index + 1),
                    translated_text: unit.source_text.clone(),
                })
                .collect(),
        };
        let body = serde_json::to_string(&payload)?;
        Ok(count_tokens_with_bpe(&self.bpe, &body))
    }

    pub fn count_rendered_prompt_body(&self, body: &str) -> usize {
        self.system_prompt_tokens + count_tokens_with_bpe(&self.bpe, body)
    }
}

#[cfg(test)]
pub fn count_request_tokens(
    model: &str,
    system_prompt: &str,
    units: &[TranslationUnit],
    spans: &[TranslationSpan],
) -> Result<usize, PromptError> {
    PromptTokenCounter::new(model, system_prompt)?.count_request_tokens(units, spans)
}

#[derive(Serialize)]
struct ResponseEnvelope {
    translations: Vec<ResponseItem>,
}

#[derive(Serialize)]
struct ResponseItem {
    id: String,
    #[serde(rename = "translatedText")]
    translated_text: String,
}

fn resolve_bpe(model: &str) -> Result<CoreBPE, PromptError> {
    bpe_for_model(model)
        .or_else(|_| {
            bpe_for_tokenizer(Tokenizer::Cl100kBase)
                .map_err(|error| PromptError::Tokenizer(error.to_string()))
        })
        .cloned()
}

fn count_tokens_with_bpe(bpe: &CoreBPE, text: &str) -> usize {
    bpe.encode_with_special_tokens(text).len()
}
