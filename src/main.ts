import { logger } from './utils/logger';
logger.info("main.ts execution started");
import { createApp } from "vue";
import { createPinia } from 'pinia';
import router from './router';
import { i18n } from './i18n';
import "./style.css";
logger.info("main.ts imports done");
import App from "./App.vue";
import QuickEntry from "./QuickEntry.vue";
import SafeQuick from "./SafeQuick.vue";

/**
 * The quick-entry window mounts a different root.
 *
 * It is opened by the global hotkey and has to appear instantly over whatever
 * the user is doing, so it must not run App.vue's setup — vault loading,
 * sync, watchers, every mini-app's route. It needs a textarea and one IPC
 * call, and mounting the shell would make it as slow as opening the app,
 * which is the thing it exists to avoid.
 */
const isQuickEntry = window.location.hash.startsWith("#/quick-entry");
/** Safe's Quick Access window, for the same reason. */
const isSafeQuick = window.location.hash.startsWith("#/safe-quick");

/**
 * Where every toast goes: undo bars and the shell's notices stack here, one
 * above the other, instead of each fixing itself to the same spot and covering
 * the last. Made before mount so a toast's `Teleport` always finds it; see
 * `.app-toasts` in style.css for where it sits.
 */
const toasts = document.createElement('div');
toasts.id = 'app-toasts';
toasts.className = 'app-toasts';
document.body.appendChild(toasts);

const app = createApp(isQuickEntry ? QuickEntry : isSafeQuick ? SafeQuick : App);
app.use(createPinia());
if (!isQuickEntry && !isSafeQuick) app.use(router);
app.use(i18n);
logger.info("main.ts app created, mounting...");
app.mount("#app");
logger.info("main.ts mount called");
