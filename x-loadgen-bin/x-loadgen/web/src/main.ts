import { createApp } from "vue";
import "@hoppscotch/ui/themes.css";
import "./assets/themes/themes.scss";
import "./assets/scss/tailwind.scss";
import "./assets/scss/styles.scss";

import App from "./App.vue";
import router from "./router";
import { initHoppscotchTheming } from "./modules/theming";
import { registerHoppscotchTippy } from "./modules/tippy";
import { registerHoppscotchUI } from "./modules/ui";

initHoppscotchTheming();

const app = createApp(App);
registerHoppscotchTippy(app);
registerHoppscotchUI(app);
app.use(router);
app.mount("#app");
