import { test, expect } from '@playwright/test';
import { mkdtempSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
// @ts-expect-error — a plain ES module, shared with the eval.
import { trial } from '../visualTrial.mjs';

/**
 * `synabit.lint`, on visuals Syn drew. Each page in this folder is one a model
 * kept (its header says which), with the fault its check exists for; a few
 * were changed to have a fault no kept visual has had yet, and say how.
 * Every check must find its page, and find nothing a page does not have:
 * the `clean-` pages are what a check that cries wolf would fail on.
 *
 * `npm run visual-lint`
 */
type Case = { file: string; fault: string; checks: string[]; proof?: { name: string; value: number; tolerance?: number }[] };
type Found = { file: string; problems: { check: string; detail: string }[] };

const HERE = dirname(fileURLToPath(import.meta.url));
const cases: Case[] = JSON.parse(readFileSync(join(HERE, 'cases.json'), 'utf8'));
const byFile = new Map(cases.map((c) => [c.file, c]));

let found: Map<string, Found>;

test.beforeAll(async () => {
  const results: Found[] = await trial(
    cases.map((c) => join(HERE, c.file)),
    { options: (path: string) => ({ proof: byFile.get(path.split('/').pop() as string)?.proof }) },
  );
  found = new Map(results.map((r) => [r.file.split('/').pop() as string, r]));
});

for (const c of cases) {
  test(`${c.file}: ${c.checks.length ? c.checks.join(', ') : 'nothing'}`, () => {
    const r = found.get(c.file);
    expect(r, 'the page was drawn').toBeTruthy();
    const checks = [...new Set(r!.problems.map((p) => p.check))].sort();
    expect(checks, r!.problems.map((p) => `${p.check}: ${p.detail}`).join('\n')).toEqual([...c.checks].sort());
  });
}

test('every check has a page that has its fault', () => {
  const checks = ['empty', 'hang', 'error', 'nan', 'long-decimal', 'raw-math', 'flat-plot', 'overflow', 'overlap', 'dead-control', 'fixed-colour', 'expect', 'proof'];
  for (const check of checks) expect(cases.some((c) => c.file === `${check}.html` && c.checks.includes(check)), check).toBe(true);
});

/**
 * The example in each pattern's guide (`guides/patterns/*.md`) is what a model
 * copies (plan 2026-10-28, §2.1): it must have nothing the lint finds, and
 * every claim it makes with `synabit.expect` must hold.
 */
const PATTERNS = join(HERE, '../../src-tauri/src/syn/visual/guides/patterns');
const examples = readdirSync(PATTERNS)
  .filter((f) => f.endsWith('.md'))
  .map((f) => {
    const block = /```html\n([\s\S]*?)\n```/.exec(readFileSync(join(PATTERNS, f), 'utf8'));
    return { pattern: f.replace(/\.md$/, ''), html: block ? block[1] : '' };
  });

test('every pattern guide has an example, and the lint finds nothing in it', async () => {
  expect(examples.map((e) => e.pattern).sort()).toEqual(['areas', 'compare', 'decompose', 'explore', 'step', 'zoom']);
  const dir = mkdtempSync(join(tmpdir(), 'synabit-patterns-'));
  const files = examples.map((e) => {
    expect(e.html, `${e.pattern} has an html block`).not.toBe('');
    const file = join(dir, `${e.pattern}.html`);
    writeFileSync(file, e.html);
    return file;
  });
  const results: (Found & { expects: { name: string; ok: boolean }[]; reported: Record<string, unknown> })[] = await trial(files);
  for (const r of results) {
    const name = r.file.split('/').pop();
    expect(r.problems.map((p) => `${p.check}: ${p.detail}`), name).toEqual([]);
    expect(r.expects.length, `${name} checks a claim with synabit.expect`).toBeGreaterThan(0);
    expect(r.expects.filter((e) => !e.ok).map((e) => e.name), name).toEqual([]);
    expect(Object.keys(r.reported).length, `${name} reports its numbers`).toBeGreaterThan(0);
  }
});
