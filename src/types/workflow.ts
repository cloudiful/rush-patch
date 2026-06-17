export type ScanSummary = {
  gameRoot: string;
  engine: "MV" | "MZ";
  hasDataDir: boolean;
  hasPluginDir: boolean;
  dataFiles: string[];
  pluginFiles: string[];
};

export type JsonExtractionPreview = {
  totalUnits: number;
  totalSpans: number;
  sampleUnits: TranslationPreviewUnit[];
};

export type TranslationPreviewUnit = {
  id: string;
  semanticKind: string;
  sourceText: string;
  context: {
    speakerName?: string | null;
    notes: string[];
  };
};

export type ProjectForm = {
  gameRoot: string;
  apiKey: string;
  baseUrl: string;
  model: string;
  apiEndpoint: ApiEndpoint;
  systemPrompt: string;
  glossaryPath: string;
  doNotTranslatePath: string;
  gameProfile: GameProfile;
  targetInputTokens: number;
  batchingStrategy: BatchingStrategy;
  debugLogging: boolean;
  maxConcurrency: number;
  requestTimeoutSecs: number;
  sourceLang: string;
  targetLang: string;
};

export type ProjectConfig = {
  gameRoot: string;
  model: string;
  apiEndpoint: ApiEndpoint;
  apiKey: string | null;
  baseUrl: string | null;
  systemPrompt: string;
  glossaryPath: string | null;
  doNotTranslatePath: string | null;
  gameProfile: GameProfile;
  targetInputTokens: number;
  batchingStrategy: BatchingStrategy;
  debugLogging: boolean;
  maxConcurrency: number;
  requestTimeoutSecs: number;
  sourceLang: string;
  targetLang: string;
};

export type ApiEndpoint = "responses" | "chat_completions";

export type GameProfile = "general_rpg" | "adult_rpg" | "custom";

export type BatchingStrategy = "maximize_utilization" | "quality_first";

export type StoredAppConfig = Omit<ProjectConfig, "apiKey">;

export type AppSettings = {
  config: StoredAppConfig;
  apiKey: string | null;
  keyringAvailable: boolean;
  keyringError: string | null;
};

export type SaveAppSettingsRequest = {
  config: StoredAppConfig;
  apiKey: string | null;
};

export type SaveAppSettingsSummary = {
  configPath: string;
  keyringAvailable: boolean;
};

export type TranslationRunSummary = {
  catalogPath: string;
  totalUnits: number;
  attemptedUnits: number;
  translatedUnits: number;
  pretranslatedTerms: number;
  failedTerms: number;
  failedUnits: number;
  validationFailedUnits: number;
  validationWarningUnits: number;
  batches: number;
  retries: number;
  cancelled: boolean;
};

export type CatalogTokenEstimate = {
  catalogPath: string;
  totalUnits: number;
  pendingUnits: number;
  reusedUnits: number;
  targetInputTokens: number;
  batchingStrategy: BatchingStrategy;
  estimatedTermBatches: number;
  estimatedTermInputTokens: number;
  estimatedTermOutputTokens: number;
  estimatedMainBatches: number;
  estimatedSceneBatches: number;
  estimatedOrphanPoolBatches: number;
  estimatedMainInputTokens: number;
  estimatedMainOutputTokens: number;
  estimatedBatches: number;
  estimatedInputTokens: number;
  estimatedOutputTokens: number;
  estimatedTotalTokens: number;
  estimatedAverageInputTokens: number;
  estimatedAverageInputUtilizationPct: number;
  averageMainInputUtilizationPct: number;
};

export type PatchPlan = {
  gameRoot: string;
  backupRoot: string;
  backedUpFiles: number;
  updatedFiles: number;
  preservedFailedUnits: number;
  validationFailedUnits: number;
  validationWarningUnits: number;
};

export type RestorePlan = {
  gameRoot: string;
  backupRoot: string;
  restoredFiles: number;
};

export type WorkflowPhase =
  | "idle"
  | "scan"
  | "catalog"
  | "estimate"
  | "translate"
  | "patch"
  | "done";

export type WorkflowEventLevel = "info" | "debug" | "warn" | "error";

export type WorkflowEvent = {
  phase: WorkflowPhase;
  level: WorkflowEventLevel;
  messageKey: string | null;
  message: string;
  current: number | null;
  total: number | null;
  detail: string | null;
  timestampMs: number;
  debug: Record<string, string> | null;
};
