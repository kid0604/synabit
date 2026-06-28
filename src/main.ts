import { logger } from './utils/logger';
logger.info("main.ts execution started");
import { createApp } from "vue";
import { createPinia } from 'pinia';
import router from './router';
import { i18n } from './i18n';
import "./style.css";
logger.info("main.ts imports done");
import App from "./App.vue";

const app = createApp(App);
app.use(createPinia());
app.use(router);
app.use(i18n);
logger.info("main.ts app created, mounting...");
app.mount("#app");
logger.info("main.ts mount called");
