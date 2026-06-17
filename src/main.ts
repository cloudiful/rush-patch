import { createApp } from "vue";
import PrimeVue from "primevue/config";
import Tooltip from "primevue/tooltip";
import Aura from "@primeuix/themes/aura";
import { createPinia } from "pinia";

import App from "./App.vue";
import { i18n } from "./i18n";
import { router } from "./router";
import "./styles.css";

const app = createApp(App);

app.use(PrimeVue, {
  theme: {
    preset: Aura,
    options: {
      darkModeSelector: false,
    },
  },
});
app.use(createPinia());
app.use(i18n);
app.use(router);
app.directive("tooltip", Tooltip);

app.mount("#app");
