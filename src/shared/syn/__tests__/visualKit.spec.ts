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
