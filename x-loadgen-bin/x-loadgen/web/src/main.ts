import { createApp } from "vue";
import "./styles/globals.css";

import App from "./App.vue";
import router from "./router";
import { initActivityMonitor } from "./modules/activity";
import { initTheme } from "./modules/theme";

initTheme();
initActivityMonitor();

const app = createApp(App);
app.use(router);
app.mount("#app");
