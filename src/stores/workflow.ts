import { defineStore } from "pinia";
import { computed, reactive, ref, watch } from "vue";
import {
  applyTranslatedPatch,
  buildCatalog,
  cancelTranslation,
  estimateCatalog,
  listenWorkflowEvents,
  loadAppSettings,
  previewExtraction,
  restoreOriginalText,
  saveAppSettings,
  scanProject,
  translateCatalog,
} from "../api/tauri";
import { currentLocale, translate } from "../i18n";
import type {
  ApiEndpoint,
  CatalogTokenEstimate,
  JsonExtractionPreview,
  PatchPlan,
  ProjectConfig,
  ProjectForm,
  RestorePlan,
  ScanSummary,
  StoredAppConfig,
  TranslationRunSummary,
  WorkflowEvent,
  WorkflowPhase,
} from "../types/workflow";
import {
  appendWorkflowLog,
  createWorkflowLogEntry,
  createWorkflowLogEntryFromEvent,
  progressSummary,
  progressValueForPhase,
  updateWorkflowProgress,
  type WorkflowLogEntry,
  type WorkflowProgressState,
} from "./workflowRuntime";

const defaultPrompt =
  "Translate Japanese RPG text into natural Chinese while preserving placeholders and control codes.";
const settingsSaveDelayMs = 500;
const defaultApiEndpoint: ApiEndpoint = "responses";

