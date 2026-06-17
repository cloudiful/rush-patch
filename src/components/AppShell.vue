<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { RouterView, useRoute, useRouter } from "vue-router";
import Button from "primevue/button";
import Menu from "primevue/menu";
import { useI18n } from "vue-i18n";
import { useWorkflowStore } from "../stores/workflow";

const SIDEBAR_AUTO_COLLAPSE_WIDTH = 1120;

const route = useRoute();
const router = useRouter();
const workflow = useWorkflowStore();
const sidebarCollapsed = ref(false);
const { t } = useI18n();

const mdiMenu = "M3 6H21V8H3V6M3 11H21V13H3V11M3 16H21V18H3V16Z";
const mdiTranslate =
  "M12.87 15.07L10.33 12.56L10.36 12.53C12.1 10.59 13.34 8.36 14.07 6H17V4H10V2H8V4H1V6H12.17C11.5 7.92 10.44 9.75 9 11.35C8.07 10.32 7.3 9.19 6.69 8H4.69C5.42 9.63 6.42 11.17 7.67 12.56L2.58 17.58L4 19L9 14L12.11 17.11L12.87 15.07M18.5 10H16.5L12 22H14L15.12 19H19.87L21 22H23L18.5 10M15.88 17L17.5 12.67L19.12 17H15.88Z";
const mdiCog =
  "M12 15.5A3.5 3.5 0 1 0 12 8.5A3.5 3.5 0 0 0 12 15.5M19.43 12.98C19.47 12.66 19.5 12.34 19.5 12C19.5 11.66 19.47 11.34 19.43 11.02L21.54 9.37C21.73 9.22 21.78 8.95 21.66 8.73L19.66 5.27C19.54 5.05 19.29 4.96 19.06 5.05L16.57 6.05C16.04 5.65 15.5 5.32 14.87 5.07L14.5 2.42C14.46 2.18 14.25 2 14 2H10C9.75 2 9.54 2.18 9.5 2.42L9.13 5.07C8.5 5.32 7.96 5.66 7.43 6.05L4.94 5.05C4.71 4.96 4.46 5.05 4.34 5.27L2.34 8.73C2.21 8.95 2.27 9.22 2.46 9.37L4.57 11.02C4.53 11.34 4.5 11.67 4.5 12C4.5 12.33 4.53 12.66 4.57 12.98L2.46 14.63C2.27 14.78 2.22 15.05 2.34 15.27L4.34 18.73C4.46 18.95 4.71 19.04 4.94 18.95L7.43 17.95C7.96 18.35 8.5 18.68 9.13 18.93L9.5 21.58C9.54 21.82 9.75 22 10 22H14C14.25 22 14.46 21.82 14.5 21.58L14.87 18.93C15.5 18.68 16.04 18.34 16.57 17.95L19.06 18.95C19.29 19.04 19.54 18.95 19.66 18.73L21.66 15.27C21.78 15.05 21.73 14.78 21.54 14.63L19.43 12.98Z";
const mdiTextBoxSearch =
  "M3 3H21V5H3V3M3 7H21V9H3V7M3 11H14V13H3V11M16.5 12A4.5 4.5 0 1 1 12 16.5A4.5 4.5 0 0 1 16.5 12M16.5 9A7.5 7.5 0 1 0 21.8 11.2L24 13.4L22.6 14.8L20.4 12.6A7.47 7.47 0 0 0 16.5 9Z";

const menuItems = computed(() => [
  { label: t("nav.translateWorkbench"), iconPath: mdiTranslate, path: "/" },
  { label: t("logs.title"), iconPath: mdiTextBoxSearch, path: "/logs" },
  { label: t("nav.settings"), iconPath: mdiCog, path: "/settings" },
]);

function isActive(path: string) {
  return path === "/" ? route.path === "/" : route.path.startsWith(path);
}

function navigate(path: string) {
  if (route.path !== path) void router.push(path);
}

function syncAutoSidebarCollapse() {
  sidebarCollapsed.value = window.innerWidth < SIDEBAR_AUTO_COLLAPSE_WIDTH;
}

function toggleSidebar() {
  sidebarCollapsed.value = !sidebarCollapsed.value;
}

onMounted(() => {
  syncAutoSidebarCollapse();
  window.addEventListener("resize", syncAutoSidebarCollapse);
  void workflow.loadPersistedSettings();
});

onBeforeUnmount(() => {
  window.removeEventListener("resize", syncAutoSidebarCollapse);
});
</script>

