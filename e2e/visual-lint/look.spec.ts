import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { dirname, extname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { LOOK_FRAME_STYLE } from '../../src/shared/syn/visualHost';
// @ts-expect-error — a plain ES module, shared with the eval.
import { assemble, ORIGIN, VISUAL } from '../visualTrial.mjs';

/**
 * A draft looked at the way the app looks at one (`SynVisualLooker.vue`,
 * `syn::visual::look`): in a sandboxed frame of the app's page, styled with
 * `LOOK_FRAME_STYLE`, linting itself once drawn. The engine waits 4 s.
 *
 * The second test is why the frame is in the window: WebKit gives a frame
 * moved off screen no animation frames, and the lint waits on two of them.
 */
const HERE = dirname(fileURLToPath(import.meta.url));
// What `look::draft_page` adds after a draft's fragment.
const draft = readFileSync(join(HERE, 'clean-loan-schedule.html'), 'utf8') + '\n<script>synabit.ready.then(() => synabit.lint({ controls: true, proof: [] }));</script>';

async function lookedAt(page: import('@playwright/test').Page, style: string, within: number) {
  await page.route('**/*', (route) => {
    const url = new URL(route.request().url());
    if (url.host === 'app.localhost') {
      return route.fulfill({
        contentType: 'text/html',
        body: `<!doctype html><body><script>
          const t0 = performance.now();
          window.addEventListener('message', (e) => {
            const m = e.data, frame = document.querySelector('iframe');
            if (m.method === 'ui/initialize') frame.contentWindow.postMessage({ jsonrpc: '2.0', id: m.id, result: { hostContext: { theme: 'light', locale: 'vi-VN' } } }, '*');
            if (m.method === 'synabit/lint') window.looked = { ms: performance.now() - t0, problems: m.params.problems };
          });
        </script><iframe sandbox="allow-scripts" style="${style}" src="${ORIGIN}/draft"></iframe></body>`,
      });
    }
    if (url.pathname === '/draft') return route.fulfill({ contentType: 'text/html', body: assemble(draft, 'light') });
    if (url.pathname === '/kit.js' || url.pathname === '/kit.css' || url.pathname.startsWith('/lib/')) {
      return route.fulfill({ contentType: extname(url.pathname) === '.css' ? 'text/css' : 'text/javascript', body: readFileSync(join(VISUAL, url.pathname)) });
    }
    return route.fulfill({ status: 404, body: '' });
  });
  await page.goto('http://app.localhost/');
  return page
    .waitForFunction(() => (window as unknown as { looked?: unknown }).looked, null, { timeout: within })
    .then((handle) => handle.jsonValue() as Promise<{ ms: number; problems: unknown[] }>)
    .catch(() => null);
}

test('a draft in the hidden frame is linted within the engine’s wait', async ({ page }) => {
  const looked = await lookedAt(page, LOOK_FRAME_STYLE, 4000);
  expect(looked, 'the lint came back').not.toBeNull();
  expect(looked!.problems).toEqual([]);
});

test('a frame moved off screen is never linted', async ({ page }) => {
  const looked = await lookedAt(page, 'position:fixed;left:-10000px;top:0;width:720px;height:900px;border:0;', 6000);
  expect(looked).toBeNull();
});
