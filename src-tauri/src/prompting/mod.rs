mod grouped_schema;
mod response;
mod schema;
mod system;
#[cfg(test)]
mod tests;
mod tokens;

use std::collections::BTreeMap;
use thiserror::Error;

pub(crate) use grouped_schema::{
    PromptSeedGroup, build_grouped_user_prompt_from_seed_groups_with_terms,
    render_grouped_prompt_body_from_seed_groups_with_terms,
};
pub(crate) use schema::{PromptRenderItemSeed, build_prompt_render_seeds};
pub use schema::{build_user_prompt, build_user_prompt_with_terms};
pub use system::build_system_prompt;
pub use tokens::PromptTokenCounter;
#[cfg(test)]
pub use tokens::count_request_tokens;

#[derive(Debug, Error)]
pub enum PromptError {
    #[error("failed to serialize translation prompt: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("translation response is not valid JSON: {0}")]
    Parse(serde_json::Error),
    #[error("failed to initialize tokenizer: {0}")]
    Tokenizer(String),
}

#[derive(Debug, Clone)]
pub struct PreparedUserPrompt {
    body: String,
    alias_to_real_id: BTreeMap<String, String>,
}

impl PreparedUserPrompt {
    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn resolve_translations(&self, raw: &str) -> Result<BTreeMap<String, String>, PromptError> {
        let parsed = response::parse_translation_response(raw)?;
        Ok(parsed
            .into_iter()
            .map(|(id, translated)| {
                let real_id = self.alias_to_real_id.get(&id).cloned().unwrap_or(id);
                (real_id, translated)
            })
            .collect())
    }
}
