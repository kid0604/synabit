import { describe, it, expect, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
import contract from '../../../../contracts/invariants.json';

vi.mock('../../../utils/logger', () => ({ logger: { warn: vi.fn(), info: vi.fn(), error: vi.fn() } }));
import { claimsAction, parseViolation, INVARIANTS } from '../index';

declare const __dirname: string;
const ROOT = resolve(__dirname, '../../../..');

function sources(dir: string, ext: RegExp): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    if (entry.name === 'node_modules' || entry.name === 'target' || entry.name.startsWith('.')) return [];
    const at = join(dir, entry.name);
    if (entry.isDirectory()) return sources(at, ext);
    return ext.test(entry.name) && !/\.spec\.ts$/.test(entry.name) ? [at] : [];
  });
}

describe('the invariants contract', () => {
  it('judges every claim case as the contract says — as Rust does over the same cases', () => {
    for (const c of contract.claims.cases) expect(claimsAction(c.answer), c.answer).toBe(c.claims);
  });

  it('names every rule a breach is reported under, front end and Rust alike', () => {
    const known = new Set<string>(INVARIANTS.map((r) => r.id));
    const used = new Set<string>();
    for (const f of sources(join(ROOT, 'src'), /\.(ts|vue)$/)) {
      for (const m of readFileSync(f, 'utf8').matchAll(/invariant: '([a-z.-]+)'/g)) used.add(m[1]);
    }
    for (const f of sources(join(ROOT, 'src-tauri/src'), /\.rs$/)) {
      for (const m of readFileSync(f, 'utf8').matchAll(/invariants::report\(\s*"([a-z.-]+)"/g)) used.add(m[1]);
      for (const m of readFileSync(f, 'utf8').matchAll(/\breport\(\s*"([a-z.-]+)"/g)) used.add(m[1]);
    }
    expect(used.size).toBeGreaterThan(3);
    for (const id of used) expect(known.has(id), `${id} is not in contracts/invariants.json`).toBe(true);
  });

  it('reads a breach back out of the line it was written as', () => {
    const line = '[2026-10-15][15:13:47][webview][WARN] [invariant] {"invariant":"ui.responsive","detail":{"blocked_ms":900}}';
    expect(parseViolation(line)).toEqual({ invariant: 'ui.responsive', detail: { blocked_ms: 900 } });
    expect(parseViolation('[2026-10-15][15:13:47][webview][WARN] something else')).toBeNull();
  });
});
