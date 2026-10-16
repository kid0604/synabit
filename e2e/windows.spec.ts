import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { openApp } from './app';

/**
 * A quick window calls only what Rust lets it (`app_shell::gate`).
 *
 * Rust's own test reads each window's file for the commands it names; what
 * the modules it imports do on import is not in that file. Both quick windows
 * loaded App.vue that way, and the browsing pane's module asked for the pane
 * from them: 386 refusals each of `syn_pane_room` and `syn_pane_page` in a
 * week's log. This watches what the window actually calls.
 */
const gate = readFileSync(new URL('../src-tauri/src/app_shell/gate.rs', import.meta.url), 'utf8');
const allowed = (name: string) => {
  const m = new RegExp(`pub const ${name}: &\\[&str\\] = &\\[([^\\]]*)\\]`).exec(gate);
  if (!m) throw new Error(`${name} not found in gate.rs`);
  return [...m[1].matchAll(/"([^"]+)"/g)].map((x) => x[1]);
};

for (const [hash, list] of [['#/quick-entry', 'QUICK_ENTRY_MAY'], ['#/safe-quick', 'SAFE_QUICK_MAY']] as const) {
  test(`${hash} calls nothing it may not`, async ({ page }) => {
    const seen = await openApp(page, `/${hash}`);
    const calls: string[] = await page.evaluate(() => (window as any).__fake.calls);
    // Plugins are reached through Tauri's capabilities, not this gate.
    const refused = [...new Set(calls.filter((c) => !c.startsWith('plugin:') && !allowed(list).includes(c)))];
    expect(refused, `${hash} called what ${list} does not allow`).toEqual([]);
    expect(seen.thrown).toEqual([]);
  });
}
