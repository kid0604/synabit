import type { Page } from '@playwright/test';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

/**
 * The app in a browser page, against the backend in `fakeTauri.js`, kept in
 * the page and away from the internet. What it throws is collected in `thrown`.
 */
const fakeBackend = readFileSync(new URL('./fakeTauri.js', import.meta.url), 'utf8');

/**
 * What each command answers, as far as the front end's own types say:
 * `invoke<Note[]>('get_notes')` answers an array. The fake answers an empty
 * one of the right kind for any command it has no answer of its own for, so
 * a screen draws with nothing in it rather than throwing on a `null` the real
 * backend never sends — which would be the fake's fault, not the app's.
 */
function commandShapes(): Record<string, string> {
  const shapes: Record<string, string> = {};
  const walk = (dir: string) => {
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      const at = join(dir, e.name);
      if (e.isDirectory()) {
        if (e.name !== '__tests__' && e.name !== 'node_modules') walk(at);
      } else if (/\.(ts|vue)$/.test(e.name) && !e.name.endsWith('.spec.ts')) {
        for (const m of readFileSync(at, 'utf8').matchAll(/invoke<([^(]{1,160}?)>\(\s*['"]([a-z_:|]+)['"]/g)) {
          const type = m[1].trim().replace(/\s*\|\s*(null|undefined)\s*$/, '');
          const kind = /\[\]$|^Array</.test(type) ? 'array'
            : /^string$/.test(type) ? 'string'
            : /^number$/.test(type) ? 'number'
            : /^boolean$/.test(type) ? 'boolean'
            : /^(void|unknown|any)$/.test(type) ? ''
            : 'object';
          if (kind && !shapes[m[2]]) shapes[m[2]] = kind;
        }
      }
    }
  };
  walk(new URL('../src', import.meta.url).pathname);
  return shapes;
}
const shapes = commandShapes();

/** What the browser says that is not the app's doing. */
export const NOT_THE_APP = [/Notification prompting can only be done from a user gesture/];

export interface Seen {
  thrown: string[];
  consoleErrors: string[];
}

export async function openApp(page: Page, at = '/'): Promise<Seen> {
  const seen: Seen = { thrown: [], consoleErrors: [] };
  page.on('pageerror', (e) => {
    if (!NOT_THE_APP.some((re) => re.test(e.message))) seen.thrown.push(e.message);
  });
  page.on('console', (m) => {
    if (m.type() === 'error' && !NOT_THE_APP.some((re) => re.test(m.text()))) seen.consoleErrors.push(m.text().slice(0, 300));
  });
  await page.route(/^(?!http:\/\/localhost)/, (route) => route.abort());
  await page.addInitScript(() => {
    window.open = () => null;
  });
  await page.addInitScript((given) => {
    (window as any).__shapes = given;
  }, shapes);
  await page.addInitScript(fakeBackend);
  await page.goto(at);
  await page.waitForTimeout(2500);
  return seen;
}
