<script setup lang="ts">
import { computed } from "vue";
import Button from "primevue/button";
import Card from "primevue/card";
import InputText from "primevue/inputtext";
import Message from "primevue/message";
import Select from "primevue/select";
import { useI18n } from "vue-i18n";
import { localeOptions, setLocale, type LocaleCode } from "../i18n";
import { useWorkflowStore } from "../stores/workflow";

const workflow = useWorkflowStore();
const { locale, t } = useI18n();

const currentLocale = computed<LocaleCode>({
  get: () => locale.value as LocaleCode,
  set: (value) => setLocale(value),
});

const apiEndpointOptions = computed(() => [
  { label: t("endpoint.responses"), value: "responses" },
  { label: t("endpoint.chatCompletions"), value: "chat_completions" },
]);
</script>

<template>
  <div class="mx-auto grid max-w-3xl gap-5">
    <Card class="border border-stone-200 shadow-none">
      <template #title>
        {{ t("settings.title") }}
      </template>

      <template #content>
        <div class="grid gap-5">
          <section class="grid gap-3">
            <h2 class="text-sm font-black uppercase tracking-[0.2em] text-stone-500">
              {{ t("settings.general") }}
            </h2>
            <label class="grid min-w-0 gap-1.5">
              <span class="text-xs font-semibold text-stone-500">{{ t("app.language") }}</span>
              <Select
                v-model="currentLocale"
                :options="localeOptions"
                option-label="label"
                option-value="value"
                class="w-full min-w-0"
              />
            </label>
          </section>

          <section class="grid gap-3">
            <h2 class="text-sm font-black uppercase tracking-[0.2em] text-stone-500">
              {{ t("config.openAiConnection") }}
            </h2>

            <div class="rp-field-grid">
              <label class="grid min-w-0 gap-1.5">
                <span class="text-xs font-semibold text-stone-500">{{ t("config.apiKey") }}</span>
                <InputText
                  v-model="workflow.form.apiKey"
                  type="password"
                  placeholder="sk-..."
                  class="w-full min-w-0"
                />
              </label>

              <label class="grid min-w-0 gap-1.5">
                <span class="text-xs font-semibold text-stone-500">{{ t("config.baseUrl") }}</span>
                <InputText
                  v-model="workflow.form.baseUrl"
                  class="w-full min-w-0"
                  :placeholder="t('config.optional')"
                />
              </label>

              <label class="grid min-w-0 gap-1.5">
                <span class="text-xs font-semibold text-stone-500">{{ t("config.apiEndpoint") }}</span>
                <Select
                  v-model="workflow.form.apiEndpoint"
                  :options="apiEndpointOptions"
                  option-label="label"
                  option-value="value"
                  class="w-full min-w-0"
                />
              </label>
            </div>
          </section>

          <div class="flex flex-wrap items-center gap-3">
            <Button
              :label="workflow.settingsSaving ? t('settings.saving') : t('settings.save')"
              severity="secondary"
              outlined
              :loading="workflow.settingsSaving"
              @click="workflow.savePersistedSettings"
            />
            <span
              v-if="!workflow.settingsError"
              class="text-sm font-semibold text-stone-500"
            >
              {{ t("settings.autoSave") }}
            </span>
          </div>

          <Message
            v-if="workflow.settingsError"
            severity="error"
            :closable="false"
          >
            {{ workflow.settingsError }}
          </Message>
        </div>
      </template>
    </Card>
  </div>
</template>

<style scoped>
.rp-field-grid {
  display: grid;
  gap: 0.75rem;
  grid-template-columns: repeat(auto-fit, minmax(min(15rem, 100%), 1fr));
}
</style>
