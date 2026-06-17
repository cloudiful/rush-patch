use crate::domain::{
    ApiEndpoint, BatchingStrategy, GameProfile, LoadedAppSettings, SaveAppSettingsRequest,
    SaveAppSettingsSummary, StoredAppConfig,
};
use keyring_core::{Entry, Error as KeyringError, set_default_store};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tauri::{AppHandle, Manager};
use thiserror::Error;

const SETTINGS_FILE_NAME: &str = "settings.json";
const KEYRING_SERVICE: &str = "Rush Patch";
const KEYRING_ACCOUNT: &str = "openai-api-key";
static KEYRING_INIT: OnceLock<Result<(), String>> = OnceLock::new();

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("failed to resolve app config directory: {0}")]
    AppConfigDir(String),
    #[error("failed to create config directory {path}: {source}")]
    CreateDir {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read settings {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse settings {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to serialize settings: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("failed to write settings {path}: {source}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to access system credential store: {0}")]
    Keyring(String),
}

pub fn load_app_settings(app: &AppHandle) -> Result<LoadedAppSettings, SettingsError> {
    let config_path = settings_path(app)?;
    let config = load_config_file(&config_path)?;
    let keyring_result = load_api_key();

    let (api_key, keyring_available, keyring_error) = match keyring_result {
        Ok(api_key) => (api_key, true, None),
        Err(error) => (None, false, Some(error.to_string())),
    };

    Ok(LoadedAppSettings {
        config,
        api_key,
        keyring_available,
        keyring_error,
    })
}

pub fn save_app_settings(
    app: &AppHandle,
    request: SaveAppSettingsRequest,
) -> Result<SaveAppSettingsSummary, SettingsError> {
    let config_path = settings_path(app)?;
    save_config_file(&config_path, &request.config)?;
    save_api_key(request.api_key)?;

    Ok(SaveAppSettingsSummary {
        config_path: config_path.display().to_string(),
        keyring_available: true,
    })
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, SettingsError> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|error| SettingsError::AppConfigDir(error.to_string()))?;
    fs::create_dir_all(&dir).map_err(|source| SettingsError::CreateDir {
        path: dir.display().to_string(),
        source,
    })?;
    Ok(dir.join(SETTINGS_FILE_NAME))
}

fn load_config_file(path: &Path) -> Result<StoredAppConfig, SettingsError> {
    if !path.exists() {
        return Ok(default_config());
    }

    let raw = fs::read_to_string(path).map_err(|source| SettingsError::Read {
        path: path.display().to_string(),
        source,
    })?;
    serde_json::from_str(&raw).map_err(|source| SettingsError::Parse {
        path: path.display().to_string(),
        source,
    })
}

fn save_config_file(path: &Path, config: &StoredAppConfig) -> Result<(), SettingsError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| SettingsError::CreateDir {
            path: parent.display().to_string(),
            source,
        })?;
    }

    let payload = serde_json::to_string_pretty(config)?;
    fs::write(path, payload).map_err(|source| SettingsError::Write {
        path: path.display().to_string(),
        source,
    })
}

fn load_api_key() -> Result<Option<String>, SettingsError> {
    let entry = credential_entry()?;
    match entry.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(error) => Err(SettingsError::Keyring(error.to_string())),
    }
}

fn save_api_key(api_key: Option<String>) -> Result<(), SettingsError> {
    match api_key.map(|value| value.trim().to_owned()) {
        Some(value) if !value.is_empty() => credential_entry()?
            .set_password(&value)
            .map_err(|error| SettingsError::Keyring(error.to_string())),
        _ => {
            let Ok(entry) = credential_entry() else {
                return Ok(());
            };
            match entry.delete_credential() {
                Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
                Err(error) => Err(SettingsError::Keyring(error.to_string())),
            }
        }
    }
}

fn credential_entry() -> Result<Entry, SettingsError> {
    ensure_keyring_store()?;
    Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|error| SettingsError::Keyring(error.to_string()))
}

fn ensure_keyring_store() -> Result<(), SettingsError> {
    KEYRING_INIT
        .get_or_init(initialize_native_keyring_store)
        .as_ref()
        .map_err(|error| SettingsError::Keyring(error.clone()))
        .map(|_| ())
}

