use async_openai::types::responses::{ResponseFormatJsonSchema, ResponseTextParam};
use serde::Deserialize;
use serde_json::{Value, json};

pub fn translation_response_text_param() -> ResponseTextParam {
    ResponseFormatJsonSchema {
        name: "rpg_translation_batch".to_owned(),
        description: Some("Batch RPG translation output keyed by original item ids.".to_owned()),
        schema: json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["translations"],
            "properties": {
                "translations": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["id", "translatedText"],
                        "properties": {
                            "id": { "type": "string" },
                            "translatedText": { "type": "string" }
                        }
                    }
                }
            }
        }),
        strict: Some(true),
    }
    .into()
}

pub fn parse_translation_response(content: &str) -> Result<TranslationResponse, serde_json::Error> {
    serde_json::from_str(content)
}

#[derive(Debug, Clone, Deserialize)]
pub struct TranslationResponse {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    error: Option<Value>,
    #[serde(default)]
    output: Vec<ResponseOutput>,
}

impl TranslationResponse {
    pub fn status_label(&self) -> Option<String> {
        self.status.clone()
    }

    pub fn failure_summary(&self) -> Option<String> {
        if let Some(error) = self.error.as_ref().filter(|value| !value.is_null()) {
            return Some(format!("response error: {error}"));
        }

        match self.status.as_deref() {
            Some("cancelled" | "failed" | "incomplete") => Some(format!(
                "response status: {}",
                self.status.as_deref().unwrap()
            )),
            _ => None,
        }
    }

    pub fn output_text(&self) -> Option<String> {
        let text = self
            .output
            .iter()
            .flat_map(|item| item.content.iter())
            .filter_map(|content| content.text.as_deref())
            .collect::<Vec<_>>()
            .join("");

        if text.is_empty() { None } else { Some(text) }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ResponseOutput {
    #[serde(default)]
    content: Vec<ResponseContent>,
}

#[derive(Debug, Clone, Deserialize)]
struct ResponseContent {
    #[serde(default)]
    text: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_output_text_without_annotations() {
        let response: TranslationResponse = serde_json::from_str(
            r#"{
              "status": "completed",
              "output": [{
                "type": "message",
                "content": [{
                  "type": "output_text",
                  "text": "{\"translations\":[]}"
                }]
              }]
            }"#,
        )
        .expect("deserialize response");

        assert_eq!(
            response.output_text().as_deref(),
            Some(r#"{"translations":[]}"#)
        );
        assert_eq!(response.failure_summary(), None);
    }

    #[test]
    fn schema_requires_translation_shape() {
        let value = serde_json::to_value(translation_response_text_param()).expect("serialize");

        assert_eq!(value["format"]["type"], "json_schema");
        assert_eq!(value["format"]["strict"], true);
        assert_eq!(value["format"]["schema"]["required"][0], "translations");
    }
}
