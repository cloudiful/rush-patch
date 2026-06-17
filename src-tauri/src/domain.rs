#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ApiEndpoint {
    #[default]
    Responses,
    ChatCompletions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GameProfile {
    #[default]
    GeneralRpg,
    AdultRpg,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BatchingStrategy {
    #[default]
    MaximizeUtilization,
    QualityFirst,
}

impl BatchingStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MaximizeUtilization => "maximize_utilization",
            Self::QualityFirst => "quality_first",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectConfig {
    pub game_root: String,
    pub model: String,
    #[serde(default)]
    pub api_endpoint: ApiEndpoint,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub system_prompt: String,
    pub glossary_path: Option<String>,
    pub do_not_translate_path: Option<String>,
    #[serde(default)]
    pub game_profile: GameProfile,
    #[serde(
        default = "default_target_input_tokens",
        alias = "maxInputTokens"
    )]
    pub target_input_tokens: usize,
    #[serde(default)]
    pub batching_strategy: BatchingStrategy,
    #[serde(default)]
    pub debug_logging: bool,
    pub max_concurrency: usize,
    pub request_timeout_secs: u64,
    pub source_lang: String,
    pub target_lang: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateCatalogRequest {
    pub catalog_path: String,
    pub config: ProjectConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredAppConfig {
    pub game_root: String,
    pub model: String,
    #[serde(default)]
    pub api_endpoint: ApiEndpoint,
    pub base_url: Option<String>,
    pub system_prompt: String,
    pub glossary_path: Option<String>,
    pub do_not_translate_path: Option<String>,
    #[serde(default)]
    pub game_profile: GameProfile,
    #[serde(
        default = "default_target_input_tokens",
        alias = "maxInputTokens"
    )]
    pub target_input_tokens: usize,
    #[serde(default)]
    pub batching_strategy: BatchingStrategy,
    #[serde(default)]
    pub debug_logging: bool,
    pub max_concurrency: usize,
    pub request_timeout_secs: u64,
    pub source_lang: String,
    pub target_lang: String,
}

pub fn default_target_input_tokens() -> usize {
    6_000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAppSettingsRequest {
    pub config: StoredAppConfig,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedAppSettings {
    pub config: StoredAppConfig,
    pub api_key: Option<String>,
    pub keyring_available: bool,
    pub keyring_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAppSettingsSummary {
    pub config_path: String,
    pub keyring_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationRunSummary {
    pub catalog_path: String,
    pub total_units: usize,
    pub attempted_units: usize,
    pub translated_units: usize,
    pub pretranslated_terms: usize,
    pub failed_terms: usize,
    pub failed_units: usize,
    pub validation_failed_units: usize,
    pub validation_warning_units: usize,
    pub batches: usize,
    pub retries: usize,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogTokenEstimate {
    pub catalog_path: String,
    pub total_units: usize,
    pub pending_units: usize,
    pub reused_units: usize,
    pub target_input_tokens: usize,
    pub batching_strategy: BatchingStrategy,
    pub estimated_term_batches: usize,
    pub estimated_term_input_tokens: usize,
    pub estimated_term_output_tokens: usize,
    pub estimated_main_batches: usize,
    pub estimated_scene_batches: usize,
    pub estimated_orphan_pool_batches: usize,
    pub estimated_main_input_tokens: usize,
    pub estimated_main_output_tokens: usize,
    pub estimated_batches: usize,
    pub estimated_input_tokens: usize,
    pub estimated_output_tokens: usize,
    pub estimated_total_tokens: usize,
    pub estimated_average_input_tokens: usize,
    pub estimated_average_input_utilization_pct: usize,
    pub average_main_input_utilization_pct: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationCatalog {
    pub project: CatalogProject,
    pub spans: Vec<TranslationSpan>,
    pub units: Vec<TranslationUnit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogProject {
    pub game_root: String,
    pub engine: String,
    pub generated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationSpan {
    pub id: String,
    pub file: String,
    pub source_kind: SourceKind,
    pub locator: String,
    pub source_text: String,
    pub protected_tokens: Vec<String>,
    pub flags: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Json,
    Js,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextEnvelope {
    pub file: String,
    pub json_path: Option<String>,
    pub map_id: Option<u32>,
    pub event_id: Option<u32>,
    pub page_id: Option<u32>,
    pub command_index: Option<u32>,
    pub speaker_name: Option<String>,
    pub prev_texts: Vec<String>,
    pub next_texts: Vec<String>,
    pub block_text: Option<String>,
    pub glossary_hits: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationUnit {
    pub id: String,
    pub group_id: String,
    pub semantic_kind: String,
    pub context: ContextEnvelope,
    pub source_text: String,
    pub translated_text: Option<String>,
    pub status: TranslationStatus,
    pub span_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranslationStatus {
    Pending,
    Translated,
    Validated,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReport {
    pub unit_id: String,
    pub status: ValidationStatus,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub token_diff: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationStatus {
    Passed,
    Warning,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonExtractionPreview {
    pub total_units: usize,
    pub total_spans: usize,
    pub sample_units: Vec<TranslationUnit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchPlan {
    pub game_root: String,
    pub backup_root: String,
    pub backed_up_files: usize,
    pub updated_files: usize,
    pub preserved_failed_units: usize,
    pub validation_failed_units: usize,
    pub validation_warning_units: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePlan {
    pub game_root: String,
    pub backup_root: String,
    pub restored_files: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowEventLevel {
    Info,
    Debug,
    Warn,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowEventPhase {
    Idle,
    Scan,
    Catalog,
    Estimate,
    Translate,
    Patch,
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowEvent {
    pub phase: WorkflowEventPhase,
    pub level: WorkflowEventLevel,
    pub message_key: Option<String>,
    pub message: String,
    pub current: Option<usize>,
    pub total: Option<usize>,
    pub detail: Option<String>,
    pub timestamp_ms: u64,
    pub debug: Option<std::collections::BTreeMap<String, String>>,
}
