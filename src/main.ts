import { logger } from './utils/logger';
logger.info("main.ts execution started");
import { createApp } from "vue";
import { createPinia } from 'pinia';
import router from './router';
import { i18n } from './i18n';
import "./style.css";
logger.info("main.ts imports done");
import type { Component } from "vue";
import { installGlobalErrorLogging } from "./utils/globalErrors";
import { watchPage } from "./shared/invariants/watchPage";

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

/**
 * Only the root this window shows is loaded.
 *
 * All three were imported here, so the quick windows loaded App.vue and
 * everything it imports — and ran what those modules do on import. The
 * browsing pane's module asks Rust for the pane's page and tells it the
 * sidebar's width, which only the main window may: every opening of a quick
 * window wrote two refusals to the log, 386 of each between 2026-10-08 and
 * -15 (found by `npm run audit`). It also made the window that has to appear
 * instantly load the whole app first.
 */
const root: Promise<{ default: Component }> = isQuickEntry
  ? import("./QuickEntry.vue")
  : isSafeQuick
    ? import("./SafeQuick.vue")
    : import("./App.vue");

root
  .then(({ default: Root }) => {
    const app = createApp(Root);
    // Before anything is installed or mounted, so an error in setup is caught too.
    installGlobalErrorLogging(app, isQuickEntry ? 'quick-entry' : isSafeQuick ? 'safe-quick' : 'main');
    app.use(createPinia());
    if (!isQuickEntry && !isSafeQuick) app.use(router);
    app.use(i18n);
    logger.info("main.ts app created, mounting...");
    app.mount("#app");
    logger.info("main.ts mount called");
    // The main window only: the quick windows open and close in a moment, and a
    // beat from them would be taken for the main page's. See `invariants`.
    if (!isQuickEntry && !isSafeQuick) watchPage(() => router.currentRoute.value.fullPath);
  })
  .catch((e) => logger.error("main.ts could not load the window's root", e));
