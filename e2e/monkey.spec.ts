import { test, expect, type Page } from '@playwright/test';
import { openApp } from './app';

/**
 * The front end, used by nobody in particular: clicks, typing and keys at
 * random, in WebKit, against the backend in `fakeTauri.js`. After every step
 * the page must not have thrown, must still answer, and must not have
 * reported a breach of its own (`[invariant]` lines — `note.roundtrip`,
 * `ui.responsive` and the rest; see `contracts/invariants.json`).
 *
 *   npx playwright test e2e/monkey.spec.ts
 *   MONKEY_SEED=123 MONKEY_STEPS=400 npx playwright test e2e/monkey.spec.ts
 *
 * The seed is printed; the same seed takes the same steps. A failure lists
 * the steps that led to it.
 */

const SEED = Number(process.env.MONKEY_SEED ?? Math.floor(Math.random() * 1e9));
const STEPS = Number(process.env.MONKEY_STEPS ?? 150);
/** A page that has not answered in this long has stopped. */
const ALIVE_MS = 5_000;
/** Blocked longer than this in a test browser is a finding, not noise. */
const BLOCKED_MS = 2_000;

/** mulberry32: small, seeded, good enough to choose clicks with. */
function rng(seed: number) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const WORDS = ['Cam', 'golive', 'CMC', 'nhiệt độ', 'đã lưu', '**đậm**', '# tiêu đề', '- mục', '1. một', '> trích', '`code`', '$5', '&amp;', '<b>', '|', '[[2026-10-16]]', '@Minh', '/', ':', 'https://example.com/a?x=1', '🙂', '\\', '```'];
const KEYS = ['Enter', 'Escape', 'Tab', 'Backspace', 'ArrowDown', 'ArrowUp', 'Shift+Enter', 'Meta+z', 'Meta+Shift+z', 'Meta+b', 'Meta+a', 'Delete'];
const CLICKABLE = 'button, a, [role="button"], [role="tab"], [role="menuitem"], [role="option"], input[type="checkbox"], summary, [tabindex="0"]';
const TYPABLE = 'input:not([type="checkbox"]):not([type="file"]), textarea, [contenteditable="true"]';

/** Visible, enabled elements matching `selector`, as handles. */
async function candidates(page: Page, selector: string) {
  return page.locator(selector).evaluateAll((els) =>
    els
      .map((el, i) => {
        const r = (el as HTMLElement).getBoundingClientRect();
        const style = getComputedStyle(el);
        const ok = r.width > 2 && r.height > 2 && r.bottom > 0 && r.right > 0 && r.top < innerHeight && r.left < innerWidth &&
          style.visibility !== 'hidden' && style.pointerEvents !== 'none' && !(el as HTMLButtonElement).disabled;
        return ok ? { i, label: `${el.tagName.toLowerCase()}${el.getAttribute('aria-label') ? `[${el.getAttribute('aria-label')}]` : ''}${(el.textContent || '').trim().slice(0, 24) ? ` "${(el.textContent || '').trim().slice(0, 24)}"` : ''}` } : null;
      })
      .filter(Boolean) as { i: number; label: string }[],
  );
}

