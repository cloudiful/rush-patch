import { translate } from "../i18n";
import type { WorkflowEvent, WorkflowEventLevel, WorkflowPhase } from "../types/workflow";

export type WorkflowLogEntry = {
  id: string;
  level: WorkflowEventLevel;
  phase: WorkflowPhase;
  text: string;
  detail: string | null;
  current: number | null;
  total: number | null;
  timestampMs: number;
  debug: Record<string, string> | null;
};

export type WorkflowProgressState = {
  phase: WorkflowPhase;
  level: WorkflowEventLevel;
  message: string;
  detail: string | null;
  current: number | null;
  total: number | null;
  timestampMs: number;
};

const MAX_STANDARD_LOGS = 300;
const MAX_DEBUG_LOGS = 1000;

const PHASE_RANGES: Record<WorkflowPhase, [number, number]> = {
  idle: [0, 0],
  scan: [4, 20],
  catalog: [20, 52],
  estimate: [52, 64],
  translate: [64, 92],
  patch: [92, 99],
  done: [100, 100],
};

export function createWorkflowLogEntry(
  text: string,
  level: WorkflowEventLevel,
  phase: WorkflowPhase,
  detail: string | null = null,
  current: number | null = null,
  total: number | null = null,
  debug: Record<string, string> | null = null,
  timestampMs = Date.now(),
): WorkflowLogEntry {
  return {
    id: `${timestampMs}-${phase}-${level}-${text}-${detail ?? ""}-${current ?? ""}-${total ?? ""}`,
    level,
    phase,
    text,
    detail,
    current,
    total,
    debug,
    timestampMs,
  };
}

export function createWorkflowLogEntryFromEvent(workflowEvent: WorkflowEvent): WorkflowLogEntry {
  const translatedMessage = workflowEvent.messageKey
    ? translate(workflowEvent.messageKey)
    : workflowEvent.message;
  return createWorkflowLogEntry(
    translatedMessage,
    workflowEvent.level,
    workflowEvent.phase,
    workflowEvent.level === "info" ? null : workflowEvent.detail,
    workflowEvent.current,
    workflowEvent.total,
    workflowEvent.debug,
    workflowEvent.timestampMs,
  );
}

export function appendWorkflowLog(
  entries: WorkflowLogEntry[],
  entry: WorkflowLogEntry,
  debugEnabled: boolean,
): WorkflowLogEntry[] {
  if (entry.level === "debug" && !debugEnabled) {
    return entries;
  }
  const limit = debugEnabled ? MAX_DEBUG_LOGS : MAX_STANDARD_LOGS;
  const latest = entries[0];
  if (
    latest &&
    latest.level === entry.level &&
    latest.phase === entry.phase &&
    latest.text === entry.text &&
    latest.current != null &&
    latest.total != null &&
    entry.current != null &&
    entry.total != null
  ) {
    return [entry, ...entries.slice(1)].slice(0, limit);
  }
  return [entry, ...entries].slice(0, limit);
}

export function updateWorkflowProgress(
  previous: WorkflowProgressState | null,
  workflowEvent: WorkflowEvent,
): WorkflowProgressState | null {
  const translatedMessage = workflowEvent.messageKey
    ? translate(workflowEvent.messageKey)
    : workflowEvent.message;
  if (workflowEvent.phase === "idle") {
    return null;
  }
  if (workflowEvent.phase === "done") {
    return {
      phase: workflowEvent.phase,
      level: workflowEvent.level,
      message: translatedMessage,
      detail: workflowEvent.detail,
      current: workflowEvent.current,
      total: workflowEvent.total,
      timestampMs: workflowEvent.timestampMs,
    };
  }
  if (
    workflowEvent.current == null &&
    workflowEvent.total == null &&
    workflowEvent.detail == null &&
    workflowEvent.level === "debug"
  ) {
    return previous;
  }
  return {
    phase: workflowEvent.phase,
    level: workflowEvent.level,
    message: translatedMessage,
    detail: workflowEvent.level === "info" ? null : workflowEvent.detail,
    current: workflowEvent.current,
    total: workflowEvent.total,
    timestampMs: workflowEvent.timestampMs,
  };
}

export function progressValueForPhase(
  phase: WorkflowPhase,
  progress: WorkflowProgressState | null,
  busy: boolean,
  hasPatchPlan: boolean,
  hasTranslation: boolean,
  hasCatalogPath: boolean,
  hasPreview: boolean,
  hasScan: boolean,
) {
  if (busy) {
    const [start, end] = PHASE_RANGES[phase];
    const current = progress?.current;
    const total = progress?.total;
    if (current != null && total != null && total > 0 && end > start) {
      const ratio = Math.min(current / total, 1);
      return Math.round(start + (end - start) * ratio);
    }
    return start;
  }
  if (hasPatchPlan) return 100;
  if (phase === "done") return 100;
  if (hasTranslation) return 76;
  if (hasCatalogPath) return 48;
  if (hasPreview) return 32;
  if (hasScan) return 18;
  return 0;
}

export function progressSummary(progress: WorkflowProgressState | null) {
  if (!progress) return "";
  if (progress.current != null && progress.total != null && progress.total > 0) {
    return `${progress.current}/${progress.total}`;
  }
  return "";
}