<template>
  <main class="h-screen overflow-hidden bg-[#f4efe6] text-slate-900">
    <div
      class="mx-auto grid h-full max-w-[1500px] gap-0 transition-[grid-template-columns] duration-200"
      :class="sidebarCollapsed ? 'grid-cols-[72px_minmax(0,1fr)]' : 'grid-cols-[260px_minmax(0,1fr)]'"
    >
      <aside
        class="h-full overflow-hidden border-r border-stone-200 bg-[#fcfaf5] py-5 transition-[padding] duration-200"
        :class="sidebarCollapsed ? 'px-2' : 'px-5'"
      >
        <div
          class="mb-5 flex items-center gap-3"
          :class="sidebarCollapsed ? 'justify-center' : 'justify-between'"
        >
          <h1
            v-if="!sidebarCollapsed"
            class="min-w-0 text-2xl font-black tracking-tight"
          >
            {{ t("app.name") }}
          </h1>

          <Button
            :aria-label="sidebarCollapsed ? t('nav.expandSidebar') : t('nav.collapseSidebar')"
            severity="secondary"
            text
            rounded
            @click="toggleSidebar"
          >
            <svg
              class="h-5 w-5"
              viewBox="0 0 24 24"
              aria-hidden="true"
            >
              <path
                :d="mdiMenu"
                fill="currentColor"
              />
            </svg>
          </Button>
        </div>

        <Menu
          v-if="!sidebarCollapsed"
          :model="menuItems"
          class="rp-sidebar-menu w-full border-0 bg-transparent"
        >
          <template #item="{ item, props }">
            <a
              v-bind="props.action"
              :title="item.label"
              class="rp-sidebar-link"
              :class="{ 'rp-sidebar-link-active': isActive(item.path) }"
              @click="navigate(item.path)"
            >
              <svg
                class="h-5 w-5 shrink-0"
                viewBox="0 0 24 24"
                aria-hidden="true"
              >
                <path
                  :d="item.iconPath"
                  fill="currentColor"
                />
              </svg>
              <span class="truncate">
                {{ item.label }}
              </span>
            </a>
          </template>
        </Menu>

        <nav
          v-else
          class="grid gap-2"
          :aria-label="t('nav.sidebar')"
        >
          <Button
            v-for="item in menuItems"
            :key="item.label"
            :aria-label="item.label"
            :title="item.label"
            class="rp-sidebar-icon-button"
            :class="{ 'rp-sidebar-icon-button-active': isActive(item.path) }"
            severity="secondary"
            text
            rounded
            @click="navigate(item.path)"
          >
            <svg
              class="h-5 w-5"
              viewBox="0 0 24 24"
              aria-hidden="true"
            >
              <path
                :d="item.iconPath"
                fill="currentColor"
              />
            </svg>
          </Button>
        </nav>
      </aside>

      <section class="h-full min-w-0 overflow-y-auto px-4 py-5 lg:px-6">
        <RouterView />
      </section>
    </div>
  </main>
</template>

<style scoped>
.rp-sidebar-link {
  align-items: center;
  border-radius: 0.9rem;
  box-sizing: border-box;
  color: rgb(51 65 85);
  display: flex;
  font-size: 0.95rem;
  font-weight: 700;
  gap: 0.65rem;
  max-width: 100%;
  min-height: 2.55rem;
  min-width: 0;
  padding: 0.65rem 0.85rem;
  text-decoration: none;
  transition:
    background-color 160ms ease,
    color 160ms ease;
}

.rp-sidebar-menu {
  max-width: 100%;
  min-width: 0 !important;
  overflow: hidden !important;
  width: 100%;
}

.rp-sidebar-menu :deep(.p-menu-list),
.rp-sidebar-menu :deep(.p-menu-item),
.rp-sidebar-menu :deep(.p-menu-item-content) {
  max-width: 100%;
  min-width: 0;
  overflow: hidden;
  width: 100%;
}

.rp-sidebar-menu :deep(.p-menu-item-link) {
  max-width: 100%;
  width: 100%;
}

.rp-sidebar-link:hover,
.rp-sidebar-link-active {
  background: rgb(20 184 166 / 0.11);
  color: rgb(15 118 110);
}

.rp-sidebar-icon-button.p-button {
  color: rgb(71 85 105);
  height: 2.75rem;
  width: 100%;
}

.rp-sidebar-icon-button.p-button:not(:disabled):hover,
.rp-sidebar-icon-button-active.p-button {
  background: rgb(20 184 166 / 0.11);
  color: rgb(15 118 110);
}
</style>
