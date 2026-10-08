import { describe, it, expect } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';

// Vitest's own; see ./node.d.ts for why Node's types are not loaded.
declare const __dirname: string;

/**
 * The design floor, beyond what the linter can see.
 *
 * `npm run lint` already refuses `text-[10px]` and bare `text-gray-400` in a
 * class list. It cannot see a `font-size: 10px` in a `<style>` block or a
 * `style=""` attribute, and it has no rule for the two other ways grey text
 * slipped under 3:1 on white: a `placeholder-gray-400` placeholder, and bare
 * `text-gray-300` (1.5:1). This reads every `.vue` and `.css` file under
 * `src/` for exactly those, and lists what it finds with file and line.
 *
 * - **Font size below 12px**, in px or rem, in `<style>` blocks, `.css`
 *   files and `style=""` attributes.
 * - **`placeholder-gray-400` / `placeholder:text-gray-400`**, unprefixed. With
 *   `dark:` in front they are the dark theme's colour, which is fine.
 * - **Bare `text-gray-300`**, same rule.
 *
 * `ALLOWED` is empty: every debt it once listed has been paid. It stays as the
 * place to record one deliberately (file + rule → count, with a reason), and
 * the last test fails if a listed debt is paid without its line going too.
 */

/**
 * Every `.vue` and `.css` file under `src/`, read from disk.
 *
 * It used to be `import.meta.glob('../**\/*.{vue,css}', { query: '?raw' })`.
 * Under Vitest that hands back an empty string for every `.css` file — CSS
 * goes through the style pipeline, not the raw loader — so the stylesheet
 * half of this test read nothing and passed, while eleven sizes under the
 * floor sat in richTable.css and richBlocks.css. The file system cannot do that to us.
 */
const SRC = resolve(__dirname, '..');

function walk(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return entry.name === 'node_modules' ? [] : walk(path);
    return /\.(vue|css)$/.test(entry.name) ? [path] : [];
  });
}

const sources: Record<string, string> = Object.fromEntries(
  walk(SRC).map((path) => [relative(SRC, path).split('\\').join('/'), readFileSync(path, 'utf8')]),
);

type Rule = 'font-size' | 'placeholder-gray-400' | 'placeholder:text-gray-400' | 'text-gray-300';

const ALLOWED: Record<string, number> = {};

interface Offence {
  file: string;
  line: number;
  rule: Rule;
  detail: string;
}

const FONT_SIZE = /font-size\s*:\s*([\d.]+)(px|rem)\b/g;
// Unprefixed: the token starts after whitespace, a quote or a bracket, never
// after `dark:` or `hover:`.
const CLASSES = /(^|[\s"'`[,{(])(placeholder-gray-400|placeholder:text-gray-400|text-gray-300)(?![\w-])/g;

const lineAt = (src: string, index: number) => src.slice(0, index).split('\n').length;
const px = (value: string, unit: string) => (unit === 'rem' ? Number(value) * 16 : Number(value));

function findOffences(file: string, src: string): Offence[] {
  const found: Offence[] = [];
  const styles: Array<[number, number]> = [];
  if (file.endsWith('.css')) styles.push([0, src.length]);
  else {
    for (const m of src.matchAll(/<style[^>]*>[\s\S]*?<\/style>/g)) styles.push([m.index!, m.index! + m[0].length]);
  }

  for (const [start, end] of styles) {
    for (const m of src.slice(start, end).matchAll(FONT_SIZE)) {
      if (px(m[1], m[2]) < 12) {
        found.push({ file, line: lineAt(src, start + m.index!), rule: 'font-size', detail: `${m[1]}${m[2]}` });
      }
    }
  }

  if (file.endsWith('.vue')) {
    for (const m of src.matchAll(/style="([^"]*)"/g)) {
      for (const k of m[1].matchAll(FONT_SIZE)) {
        if (px(k[1], k[2]) < 12) {
          found.push({ file, line: lineAt(src, m.index!), rule: 'font-size', detail: `style="${m[1]}"` });
        }
      }
    }
    // Everything but the style blocks, blanked to keep line numbers true.
    let body = src;
    for (const [start, end] of [...styles].reverse()) {
      body = body.slice(0, start) + src.slice(start, end).replace(/[^\n]/g, ' ') + body.slice(end);
    }
    for (const m of body.matchAll(CLASSES)) {
      found.push({ file, line: lineAt(body, m.index! + m[1].length), rule: m[2] as Rule, detail: m[2] });
    }
  }
  return found;
}

describe('design floor', () => {
  const offences = Object.entries(sources).flatMap(([path, src]) => findOffences(path, src));

  /**
   * The check on the check: the stylesheets are actually read. Without it the
   * reader can break the way the old one did — every `.css` empty — and the
   * floor below would pass on nothing.
   */
  it('reads the stylesheets, not just the components', () => {
    expect(sources['shared/rich-table/richTable.css']?.length ?? 0).toBeGreaterThan(1000);
    expect(sources['style.css']?.length ?? 0).toBeGreaterThan(0);
    expect(Object.keys(sources).filter((f) => f.endsWith('.vue')).length).toBeGreaterThan(100);
  });

  it('finds what it is looking for', () => {
    expect(findOffences('x.vue', '<template><p class="text-gray-300 dark:text-gray-400"/></template>')).toHaveLength(1);
    expect(findOffences('x.vue', '<template><p class="text-gray-500 dark:text-gray-300"/></template>')).toHaveLength(0);
    expect(findOffences('x.vue', '<input class="placeholder-gray-400">')).toHaveLength(1);
    expect(findOffences('x.vue', '<input class="dark:placeholder:text-gray-400">')).toHaveLength(0);
    expect(findOffences('x.vue', '<p style="font-size: 11px">a</p>')).toHaveLength(1);
    expect(findOffences('x.vue', '<style>\n.a { font-size: 0.625rem }\n.b { font-size: 12px }\n</style>')).toHaveLength(1);
    expect(findOffences('x.css', '.a { font-size: 10px }')).toHaveLength(1);
  });

  it('has no text under 12px and no too-faint grey, beyond the listed debts', () => {
    const counts = new Map<string, Offence[]>();
    for (const o of offences) {
      const key = `${o.file} ${o.rule}`;
      counts.set(key, [...(counts.get(key) ?? []), o]);
    }
    const over = [...counts.entries()]
      .filter(([key, list]) => list.length > (ALLOWED[key] ?? 0))
      .flatMap(([, list]) => list.map((o) => `${o.file}:${o.line}  ${o.rule}  ${o.detail}`));
    expect(over, `Below the design floor:\n${over.join('\n')}`).toEqual([]);
  });

  /**
   * A debt that was paid must leave the list, or the list would quietly let
   * the same file slip back to where it was.
   */
  it('lists no debt that has already been paid', () => {
    const counts = new Map<string, number>();
    for (const o of offences) {
      const key = `${o.file} ${o.rule}`;
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
    const stale = Object.entries(ALLOWED)
      .filter(([key, n]) => (counts.get(key) ?? 0) < n)
      .map(([key, n]) => `${key}: listed ${n}, found ${counts.get(key) ?? 0}`);
    expect(stale, `Shrink ALLOWED:\n${stale.join('\n')}`).toEqual([]);
  });
});
