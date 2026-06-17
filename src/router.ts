import { createRouter, createWebHashHistory } from "vue-router";
import AppShell from "./components/AppShell.vue";
import LogsPage from "./pages/LogsPage.vue";
import SettingsPage from "./pages/SettingsPage.vue";
import TranslateWorkbench from "./pages/TranslateWorkbench.vue";

export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    {
      path: "/",
      component: AppShell,
      children: [
        {
          path: "",
          name: "translate-workbench",
          component: TranslateWorkbench,
        },
        {
          path: "settings",
          name: "settings",
          component: SettingsPage,
        },
        {
          path: "logs",
          name: "logs",
          component: LogsPage,
        },
      ],
    },
  ],
});
