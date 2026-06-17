import { describe, expect, test } from "bun:test";
import {
  appendWorkflowLog,
  createWorkflowLogEntry,
  progressValueForPhase,
  updateWorkflowProgress,
} from "./workflowRuntime";
import type { WorkflowEvent } from "../types/workflow";

describe("workflowRuntime", () => {
  test("drops debug logs when debug logging is disabled", () => {
    const debugEntry = createWorkflowLogEntry("details", "debug", "translate");
    const infoEntry = createWorkflowLogEntry("info", "info", "translate");

    const entries = appendWorkflowLog([infoEntry], debugEntry, false);

    expect(entries).toHaveLength(1);
    expect(entries[0].text).toBe("info");
  });

  test("updates progress from workflow events", () => {
    const workflowEvent: WorkflowEvent = {
      phase: "catalog",
      level: "info",
      messageKey: null,
      message: "Writing catalog chunks",
      current: 2,
      total: 4,
      detail: "Chunk 2/4",
      timestampMs: 42,
      debug: null,
    };

    const progress = updateWorkflowProgress(null, workflowEvent);

    expect(progress?.phase).toBe("catalog");
    expect(progress?.level).toBe("info");
    expect(progress?.current).toBe(2);
    expect(progress?.detail).toBeNull();
  });

  test("derives weighted progress bar value from phase progress", () => {
    const value = progressValueForPhase(
      "translate",
      {
        phase: "translate",
        level: "info",
        message: "Completed translation batches",
        detail: "Batch 5/10",
        current: 5,
        total: 10,
        timestampMs: 0,
      },
      true,
      false,
      false,
      false,
      false,
      true,
    );

    expect(value).toBeGreaterThan(64);
    expect(value).toBeLessThan(92);
  });

  test("merges consecutive progress logs with the same message", () => {
    const previous = createWorkflowLogEntry("正在提取 JSON 文本", "info", "catalog", "1/10", 1, 10);
    const next = createWorkflowLogEntry("正在提取 JSON 文本", "info", "catalog", "2/10", 2, 10);

    const entries = appendWorkflowLog([previous], next, false);

    expect(entries).toHaveLength(1);
    expect(entries[0].detail).toBe("2/10");
    expect(entries[0].current).toBe(2);
    expect(entries[0].total).toBe(10);
  });
});
