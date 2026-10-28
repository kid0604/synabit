import { defineConfig, devices } from '@playwright/test';

/**
 * The front end in a browser, against a backend in the page (`e2e/fakeTauri.js`).
 * WebKit, the engine macOS's WKWebView is; the app's own dev server on a port
 * of its own, so a running `tauri dev` is left alone.
 */
const PORT = 1431;

export default defineConfig({
  testDir: 'e2e',
  // Visuals have their own config, with no dev server: `npm run visual-lint`.
  testIgnore: 'visual-lint/**',
  timeout: 10 * 60_000,
  workers: 1,
  reporter: [['list']],
  use: {
    baseURL: `http://localhost:${PORT}`,
    viewport: { width: 1280, height: 800 },
    trace: 'retain-on-failure',
  },
  projects: [{ name: 'webkit', use: { ...devices['Desktop Safari'] } }],
  webServer: {
    command: `npx vite --port ${PORT} --strictPort`,
    url: `http://localhost:${PORT}`,
    reuseExistingServer: true,
    timeout: 120_000,
  },
});
