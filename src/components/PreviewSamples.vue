<script setup lang="ts">
import Card from "primevue/card";
import { useI18n } from "vue-i18n";
import { useWorkflowStore } from "../stores/workflow";

const workflow = useWorkflowStore();
const { t } = useI18n();
</script>

<template>
  <Card
    v-if="workflow.preview?.sampleUnits?.length"
    class="border border-stone-200 shadow-none"
  >
    <template #title>
      {{ t("preview.title") }}
    </template>

    <template #content>
      <div class="grid max-h-80 gap-2 overflow-auto">
        <div
          v-for="unit in workflow.preview.sampleUnits"
          :key="unit.id"
          class="rounded-xl border border-stone-200 bg-stone-50 px-3 py-2"
        >
          <div class="flex items-center justify-between gap-3 text-xs">
            <span class="font-semibold text-stone-700">{{ unit.semanticKind }}</span>
            <span class="text-stone-500">{{ unit.context.speakerName || t("preview.noSpeaker") }}</span>
          </div>

          <p class="mt-1 line-clamp-3 whitespace-pre-wrap text-xs leading-5 text-stone-700">
            {{ unit.sourceText }}
          </p>
        </div>
      </div>
    </template>
  </Card>
</template>
