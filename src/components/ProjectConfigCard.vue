<script setup lang="ts">
import { ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import Button from "primevue/button";
import Card from "primevue/card";
import InputText from "primevue/inputtext";
import Panel from "primevue/panel";
import Textarea from "primevue/textarea";
import { useI18n } from "vue-i18n";
import FieldHelpLabel from "./FieldHelpLabel.vue";
import { useWorkflowStore } from "../stores/workflow";

const workflow = useWorkflowStore();
const { locale, t } = useI18n();
const batchCollapsed = ref(true);
const glossaryCollapsed = ref(true);
const gameProfileOptions = ["general_rpg", "adult_rpg", "custom"] as const;
const batchingStrategyOptions = ["maximize_utilization", "quality_first"] as const;
const batchingStrategyLabels = {
  "zh-CN": {
    label: "批处理策略",
    help: "尽量吃满会优先减少请求数，让每批更接近目标 Token；质量优先会更保守，减少跨组混合。",
    maximize_utilization: "尽量吃满",
    quality_first: "质量优先",
  },
  "en-US": {
    label: "Batching strategy",
    help: "Maximize utilization reduces request count and tries to stay close to the token target. Quality first stays more conservative and mixes fewer groups.",
    maximize_utilization: "Maximize utilization",
    quality_first: "Quality first",
  },
} as const;

const mdiFolderOpen =
  "M19 20H4C2.89 20 2 19.1 2 18V6C2 4.89 2.89 4 4 4H10L12 6H20C21.1 6 22 6.9 22 8H5C3.9 8 3 8.9 3 10V18L5.14 11.5C5.41 10.66 6.2 10 7.1 10H21.8C22.9 10 23.7 11.1 23.3 12.1L20.9 18.6C20.6 19.4 19.9 20 19 20Z";

async function pickGameDirectory() {
  let selected: string | string[] | null;
  try {
    selected = await open({
      directory: true,
      multiple: false,
      title: t("config.chooseGameRoot"),
    });
  } catch (error) {
    console.error("Failed to open directory picker", error);
    return;
  }

  if (typeof selected === "string") {
    workflow.form.gameRoot = selected;
  }
}

function activeBatchingLabels() {
  return batchingStrategyLabels[locale.value as keyof typeof batchingStrategyLabels]
    ?? batchingStrategyLabels["en-US"];
}

function batchingLabel(strategy: (typeof batchingStrategyOptions)[number]) {
  return activeBatchingLabels()[strategy];
}
</script>

<template>
  <Card class="border border-stone-200 shadow-none">
    <template #title>
      {{ t("config.title") }}
    </template>

    <template #content>
      <div class="grid gap-4">
        <label class="grid min-w-0 gap-1.5">
          <span class="text-xs font-semibold text-stone-500">{{ t("config.gameRoot") }}</span>
          <div class="flex min-w-0 flex-wrap gap-2 sm:flex-nowrap">
            <InputText
              v-model="workflow.form.gameRoot"
              class="min-w-[14rem] flex-1"
              placeholder="C:\\Games\\ExampleRpg"
            />
            <Button
              :aria-label="t('config.chooseGameRoot')"
              class="shrink-0"
              severity="secondary"
              outlined
              @click="pickGameDirectory"
            >
              <svg
                class="h-5 w-5"
                viewBox="0 0 24 24"
                aria-hidden="true"
              >
                <path
                  :d="mdiFolderOpen"
                  fill="currentColor"
                />
              </svg>
            </Button>
          </div>
        </label>

        <Panel :header="t('config.translation')">
          <div class="rp-field-grid">
            <label class="grid min-w-0 gap-1.5">
              <span class="text-xs font-semibold text-stone-500">{{ t("config.model") }}</span>
              <InputText
                v-model="workflow.form.model"
                class="w-full min-w-0"
                placeholder="gpt-4.1-mini"
              />
            </label>
          </div>
        </Panel>

        <Panel
          v-model:collapsed="batchCollapsed"
          :header="t('config.batch')"
          toggleable
        >
          <div class="rp-field-grid">
            <label class="grid min-w-0 gap-1.5">
              <FieldHelpLabel
                :label="t('config.targetInputTokens')"
                :help="t('configHelp.targetInputTokens')"
              />
              <InputText
                v-model.number="workflow.form.targetInputTokens"
                class="w-full min-w-0"
                type="number"
                min="1"
              />
            </label>

            <label class="grid min-w-0 gap-1.5">
              <FieldHelpLabel
                :label="activeBatchingLabels().label"
                :help="activeBatchingLabels().help"
              />
              <select
                v-model="workflow.form.batchingStrategy"
                class="rp-select"
              >
                <option
                  v-for="strategy in batchingStrategyOptions"
                  :key="strategy"
                  :value="strategy"
                >
                  {{ batchingLabel(strategy) }}
                </option>
              </select>
            </label>

            <label class="grid min-w-0 gap-1.5">
              <FieldHelpLabel
                :label="t('config.concurrency')"
                :help="t('configHelp.concurrency')"
              />
              <InputText
                v-model.number="workflow.form.maxConcurrency"
                class="w-full min-w-0"
                type="number"
                min="1"
              />
            </label>

            <label class="grid min-w-0 gap-1.5">
              <FieldHelpLabel
                :label="t('config.timeoutSecs')"
                :help="t('configHelp.timeoutSecs')"
              />
              <InputText
                v-model.number="workflow.form.requestTimeoutSecs"
                class="w-full min-w-0"
                type="number"
                min="1"
              />
            </label>

            <div class="grid min-w-0 gap-2">
              <FieldHelpLabel
                :label="t('config.debugLogging')"
                :help="t('configHelp.debugLogging')"
              />
              <label class="flex items-center gap-2 rounded-xl border border-stone-200 px-3 py-2 text-sm text-stone-700">
                <input
                  v-model="workflow.form.debugLogging"
                  type="checkbox"
                >
                <span>{{ t("config.debugLogging") }}</span>
              </label>
            </div>
          </div>
        </Panel>

        <Panel
          v-model:collapsed="glossaryCollapsed"
          :header="t('config.promptGlossary')"
          toggleable
        >
          <div class="grid gap-3">
            <label class="grid min-w-0 gap-1.5">
              <FieldHelpLabel
                :label="t('config.gameProfile')"
                :help="t('configHelp.gameProfile')"
              />
              <select
                v-model="workflow.form.gameProfile"
                class="rp-select"
              >
                <option
                  v-for="profile in gameProfileOptions"
                  :key="profile"
                  :value="profile"
                >
                  {{ t(`gameProfile.${profile}`) }}
                </option>
              </select>
            </label>

            <label class="grid min-w-0 gap-1.5">
              <span class="text-xs font-semibold text-stone-500">{{ t("config.systemPrompt") }}</span>
              <Textarea
                v-model="workflow.form.systemPrompt"
                rows="3"
                auto-resize
              />
            </label>

            <div class="grid gap-3 md:grid-cols-2">
              <label class="grid min-w-0 gap-1.5">
                <span class="text-xs font-semibold text-stone-500">{{ t("config.glossaryPath") }}</span>
                <InputText
                  v-model="workflow.form.glossaryPath"
                  class="w-full min-w-0"
                  :placeholder="t('config.optional')"
                />
              </label>

              <label class="grid min-w-0 gap-1.5">
                <span class="text-xs font-semibold text-stone-500">{{ t("config.doNotTranslatePath") }}</span>
                <InputText
                  v-model="workflow.form.doNotTranslatePath"
                  class="w-full min-w-0"
                  :placeholder="t('config.optional')"
                />
              </label>
            </div>
          </div>
        </Panel>

        <div class="flex flex-wrap gap-2 pt-1">
          <Button
            :label="t('config.start')"
            class="rp-primary-button min-w-32"
            :disabled="!workflow.canRun"
            :loading="workflow.busy"
            @click="workflow.runMainWorkflow"
          />
          <Button
            :label="t('config.preview')"
            severity="secondary"
            outlined
            :disabled="!workflow.canPreview"
            @click="workflow.previewText"
          />
          <Button
            :label="t('config.estimateTokens')"
            severity="secondary"
            outlined
            :disabled="!workflow.canEstimate"
            @click="workflow.estimateTokens"
          />
          <Button
            :label="t('config.restore')"
            severity="danger"
            outlined
            :disabled="!workflow.canRestore"
            @click="workflow.restoreOriginal"
          />
          <Button
            v-if="workflow.cancellable"
            :label="t('config.cancel')"
            severity="danger"
            @click="workflow.cancelTranslationRequest"
          />
        </div>
      </div>
    </template>
  </Card>
</template>

<style scoped>
.rp-field-grid {
  display: grid;
  gap: 0.75rem;
  grid-template-columns: repeat(auto-fit, minmax(min(15rem, 100%), 1fr));
}

.rp-select {
  width: 100%;
  min-width: 0;
  border: 1px solid var(--p-inputtext-border-color);
  border-radius: var(--p-inputtext-border-radius);
  background: var(--p-inputtext-background);
  color: var(--p-inputtext-color);
  padding: 0.625rem 0.75rem;
}
</style>
