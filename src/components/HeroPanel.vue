<script setup lang="ts">
import Tag from "primevue/tag";
import { useI18n } from "vue-i18n";

const { t } = useI18n();
</script>

<template>
  <div class="flex flex-col gap-5 lg:flex-row lg:items-end lg:justify-between">
    <div class="space-y-3">
      <Tag
        :value="t('hero.tag')"
        severity="contrast"
      />
      <h1 class="text-4xl font-black tracking-tight text-stone-950">
        {{ t("hero.title") }}
      </h1>
      <p class="max-w-2xl text-sm leading-6 text-stone-600">
        {{ t("hero.body") }}
      </p>
    </div>

    <div class="rounded-3xl bg-stone-950 px-5 py-4 text-sm text-amber-50">
      <p class="font-semibold">
        {{ t("hero.transaction") }}
      </p>
      <p class="mt-1 text-amber-100/80">
        {{ t("hero.flow") }}
      </p>
    </div>
  </div>
</template>
