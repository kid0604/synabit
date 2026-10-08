import type { App } from 'vue';
import { logger } from './logger';
import { errorText } from '../shared/errorText';

/**
 * Send the errors nobody caught to the log file, not only to DevTools.
 *
 * A thrown error in a component, or a promise rejected with no `.catch`, went
 * to the WebView's console and nowhere else. On a phone, or in a release build
 * on the desktop, nobody has that console open — so the failure a bug report
 * needed was the one thing the exported diagnostics did not contain.
 *
 * Three ways in, one report each:
 *
 * - **Vue's `errorHandler`** — render, watchers, lifecycle hooks, and event
 *   handlers, including the promises they return. Vue catches these itself,
 *   so they never reach the two below.
 * - **`unhandledrejection`** — a promise rejected outside Vue with nobody
 *   listening: a `void someCommand()` in a timer, a listener callback.
 * - **`error`** — a synchronous throw outside Vue.
 *
 * The same error object is reported once even if it somehow arrives twice,
 * and a run of the same message (a render that throws every frame) is logged
 * once with a count rather than filling the log.
 *
 * `where` says which root this is — the main window, the quick-entry box or
 * Safe's Quick Access — because they share one log file.
 *
 * Returns what takes the window listeners off again, for tests.
 */
export function installGlobalErrorLogging(app: App, where: string): () => void {
  const seen = new WeakSet<object>();
  let last = '';
  let repeats = 0;

  const report = (kind: string, error: unknown, detail?: string) => {
    try {
      if (error && typeof error === 'object') {
        if (seen.has(error)) return;
        seen.add(error);
      }
      const head = `[${where}] Uncaught (${kind})${detail ? ` in ${detail}` : ''}:`;
      const key = `${head} ${errorText(error)}`;
      if (key === last) {
        repeats++;
        return;
      }
      if (repeats > 0) logger.warn(`[${where}] The previous error repeated ${repeats} more time(s).`);
      last = key;
      repeats = 0;
      logger.error(head, error);
    } catch {
      // Reporting must never be what throws next: that would land back here.
    }
  };

  app.config.errorHandler = (error, _instance, info) => report('vue', error, info);

  const onRejection = (event: PromiseRejectionEvent) => report('promise', event.reason);
  // A failed <img> or <script> load fires `error` too, without an Error in it;
  // those are not exceptions, and only reach a window listener that captures,
  // which this does not.
  const onError = (event: ErrorEvent) =>
    report('error', event.error ?? event.message, event.filename ? `${event.filename}:${event.lineno}` : undefined);
  window.addEventListener('unhandledrejection', onRejection);
  window.addEventListener('error', onError);
  return () => {
    window.removeEventListener('unhandledrejection', onRejection);
    window.removeEventListener('error', onError);
  };
}
