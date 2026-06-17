import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppSettings,
  CatalogTokenEstimate,
  JsonExtractionPreview,
  PatchPlan,
  ProjectConfig,
  RestorePlan,
  SaveAppSettingsRequest,
  SaveAppSettingsSummary,
  ScanSummary,
  TranslationRunSummary,
  WorkflowEvent,
} from "../types/workflow";

const defaultPrompt =
  "Translate Japanese RPG text into natural Chinese while preserving placeholders and control codes.";

function hasTauriRuntime() {
  return "__TAURI_INTERNALS__" in (globalThis as { __TAURI_INTERNALS__?: unknown });
}

function defaultAppSettings(): AppSettings {
  return {
    config: {
      gameRoot: "",
      model: "gpt-4.1-mini",
      baseUrl: null,
      apiEndpoint: "responses",
      systemPrompt: defaultPrompt,
      glossaryPath: null,
      doNotTranslatePath: null,
      gameProfile: "general_rpg",
      targetInputTokens: 6000,
      batchingStrategy: "maximize_utilization",
      debugLogging: false,
      maxConcurrency: 1,
      requestTimeoutSecs: 90,
      sourceLang: "Japanese",
      targetLang: "Chinese",
    },
    apiKey: null,
    keyringAvailable: false,
    keyringError: null,
  };
}

export function scanProject(gameRoot: string) {
  return invoke<ScanSummary>("scan_project", { gameRoot });
}

export function previewExtraction(gameRoot: string) {
  return invoke<JsonExtractionPreview>("extract_json_preview", { gameRoot });
}

export function buildCatalog(gameRoot: string, debugLogging: boolean) {
  return invoke<string>("build_catalog", { gameRoot, debugLogging });
}

export function estimateCatalog(catalogPath: string, config: ProjectConfig) {
  return invoke<CatalogTokenEstimate>("estimate_catalog", {
    request: { catalogPath, config },
  });
}

export function loadAppSettings() {
  if (!hasTauriRuntime()) return Promise.resolve(defaultAppSettings());
  return invoke<AppSettings>("load_app_settings");
}

export function saveAppSettings(request: SaveAppSettingsRequest) {
  if (!hasTauriRuntime()) {
    return Promise.resolve({
      configPath: "",
      keyringAvailable: false,
    });
  }
  return invoke<SaveAppSettingsSummary>("save_app_settings", { request });
}

export function translateCatalog(catalogPath: string, config: ProjectConfig) {
  return invoke<TranslationRunSummary>("translate_catalog", {
    request: { catalogPath, config },
  });
}

export function cancelTranslation() {
  return invoke<boolean>("cancel_translation");
}

export function applyTranslatedPatch(
  gameRoot: string,
  catalogPath: string,
  debugLogging: boolean,
) {
  return invoke<PatchPlan>("apply_translated_patch", {
    gameRoot,
    catalogPath,
    debugLogging,
  });
}

export function restoreOriginalText(gameRoot: string) {
  return invoke<RestorePlan>("restore_original_text", { gameRoot });
}

export function listenWorkflowEvents(
  handler: (workflowEvent: WorkflowEvent) => void,
): Promise<UnlistenFn> {
  if (!hasTauriRuntime()) {
    return Promise.resolve(() => {});
  }
  return listen<WorkflowEvent>("rush-patch://workflow-event", (event) => {
    handler(event.payload);
  });
}