type LogParams = Record<string, number | string>;

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function optionalText(value: string) {
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function isCancellationMessage(message: string) {
  return message.toLowerCase().includes("cancelled");
}

export const useWorkflowStore = defineStore("workflow", () => {
  const form = reactive<ProjectForm>({
    gameRoot: "",
    apiKey: "",
    baseUrl: "",
    model: "gpt-4.1-mini",
    apiEndpoint: defaultApiEndpoint,
    systemPrompt: defaultPrompt,
    glossaryPath: "",
    doNotTranslatePath: "",
    gameProfile: "general_rpg",
    targetInputTokens: 6000,
    batchingStrategy: "maximize_utilization",
    debugLogging: false,
    maxConcurrency: 1,
    requestTimeoutSecs: 90,
    sourceLang: "Japanese",
    targetLang: "Chinese",
  });

  const busy = ref(false);
  const translating = ref(false);
  const estimating = ref(false);
  const settingsLoaded = ref(false);
  const settingsSaving = ref(false);
  const settingsError = ref("");
  const phase = ref<WorkflowPhase>("idle");
  const error = ref("");
  const logEntries = ref<WorkflowLogEntry[]>([
    createWorkflowLogEntry(translate("logs.ready"), "info", "idle"),
  ]);
  const progress = ref<WorkflowProgressState | null>(null);
  const scan = ref<ScanSummary | null>(null);
  const preview = ref<JsonExtractionPreview | null>(null);
  const catalogPath = ref("");
  const catalogEstimate = ref<CatalogTokenEstimate | null>(null);
  const translation = ref<TranslationRunSummary | null>(null);
  const patchPlan = ref<PatchPlan | null>(null);
  const restorePlan = ref<RestorePlan | null>(null);

  let loadingSettings = false;
  let workflowEventsReady = false;
  let saveTimer: ReturnType<typeof setTimeout> | null = null;

  const canRun = computed(
    () =>
      form.gameRoot.trim().length > 0 &&
      form.apiKey.trim().length > 0 &&
      form.model.trim().length > 0 &&
      !busy.value,
  );
  const canPreview = computed(() => form.gameRoot.trim().length > 0 && !busy.value);
  const canRestore = computed(() => form.gameRoot.trim().length > 0 && !busy.value);
  const canEstimate = computed(
    () => form.gameRoot.trim().length > 0 && form.model.trim().length > 0 && !busy.value,
  );
  const cancellable = computed(() => busy.value && (translating.value || estimating.value));
  const progressValue = computed(() =>
    progressValueForPhase(
      phase.value,
      progress.value,
      busy.value,
      Boolean(patchPlan.value),
      Boolean(translation.value),
      Boolean(catalogPath.value),
      Boolean(preview.value),
      Boolean(scan.value),
    ),
  );
  const phaseLabel = computed(() => {
    const activeLocale = currentLocale.value;
    return activeLocale ? translate(`phase.${phase.value}`) : "";
  });
  const progressSummaryText = computed(() => progressSummary(progress.value));
  const progressMessage = computed(() => progress.value?.message ?? "");
  const progressDetail = computed(() => progress.value?.detail ?? "");

  void ensureWorkflowEvents();

  function message(key: string, params?: LogParams) {
    return translate(key, params);
  }

  function appendLogEntry(entry: WorkflowLogEntry) {
    logEntries.value = appendWorkflowLog(logEntries.value, entry, form.debugLogging);
  }

  function log(key: string, params?: LogParams) {
    appendLogEntry(createWorkflowLogEntry(message(key, params), "info", "idle"));
  }

  function logError(key: string, params?: LogParams) {
    appendLogEntry(createWorkflowLogEntry(message(key, params), "error", "idle"));
  }

  function onWorkflowEvent(workflowEvent: WorkflowEvent) {
    phase.value = workflowEvent.phase;
    progress.value = updateWorkflowProgress(progress.value, workflowEvent);
    appendLogEntry(createWorkflowLogEntryFromEvent(workflowEvent));
    if (workflowEvent.level === "error") {
      error.value = workflowEvent.detail ?? workflowEvent.message;
    }
  }

  async function ensureWorkflowEvents() {
    if (workflowEventsReady) return;
    workflowEventsReady = true;
    try {
      await listenWorkflowEvents(onWorkflowEvent);
    } catch (caught) {
      workflowEventsReady = false;
      const messageText = `Failed to attach workflow event listener: ${errorMessage(caught)}`;
      appendLogEntry(createWorkflowLogEntry(messageText, "warn", "idle"));
    }
  }

  function resetWorkflowState() {
    error.value = "";
    progress.value = null;
    patchPlan.value = null;
    restorePlan.value = null;
  }

  function toStoredAppConfig(): StoredAppConfig {
    return {
      gameRoot: form.gameRoot,
      model: form.model.trim(),
      apiEndpoint: form.apiEndpoint,
      baseUrl: optionalText(form.baseUrl),
      systemPrompt: form.systemPrompt,
      glossaryPath: optionalText(form.glossaryPath),
      doNotTranslatePath: optionalText(form.doNotTranslatePath),
      gameProfile: form.gameProfile,
      targetInputTokens: Number(form.targetInputTokens) || 6000,
      batchingStrategy: form.batchingStrategy,
      debugLogging: form.debugLogging,
      maxConcurrency: Number(form.maxConcurrency) || 1,
      requestTimeoutSecs: Number(form.requestTimeoutSecs) || 90,
      sourceLang: form.sourceLang,
      targetLang: form.targetLang,
    };
  }

  function toProjectConfig(): ProjectConfig {
    return {
      ...toStoredAppConfig(),
      apiKey: optionalText(form.apiKey),
    };
  }

  function applyStoredConfig(config: StoredAppConfig, apiKey: string | null) {
    form.gameRoot = config.gameRoot ?? "";
    form.model = config.model || "gpt-4.1-mini";
    form.apiEndpoint = config.apiEndpoint ?? defaultApiEndpoint;
    form.baseUrl = config.baseUrl ?? "";
    form.systemPrompt = config.systemPrompt || defaultPrompt;
    form.glossaryPath = config.glossaryPath ?? "";
    form.doNotTranslatePath = config.doNotTranslatePath ?? "";
    form.gameProfile = config.gameProfile ?? "general_rpg";
    form.targetInputTokens =
      Number((config as StoredAppConfig & { maxInputTokens?: number }).targetInputTokens) ||
      Number((config as StoredAppConfig & { maxInputTokens?: number }).maxInputTokens) ||
      6000;
    form.batchingStrategy = config.batchingStrategy ?? "maximize_utilization";
    form.debugLogging = Boolean(config.debugLogging);
    form.maxConcurrency = Number(config.maxConcurrency) || 1;
    form.requestTimeoutSecs = Number(config.requestTimeoutSecs) || 90;
    form.sourceLang = config.sourceLang || "Japanese";
    form.targetLang = config.targetLang || "Chinese";
    form.apiKey = apiKey ?? "";
  }

  async function loadPersistedSettings() {
    if (settingsLoaded.value) return;

    await ensureWorkflowEvents();
    loadingSettings = true;
    try {
      const settings = await loadAppSettings();
      applyStoredConfig(settings.config, settings.apiKey);
      if (!settings.keyringAvailable && settings.keyringError) {
        const params = { message: settings.keyringError };
        settingsError.value = message("logs.keyringReadFailed", params);
        logError("logs.keyringReadFailed", params);
      }
    } catch (caught) {
      const params = { message: errorMessage(caught) };
      settingsError.value = message("logs.settingsReadFailed", params);
      logError("logs.settingsReadFailed", params);
    } finally {
      loadingSettings = false;
      settingsLoaded.value = true;
    }
  }

  async function savePersistedSettings() {
    settingsSaving.value = true;
    settingsError.value = "";
    try {
      await saveAppSettings({
        config: toStoredAppConfig(),
        apiKey: optionalText(form.apiKey),
      });
    } catch (caught) {
      const params = { message: errorMessage(caught) };
      settingsError.value = message("logs.settingsSaveFailed", params);
      logError("logs.settingsSaveFailed", params);
    } finally {
      settingsSaving.value = false;
    }
  }

  function scheduleSettingsSave() {
    if (!settingsLoaded.value || loadingSettings) return;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      saveTimer = null;
      void savePersistedSettings();
    }, settingsSaveDelayMs);
  }

  function validateRunForm() {
    if (!form.gameRoot.trim()) return "validation.gameRootRequired";
    if (!form.apiKey.trim()) return "validation.apiKeyRequired";
    if (!form.model.trim()) return "validation.modelRequired";
    return "";
  }

  async function runMainWorkflow() {
    if (busy.value) return;

    const validationError = validateRunForm();
    if (validationError) {
      error.value = message(validationError);
      logError(validationError);
      return;
    }

    await savePersistedSettings();

    busy.value = true;
    translating.value = false;
    estimating.value = false;
    resetWorkflowState();

    try {
      phase.value = "scan";
      scan.value = await scanProject(form.gameRoot);
      preview.value = null;
      catalogPath.value = "";
      catalogEstimate.value = null;
      translation.value = null;

      phase.value = "catalog";
      catalogPath.value = await buildCatalog(form.gameRoot, form.debugLogging);

      phase.value = "translate";
      translating.value = true;
      translation.value = await translateCatalog(catalogPath.value, toProjectConfig());
      translating.value = false;

      if (translation.value.cancelled) {
        phase.value = "idle";
        return;
      }

      phase.value = "patch";
      patchPlan.value = await applyTranslatedPatch(
        form.gameRoot,
        catalogPath.value,
        form.debugLogging,
      );
      phase.value = "done";
      progress.value = {
        phase: "done",
        level: "info",
        message: message("workflow.done"),
        detail: message("status.patched", {
          count: patchPlan.value.updatedFiles,
          backups: patchPlan.value.backedUpFiles,
        }),
        current: patchPlan.value.updatedFiles,
        total: patchPlan.value.updatedFiles,
        timestampMs: Date.now(),
      };
    } catch (caught) {
      const messageText = errorMessage(caught);
      if (!isCancellationMessage(messageText)) {
        error.value = messageText;
        if (phase.value === "scan") {
          logError("logs.workflowFailed", { message: messageText });
        }
      }
    } finally {
      busy.value = false;
      translating.value = false;
      estimating.value = false;
      if (phase.value !== "done") {
        phase.value = "idle";
      }
    }
  }

  async function estimateTokens() {
    if (busy.value) return;
    if (!form.gameRoot.trim()) {
      error.value = message("validation.gameRootRequired");
      logError("validation.gameRootRequired");
      return;
    }
    if (!form.model.trim()) {
      error.value = message("validation.modelRequired");
      logError("validation.modelRequired");
      return;
    }

    await savePersistedSettings();

    busy.value = true;
    estimating.value = false;
    resetWorkflowState();

    try {
      phase.value = "scan";
      scan.value = await scanProject(form.gameRoot);

      phase.value = "catalog";
      catalogPath.value = await buildCatalog(form.gameRoot, form.debugLogging);

      phase.value = "estimate";
      estimating.value = true;
      catalogEstimate.value = await estimateCatalog(catalogPath.value, toProjectConfig());
      estimating.value = false;
    } catch (caught) {
      const messageText = errorMessage(caught);
      if (!isCancellationMessage(messageText)) {
        error.value = messageText;
        if (phase.value === "scan") {
          logError("logs.workflowFailed", { message: messageText });
        }
      }
    } finally {
      busy.value = false;
      estimating.value = false;
      phase.value = "idle";
    }
  }

  async function runTask<T>(labelKey: string, failedKey: string, task: () => Promise<T>) {
    busy.value = true;
    error.value = "";
    log(labelKey);
    try {
      return await task();
    } catch (caught) {
      const messageText = errorMessage(caught);
      error.value = messageText;
      logError(failedKey, { message: messageText });
      return null;
    } finally {
      busy.value = false;
    }
  }

  async function previewText() {
    const result = await runTask("logs.previewExtract", "logs.previewFailed", () =>
      previewExtraction(form.gameRoot),
    );
    if (!result) return;

    preview.value = result;
    log("logs.previewSummary", { units: result.totalUnits, spans: result.totalSpans });
  }

  async function restoreOriginal() {
    const result = await runTask("logs.restoreOriginal", "logs.restoreFailed", () =>
      restoreOriginalText(form.gameRoot),
    );
    if (!result) return;

    restorePlan.value = result;
    patchPlan.value = null;
    log("logs.restoreComplete", { count: result.restoredFiles });
  }

  async function cancelTranslationRequest() {
    if (!cancellable.value) return;

    try {
      await cancelTranslation();
      log("logs.cancelRequested");
    } catch (caught) {
      const messageText = errorMessage(caught);
      error.value = messageText;
      logError("logs.cancelFailed", { message: messageText });
    }
  }

  watch(form, scheduleSettingsSave, { deep: true });
  watch(
    () => form.debugLogging,
    (enabled) => {
      if (enabled) return;
      logEntries.value = logEntries.value
        .filter((entry) => entry.level !== "debug")
        .slice(0, 300);
    },
  );

  return {
    form,
    busy,
    translating,
    estimating,
    settingsLoaded,
    settingsSaving,
    settingsError,
    phase,
    phaseLabel,
    error,
    logEntries,
    progress,
    progressSummaryText,
    progressMessage,
    progressDetail,
    scan,
    preview,
    catalogPath,
    catalogEstimate,
    translation,
    patchPlan,
    restorePlan,
    canRun,
    canPreview,
    canRestore,
    canEstimate,
    cancellable,
    progressValue,
    loadPersistedSettings,
    savePersistedSettings,
    runMainWorkflow,
    estimateTokens,
    previewText,
    restoreOriginal,
    cancelTranslationRequest,
  };
});
