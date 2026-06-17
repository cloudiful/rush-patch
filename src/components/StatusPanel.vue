<script setup lang="ts">
import Card from "primevue/card";
import ProgressBar from "primevue/progressbar";
import Tag from "primevue/tag";
import { useI18n } from "vue-i18n";
import { useWorkflowStore } from "../stores/workflow";

const workflow = useWorkflowStore();
const { t } = useI18n();
</script>

<template>
  <Card class="border border-stone-200 shadow-none">
    <template #title>
      <div class="flex items-center justify-between gap-3">
        <span>{{ t("status.title") }}</span>
        <Tag
          :value="workflow.phaseLabel"
          :severity="workflow.busy ? 'warn' : 'success'"
        />
      </div>
    </template>

    <template #content>
      <div class="grid gap-3 text-sm">
        <ProgressBar
          :show-value="false"
          :value="workflow.progressValue"
          style="height: 1rem"
        />

        <div
          v-if="workflow.progressMessage || workflow.progressDetail"
          class="rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-sm text-stone-700"
        >
          <p class="font-medium">
            {{ workflow.progressMessage || workflow.phaseLabel }}
            <span
              v-if="workflow.progressSummaryText"
              class="ml-2 text-stone-500"
            >
              {{ workflow.progressSummaryText }}
            </span>
          </p>
          <p
            v-if="workflow.progressDetail"
            class="mt-1 text-xs text-stone-500"
          >
            {{ workflow.progressDetail }}
          </p>
        </div>

        <p
          v-if="workflow.error"
          class="rounded-xl bg-rose-50 px-3 py-2 text-rose-700"
        >
          {{ workflow.error }}
        </p>

        <div class="grid grid-cols-2 gap-2">
          <div class="rounded-xl bg-stone-50 px-3 py-2">
            <p class="text-xs text-stone-500">
              {{ t("status.engine") }}
            </p>
            <p class="font-semibold">
              {{ workflow.scan?.engine || "-" }}
            </p>
          </div>
          <div class="rounded-xl bg-stone-50 px-3 py-2">
            <p class="text-xs text-stone-500">
              {{ t("status.files") }}
            </p>
            <p class="font-semibold">
              {{ workflow.scan ? `${workflow.scan.dataFiles.length}+${workflow.scan.pluginFiles.length}` : "-" }}
            </p>
          </div>
          <div class="rounded-xl bg-stone-50 px-3 py-2">
            <p class="text-xs text-stone-500">
              {{ t("status.units") }}
            </p>
            <p class="font-semibold">
              {{ workflow.preview?.totalUnits ?? workflow.catalogEstimate?.totalUnits ?? "-" }}
            </p>
          </div>
          <div class="rounded-xl bg-stone-50 px-3 py-2">
            <p class="text-xs text-stone-500">
              {{ t("status.spans") }}
            </p>
            <p class="font-semibold">
              {{ workflow.preview?.totalSpans ?? "-" }}
            </p>
          </div>
        </div>

        <div
          v-if="workflow.catalogEstimate"
          class="rounded-xl bg-violet-50 px-3 py-2 text-violet-900"
        >
          {{ t("status.tokenEstimate", {
            pending: workflow.catalogEstimate.pendingUnits,
            reused: workflow.catalogEstimate.reusedUnits,
            batches: workflow.catalogEstimate.estimatedBatches,
            total: workflow.catalogEstimate.estimatedTotalTokens,
          }) }}
        </div>

        <div
          v-if="workflow.translation"
          class="rounded-xl bg-amber-50 px-3 py-2 text-amber-900"
        >
          {{ workflow.translation.cancelled ? t("status.cancelled") : t("status.translated") }}
          {{ workflow.translation.translatedUnits }}/{{ workflow.translation.attemptedUnits }}
          · {{ t("status.failed") }} {{ workflow.translation.failedUnits }}
        </div>

        <div
          v-if="workflow.patchPlan"
          class="rounded-xl bg-sky-50 px-3 py-2 text-sky-900"
        >
          {{ t("status.patched", { count: workflow.patchPlan.updatedFiles, backups: workflow.patchPlan.backedUpFiles }) }}
        </div>

        <div
          v-if="workflow.restorePlan"
          class="rounded-xl bg-emerald-50 px-3 py-2 text-emerald-900"
        >
          {{ t("status.restored", { count: workflow.restorePlan.restoredFiles }) }}
        </div>
      </div>
    </template>
  </Card>
</template>