fn initialize_native_keyring_store() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let store = windows_native_keyring_store::Store::new().map_err(|error| error.to_string())?;
        set_default_store(store);
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        Err("native credential store initialization is not configured for this platform".to_owned())
    }
}

fn default_config() -> StoredAppConfig {
    StoredAppConfig {
        game_root: String::new(),
        model: "gpt-4.1-mini".to_owned(),
        api_endpoint: ApiEndpoint::Responses,
        base_url: None,
        system_prompt:
            "Translate Japanese RPG text into natural Chinese while preserving placeholders and control codes."
                .to_owned(),
        glossary_path: None,
        do_not_translate_path: None,
        game_profile: GameProfile::GeneralRpg,
        target_input_tokens: crate::domain::default_target_input_tokens(),
        batching_strategy: BatchingStrategy::MaximizeUtilization,
        debug_logging: false,
        max_concurrency: 1,
        request_timeout_secs: 90,
        source_lang: "Japanese".to_owned(),
        target_lang: "Chinese".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_settings_path(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        std::env::temp_dir().join(format!("rush_patch_settings_{name}_{stamp}.json"))
    }

    #[test]
    fn missing_config_file_loads_defaults() {
        let path = temp_settings_path("missing");
        let config = load_config_file(&path).expect("load defaults");

        assert_eq!(config.model, "gpt-4.1-mini");
        assert_eq!(config.api_endpoint, ApiEndpoint::Responses);
        assert_eq!(config.game_profile, GameProfile::GeneralRpg);
        assert_eq!(config.target_input_tokens, 6_000);
        assert_eq!(config.batching_strategy, BatchingStrategy::MaximizeUtilization);
        assert!(!config.debug_logging);
    }

    #[test]
    fn config_file_round_trips_without_api_key() {
        let path = temp_settings_path("round_trip");
        let mut config = default_config();
        config.game_root = "C:\\Games\\ExampleRpg".to_owned();
        config.base_url = Some("https://api.example.test/v1".to_owned());

        save_config_file(&path, &config).expect("save config");
        let loaded = load_config_file(&path).expect("load config");

        assert_eq!(loaded.game_root, config.game_root);
        assert_eq!(loaded.api_endpoint, ApiEndpoint::Responses);
        assert_eq!(loaded.base_url, config.base_url);
        let raw = fs::read_to_string(&path).expect("read config");
        assert!(!raw.contains("apiKey"));

        fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn legacy_config_without_endpoint_defaults_to_responses() {
        let path = temp_settings_path("legacy_endpoint");
        fs::write(
            &path,
            r#"{
              "gameRoot": "",
              "model": "gpt-4.1-mini",
              "baseUrl": null,
              "systemPrompt": "prompt",
              "glossaryPath": null,
              "doNotTranslatePath": null,
              "maxConcurrency": 1,
              "requestTimeoutSecs": 90,
              "sourceLang": "Japanese",
              "targetLang": "Chinese"
            }"#,
        )
        .expect("write legacy config");

        let loaded = load_config_file(&path).expect("load legacy config");

        assert_eq!(loaded.api_endpoint, ApiEndpoint::Responses);
        assert_eq!(loaded.game_profile, GameProfile::GeneralRpg);
        assert_eq!(loaded.batching_strategy, BatchingStrategy::MaximizeUtilization);
        assert!(!loaded.debug_logging);

        fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn legacy_max_input_tokens_maps_to_target_input_tokens() {
        let path = temp_settings_path("legacy_target_tokens");
        fs::write(
            &path,
            r#"{
              "gameRoot": "",
              "model": "gpt-4.1-mini",
              "apiEndpoint": "responses",
              "baseUrl": null,
              "systemPrompt": "prompt",
              "glossaryPath": null,
              "doNotTranslatePath": null,
              "gameProfile": "general_rpg",
              "maxInputTokens": 4321,
              "debugLogging": false,
              "maxConcurrency": 1,
              "requestTimeoutSecs": 90,
              "sourceLang": "Japanese",
              "targetLang": "Chinese"
            }"#,
        )
        .expect("write legacy config");

        let loaded = load_config_file(&path).expect("load legacy config");

        assert_eq!(loaded.target_input_tokens, 4321);

        fs::remove_file(path).expect("cleanup");
    }
}
