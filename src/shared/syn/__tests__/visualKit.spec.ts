/**
 * The kit a Syn visual runs with (`src-tauri/src/syn/visual/kit.js`), run
 * here as a page would run it: standalone, with no host to answer.
 */
import { describe, it, expect, beforeAll } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

declare const __dirname: string;

const KIT = readFileSync(resolve(__dirname, '../../../../src-tauri/src/syn/visual/kit.js'), 'utf8');

interface Kit {
  number(value: number, digits?: number): string;
  expect(name: string, actual: unknown, expected: unknown, tolerance?: number): boolean;
}

let kit: Kit;
beforeAll(() => {
  document.documentElement.lang = 'vi-VN';
  new Function(KIT)();
  kit = (window as unknown as { synabit: Kit }).synabit;
});

describe("the kit's numbers", () => {
  it('writes them the way the locale does, rounded', () => {
    expect(kit.number(1500000)).toBe('1.500.000');
    expect(kit.number(0.1 + 0.2)).toBe('0,3');
  });

  /** An energy drift of -0.00001%, seen 2026-10-27 as "-0%". */
  it('never shows a minus on a number that rounds to zero', () => {
    expect(kit.number(-0.00001, 4)).toBe('0');
    expect(kit.number(-0)).toBe('0');
    expect(kit.number(-0.004)).toBe('0');
    expect(kit.number(-0.006)).toBe('-0,01');
  });
});

/**
 * What a visual claims of itself (plan 2026-10-28, §2.2). `synabit.lint` reads
 * these back in a page; `e2e/visual-lint/` checks it there, drawn in WebKit.
 */
describe("the kit's claims", () => {
  /** The tax of 2026-10-28: 5P = 120 + 3t, derived without the 3. */
  it('checks a derived number substituted back', () => {
    const t = 2;
    const wrong = (t + 120) / 5;
    expect(kit.expect('demand meets supply', 100 - 2 * wrong, 3 * (wrong - t) - 20, 1e-6)).toBe(false);
    const right = (120 + 3 * t) / 5;
    expect(kit.expect('demand meets supply', 100 - 2 * right, 3 * (right - t) - 20, 1e-6)).toBe(true);
  });

  it('holds float noise within a part in a billion without a tolerance, and nothing past it', () => {
    expect(kit.expect('sum', 0.1 + 0.2, 0.3)).toBe(true);
    expect(kit.expect('sum', 0.3001, 0.3)).toBe(false);
    expect(kit.expect('nothing', Number.NaN, 1, 10)).toBe(false);
    expect(kit.expect('a label', 'A', 'A')).toBe(true);
  });
});
