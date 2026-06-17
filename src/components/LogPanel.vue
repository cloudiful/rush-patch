<script setup lang="ts">
import { computed } from "vue";
import { useRouter } from "vue-router";
import Button from "primevue/button";
import Card from "primevue/card";
import { useI18n } from "vue-i18n";
import { useWorkflowStore } from "../stores/workflow";

const props = withDefaults(
  defineProps<{
    mode?: "compact" | "page";
  }>(),
  {
    mode: "compact",
  },
);

const workflow = useWorkflowStore();
const router = useRouter();
const { t } = useI18n();

const compactMode = computed(() => props.mode === "compact");
const containerClass = computed(() =>
  compactMode.value
    ? "max-h-72"
    : "min-h-[72vh] max-h-[72vh] lg:min-h-[78vh] lg:max-h-[78vh]",
);
const entryClass = computed(() =>
  compactMode.value
    ? "mb-2 rounded-lg border border-stone-200 bg-white px-2.5 py-2 last:mb-0"
    : "mb-3 rounded-xl border border-stone-200 bg-white px-3 py-3 last:mb-0",
);
const detailClass = computed(() =>
  compactMode.value ? "mt-1 break-all text-stone-500" : "mt-1 break-all text-sm text-stone-500",
);

function openLogsPage() {
  if (router.currentRoute.value.path !== "/logs") {
    void router.push("/logs");
  }
}

function formatTime(timestampMs: number) {
  return new Intl.DateTimeFormat(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(new Date(timestampMs));
}
</script>

<template>
  <Card class="border border-stone-200 shadow-none">
    <template #title>
      <div class="flex items-center justify-between gap-3">
        <div>
          <p>{{ t("logs.title") }}</p>
          <p class="mt-1 text-xs font-medium text-stone-400">
            {{ t("logs.entryCount", { count: workflow.logEntries.length }) }}
            <span class="mx-1">·</span>
            {{ workflow.phaseLabel || "idle" }}
          </p>
        </div>

        <Button
          v-if="compactMode"
          :label="t('logs.openPage')"
          severity="secondary"
          text
          @click="openLogsPage"
        />
      </div>
    </template>

    <template #content>
      <div
        class="overflow-auto rounded-xl border border-stone-200 bg-stone-50 p-3 font-mono text-xs text-stone-700"
        :class="containerClass"
      >
        <div
          v-for="entry in workflow.logEntries"
          :key="entry.id"
          :class="entryClass"
        >
          <div class="flex items-center justify-between gap-3">
            <div class="flex items-center gap-2">
              <span
                class="rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide"
                :class="{
                  'bg-sky-100 text-sky-700': entry.level === 'info',
                  'bg-amber-100 text-amber-800': entry.level === 'warn',
                  'bg-rose-100 text-rose-700': entry.level === 'error',
                  'bg-stone-200 text-stone-700': entry.level === 'debug',
                }"
              >
                {{ t(`logs.level.${entry.level}`) }}
              </span>
              <span class="text-[10px] tracking-wide text-stone-400">
                {{ t(`phase.${entry.phase}`) }}
              </span>
              <span
                v-if="entry.current != null && entry.total != null && entry.total > 0"
                class="rounded bg-stone-100 px-1.5 py-0.5 text-[10px] text-stone-500"
              >
                {{ entry.current }}/{{ entry.total }}
              </span>
            </div>

            <span class="shrink-0 text-[10px] uppercase tracking-wide text-stone-400">
              {{ formatTime(entry.timestampMs) }}
            </span>
          </div>

          <p
            class="mt-1 whitespace-pre-wrap break-all"
            :class="compactMode ? '' : 'text-sm leading-6'"
          >
            {{ entry.text }}
          </p>

          <p
            v-if="entry.detail"
            :class="detailClass"
          >
            {{ entry.detail }}
          </p>

          <p
            v-if="entry.debug"
            class="mt-2 whitespace-pre-wrap break-all rounded-lg bg-stone-50 px-2 py-1 text-stone-400"
            :class="compactMode ? '' : 'text-sm leading-6'"
          >
            {{ Object.entries(entry.debug).map(([key, value]) => `${key}=${value}`).join(" ") }}
          </p>
        </div>

        <div
          v-if="workflow.logEntries.length === 0"
          class="rounded-xl border border-dashed border-stone-300 bg-white/70 px-4 py-8 text-center text-stone-400"
        >
          {{ t("logs.empty") }}
        </div>
      </div>
    </template>
  </Card>
</template>