test(`monkey (seed ${SEED})`, async ({ page }) => {
  const seen = await openApp(page);

  const random = rng(SEED);
  const pick = <T>(xs: T[]) => xs[Math.floor(random() * xs.length)];
  const steps: string[] = [];
  const fail = (what: string) => `${what}\n\nseed ${SEED}; the last steps:\n${steps.slice(-20).map((s, i) => `  ${steps.length - Math.min(20, steps.length) + i + 1}. ${s}`).join('\n')}`;

  for (let n = 0; n < STEPS; n++) {
    const roll = random();
    try {
      if (roll < 0.55) {
        const found = await candidates(page, CLICKABLE);
        if (found.length) {
          const c = pick(found);
          steps.push(`click ${c.label}`);
          await page.locator(CLICKABLE).nth(c.i).click({ timeout: 2000, noWaitAfter: true });
        }
      } else if (roll < 0.8) {
        const found = await candidates(page, TYPABLE);
        if (found.length) {
          const c = pick(found);
          const text = Array.from({ length: 1 + Math.floor(random() * 4) }, () => pick(WORDS)).join(' ');
          steps.push(`type into ${c.label}: ${JSON.stringify(text)}`);
          const target = page.locator(TYPABLE).nth(c.i);
          await target.click({ timeout: 2000, noWaitAfter: true });
          await page.keyboard.type(text, { delay: 5 });
        }
      } else if (roll < 0.95) {
        const key = pick(KEYS);
        steps.push(`press ${key}`);
        await page.keyboard.press(key);
      } else {
        steps.push('scroll');
        await page.mouse.wheel(0, (random() - 0.5) * 1200);
      }
    } catch (e) {
      // An element that moved or went away between choosing and clicking is
      // the page changing under the monkey, not a fault.
      steps.push(`  (skipped: ${String(e).split('\n')[0].slice(0, 80)})`);
    }
    await page.waitForTimeout(60);
    // Backspace outside a field is "back" in a test browser, and back from the
    // app's first page leaves it. A Tauri window has no page before the app,
    // so this is the browser's, not a fault: note it and open the app again.
    if (!page.url().startsWith('http://localhost')) {
      steps.push(`  (left the app for ${page.url()}; opened it again)`);
      await page.goto('/');
      await page.waitForTimeout(1500);
    }
    if (process.env.MONKEY_TRACE) {
      const shown = await page.evaluate(() => document.querySelectorAll('button, a, input, textarea, [contenteditable]').length).catch(() => -1);
      console.log(`${n + 1}. ${steps[steps.length - 1] ?? '(nothing to do)'} — ${shown} controls on screen, at ${page.url().replace(/^https?:\/\/[^/]+/, '')}`);
    }

    // ── The invariants, after every step ──
    const alive = await Promise.race([
      page.evaluate(() => true).catch(() => false),
      new Promise<boolean>((r) => setTimeout(() => r(false), ALIVE_MS)),
    ]);
    expect(alive, fail(`ui.page-alive: the page did not answer within ${ALIVE_MS} ms`)).toBe(true);
    expect(seen.thrown, fail(`ui.no-uncaught-error: ${seen.thrown[0]}`)).toEqual([]);
    // Vue catches an error in a render or a hook and the app logs it as
    // "Uncaught (vue)": thrown all the same, and the screen it was drawing
    // is wrong or empty.
    const uncaught = seen.consoleErrors.filter((e) => e.includes('Uncaught ('));
    expect(uncaught, fail(`ui.no-uncaught-error: ${uncaught[0]}`)).toEqual([]);
    const reported = await page.evaluate(() => (window as any).__fake.violations.splice(0));
    const serious = reported.filter((v: any) => v.invariant !== 'ui.responsive' || (v.detail?.blocked_ms ?? 0) > BLOCKED_MS);
    expect(serious, fail(`the app reported: ${JSON.stringify(serious[0])}`)).toEqual([]);
  }

  const unknown = await page.evaluate(() => (window as any).__fake.unknown);
  const acted = steps.filter((s) => !s.startsWith('  (skipped')).length;
  await page.screenshot({ path: `test-results/monkey-${SEED}-end.png` });
  console.log(
    `\nseed ${SEED}: ${STEPS} steps (${acted} acted on something), no breach.\n` +
      `console errors (the app's own catch-and-log, often the fake backend's thin answers): ${seen.consoleErrors.length}\n` +
      [...new Set(seen.consoleErrors.map((e) => e.slice(0, 140)))].slice(0, 12).map((e) => `  - ${e}`).join('\n') +
      `\ncommands the fake has no answer for: ${Object.keys(unknown).length}\n  ${Object.keys(unknown).slice(0, 40).join(', ')}`,
  );
});
