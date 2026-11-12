import { createApp } from "vue";
import "./styles/globals.css";

import App from "./App.vue";
import router from "./router";
import { initTheme } from "./modules/theme";

initTheme();

const app = createApp(App);
app.use(router);
app.mount("#app");
