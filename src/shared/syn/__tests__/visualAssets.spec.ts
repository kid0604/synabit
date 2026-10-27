/**
 * The libraries a Syn visual is given are copies of the app's own d3 and
 * KaTeX, kept in `src-tauri/src/syn/visual/lib/` so the Rust build needs no
 * `node_modules`. This says when they drift: the app updated a library and
 * the visuals kept the old one. To refresh, copy the same files again.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

declare const __dirname: string;

const ROOT = resolve(__dirname, '../../../..');
const vendored = (file: string): Uint8Array => readFileSync(resolve(ROOT, 'src-tauri/src/syn/visual/lib', file));
const installed = (file: string): Uint8Array => readFileSync(resolve(ROOT, 'node_modules', file));
const same = (a: Uint8Array, b: Uint8Array) => a.length === b.length && a.every((byte, i) => byte === b[i]);

describe("the libraries a visual is given are the app's own", () => {
  it('d3', () => {
    expect(same(vendored('d3.v7.min.js'), installed('d3/dist/d3.min.js'))).toBe(true);
  });

  it('KaTeX, its stylesheet and every woff2 font', () => {
    expect(same(vendored('katex.min.js'), installed('katex/dist/katex.min.js'))).toBe(true);
    expect(same(vendored('katex.min.css'), installed('katex/dist/katex.min.css'))).toBe(true);
    const fonts = readdirSync(resolve(ROOT, 'node_modules/katex/dist/fonts'), { withFileTypes: true })
      .map((entry) => entry.name)
      .filter((name) => name.endsWith('.woff2'));
    expect(fonts.length).toBe(20);
    for (const font of fonts) {
      expect(same(vendored(`fonts/${font}`), installed(`katex/dist/fonts/${font}`)), font).toBe(true);
    }
  });
});
