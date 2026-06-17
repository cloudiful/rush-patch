<script setup lang="ts">
defineProps<{
  help: string;
  label: string;
}>();

const mdiInformationOutline =
  "M11 17H13V11H11V17M12 2A10 10 0 1 0 12 22A10 10 0 0 0 12 2M12 20A8 8 0 1 1 12 4A8 8 0 0 1 12 20M11 9H13V7H11V9Z";
</script>

<template>
  <span class="inline-flex min-w-0 items-center gap-1.5 text-xs font-semibold text-stone-500">
    <span class="truncate">{{ label }}</span>
    <span
      v-tooltip.top="{ value: help, showDelay: 150 }"
      :aria-label="help"
      class="rp-help-icon"
      role="img"
      tabindex="0"
    >
      <svg
        class="h-4 w-4"
        viewBox="0 0 24 24"
        aria-hidden="true"
      >
        <path
          :d="mdiInformationOutline"
          fill="currentColor"
        />
      </svg>
    </span>
  </span>
</template>

<style scoped>
.rp-help-icon {
  align-items: center;
  border-radius: 999px;
  color: rgb(100 116 139);
  cursor: help;
  display: inline-flex;
  flex: none;
  height: 1.25rem;
  justify-content: center;
  outline: none;
  transition:
    background-color 160ms ease,
    color 160ms ease;
  width: 1.25rem;
}

.rp-help-icon:hover,
.rp-help-icon:focus-visible {
  background: rgb(20 184 166 / 0.12);
  color: rgb(15 118 110);
}
</style>
