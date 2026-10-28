import { defineConfig } from '@playwright/test';

/**
 * Syn's visuals drawn in WebKit and looked at by `synabit.lint`
 * (`e2e/visual-lint/`). No dev server: each visual is served the way the app's
 * `synvisual` frame serves it, by `e2e/visualTrial.mjs`.
 */
export default defineConfig({
  testDir: 'e2e/visual-lint',
  timeout: 5 * 60_000,
  workers: 1,
  reporter: [['list']],
});
