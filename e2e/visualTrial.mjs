// Runs Syn's visuals in WebKit — the engine macOS's WKWebView is — the way the
// app's `synvisual` frame serves them, and says what `synabit.lint` found.
//
// The eval's harness has no WebView (`tauri::test::MockRuntime`), and a model
// never sees what it drew; this is where a visual is drawn and looked at
// outside the app (`docs/syn-visuals-quality-plan-2026-10-28.md` §2.3, PR Q1).
//
//   node e2e/visualTrial.mjs [--theme dark] [--shots <dir>] [--json <file>] [--explore] <file or dir>...
//
// A file is a visual's fragment, as its node keeps it, or a whole page, as
// `SYN_EVAL_KEEP` writes it. Fragments are served as `protocol::assemble`
// serves them: the same policy, `/kit.js`, `/kit.css` and `/lib/`.
import { webkit } from '@playwright/test';
import { readFileSync, readdirSync, statSync, writeFileSync, mkdirSync } from 'node:fs';
import { basename, dirname, extname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const VISUAL = resolve(HERE, '../src-tauri/src/syn/visual');
const ORIGIN = 'http://synvisual.localhost';

// The policy, read from where the app keeps it rather than copied: a frame
// that runs here and not there would prove nothing.
const policy = () => {
  const source = readFileSync(join(VISUAL, 'protocol.rs'), 'utf8');
  const literal = source.match(/pub const CSP: &str = "([\s\S]*?)";/);
  if (!literal) throw new Error('protocol.rs no longer declares CSP the way this reads it');
  return literal[1]
    .replace(/\\\n\s*/g, '')
    .split(';')
    .map((d) => d.trim())
    .filter((d) => d && !d.startsWith('frame-ancestors'))
    .join('; ');
};

const assemble = (fragment, theme) =>
  `<!doctype html>\n<html lang="vi" data-theme="${theme}">\n<head>\n<meta charset="utf-8">\n` +
  `<meta http-equiv="Content-Security-Policy" content="${policy()}">\n` +
  `<meta name="viewport" content="width=device-width, initial-scale=1">\n` +
  `<link rel="stylesheet" href="/kit.css">\n<script src="/kit.js"></script>\n</head>\n<body>\n${fragment}\n</body>\n</html>\n`;

const TYPES = { '.js': 'text/javascript', '.css': 'text/css', '.woff2': 'font/woff2', '.html': 'text/html; charset=utf-8' };

export const pagesIn = (paths) =>
  paths.flatMap((path) =>
    statSync(path).isDirectory()
      ? readdirSync(path).filter((name) => name.endsWith('.html')).sort().map((name) => join(path, name))
      : [path],
  );

// Everything a visual can be made to show: every button pressed until it
// stops changing anything (a stepper walked to its last step), every slider
// at both ends and back, every option of every select. The text of each
// state is kept — an answer at the end of a walk is still an answer.
const EXPLORE = async () => {
  const settle = () => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(r, 60))));
  const seen = new Set([document.body.innerText]);
  const note = () => seen.size < 400 && seen.add(document.body.innerText);
  const controls = [...document.querySelectorAll('input, select, button')].filter((c) => !c.disabled && c.getClientRects().length);
  for (const control of controls) {
    if (control.tagName === 'BUTTON' || control.type === 'checkbox') {
      for (let n = 0; n < 30; n++) {
        const before = document.body.innerText;
        control.click();
        await settle();
        note();
        if (document.body.innerText === before && n > 0) break;
      }
    } else if (control.tagName === 'SELECT') {
      for (let i = 0; i < control.options.length; i++) {
        control.selectedIndex = i;
        control.dispatchEvent(new Event('change', { bubbles: true }));
        control.dispatchEvent(new Event('input', { bubbles: true }));
        await settle();
        note();
      }
    } else if (control.type === 'range' || control.type === 'number') {
      const start = control.value;
      for (const value of [control.min || '0', control.max || '100', start]) {
        control.value = value;
        control.dispatchEvent(new Event('input', { bubbles: true }));
        control.dispatchEvent(new Event('change', { bubbles: true }));
        await settle();
        note();
      }
    }
  }
  return [...seen];
};

/**
 * Each visual, drawn and looked at: `{ file, text, problems, expects,
 * reported, seen }`. `text` is what it shows as drawn; `controls` uses every
 * control once, as `synabit.lint` does with it; `explore` then loads it
 * again and walks every control, keeping the text of each state in `seen`.
 */
export async function trial(files, { theme = 'light', shots = null, controls = true, explore = false, width = 720, options = () => ({}) } = {}) {
  const browser = await webkit.launch();
  const results = [];
  try {
    for (const file of files) {
      const source = readFileSync(file, 'utf8');
      const whole = /^\s*<!doctype/i.test(source);
      const page = await browser.newPage({ viewport: { width, height: 900 } });
      await page.route(`${ORIGIN}/**`, (route) => {
        const path = new URL(route.request().url()).pathname;
        if (path === '/visual') {
          return route.fulfill({ contentType: TYPES['.html'], body: whole ? source : assemble(source, theme) });
        }
        const served = path === '/kit.js' || path === '/kit.css' ? join(VISUAL, path) : path.startsWith('/lib/') ? join(VISUAL, path) : null;
        if (!served || !served.startsWith(VISUAL)) return route.fulfill({ status: 404, body: '' });
        return route.fulfill({ contentType: TYPES[extname(served)] || 'application/octet-stream', body: readFileSync(served) });
      });
      const errors = [];
      page.on('pageerror', (error) => errors.push(error.message));
      await page.goto(`${ORIGIN}/visual`);
      await page.evaluate(() => window.synabit && window.synabit.ready);
      const text = await page.evaluate(() => document.body.innerText);
      if (shots) {
        // As drawn, before the controls are used.
        mkdirSync(shots, { recursive: true });
        await page.screenshot({ path: join(shots, `${basename(file, '.html')}.png`), fullPage: true });
      }
      let found;
      try {
        found = await page.evaluate((o) => window.synabit.lint(o), { ...options(file), controls });
      } catch (error) {
        found = { problems: [{ check: 'error', detail: `the kit did not run: ${error.message}` }], expects: [], reported: {} };
      }
      let seen = [];
      if (explore) {
        await page.goto(`${ORIGIN}/visual`);
        await page.evaluate(() => window.synabit && window.synabit.ready);
        seen = await page.evaluate(EXPLORE).catch(() => []);
      }
      results.push({ file, text, ...found, seen, pageErrors: errors });
      await page.close();
    }
  } finally {
    await browser.close();
  }
  return results;
}

const isMain = process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  const args = process.argv.slice(2);
  const option = (name) => {
    const at = args.indexOf(name);
    return at >= 0 ? args.splice(at, 2)[1] : null;
  };
  const theme = option('--theme') || 'light';
  const shots = option('--shots');
  const json = option('--json');
  const explore = args.includes('--explore');
  const results = await trial(pagesIn(args.filter((a) => a !== '--explore')), { theme, shots, explore });
  for (const r of results) {
    console.log(`\n${basename(r.file)}: ${r.problems.length ? `${r.problems.length} problem(s)` : 'clean'}`);
    for (const p of r.problems) console.log(`  ${p.check.padEnd(13)} ${p.detail}`);
  }
  if (json) writeFileSync(json, JSON.stringify(results, null, 2));
  process.exitCode = results.some((r) => r.problems.length) ? 1 : 0;
}
