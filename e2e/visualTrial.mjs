// Runs Syn's visuals in WebKit — the engine macOS's WKWebView is — the way the
// app's `synvisual` frame serves them, and says what `synabit.lint` found.
//
// The eval's harness has no WebView (`tauri::test::MockRuntime`), and a model
// never sees what it drew; this is where a visual is drawn and looked at
// outside the app (`docs/syn-visuals-quality-plan-2026-10-28.md` §2.3, PR Q1).
//
//   node e2e/visualTrial.mjs [--theme dark] [--shots <dir>] [--json <file>] [--proof <json>] [--explore] <file or dir>...
//
// A file is a visual's fragment, as its node keeps it, or a whole page, as
// `SYN_EVAL_KEEP` writes it. Fragments are served as `protocol::assemble`
// serves them: the same policy, `/kit.js`, `/kit.css` and `/lib/`.
import { webkit } from '@playwright/test';
import { readFileSync, readdirSync, statSync, writeFileSync, mkdirSync } from 'node:fs';
import { basename, dirname, extname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
export const VISUAL = resolve(HERE, '../src-tauri/src/syn/visual');
export const ORIGIN = 'http://synvisual.localhost';

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

export const assemble = (fragment, theme) =>
  `<!doctype html>\n<html lang="vi" data-theme="${theme}">\n<head>\n<meta charset="utf-8">\n` +
  `<meta http-equiv="Content-Security-Policy" content="${policy()}">\n` +
  `<meta name="viewport" content="width=device-width, initial-scale=1">\n` +
  `<link rel="stylesheet" href="/kit.css">\n<script src="/kit.js"></script>\n</head>\n<body>\n${fragment}\n</body>\n</html>\n`;

// The page a visual is shown in, as the app shows it: a frame sandboxed to
// `allow-scripts`, so its origin is opaque, under a page that answers the
// kit's `ui/initialize`. Looked at outside such a frame, a visual's errors
// carried their messages; in the app every one was "Script error.", and the
// eval never saw what a model in the app was told (2026-10-28).
export const HOST = 'http://app.localhost';
const hostPage = (theme) =>
  `<!doctype html>\n<html><body style="margin:0">\n<script>\naddEventListener('message', (event) => {\n` +
  `  const m = event.data;\n  if (!m || m.jsonrpc !== '2.0' || m.id == null) return;\n` +
  `  const result = m.method === 'ui/initialize'\n` +
  `    ? { protocolVersion: '2026-01-26', hostInfo: { name: 'synabit-trial', version: '1' }, hostCapabilities: {}, hostContext: { theme: '${theme}', locale: 'vi' } }\n` +
  `    : {};\n  event.source.postMessage({ jsonrpc: '2.0', id: m.id, result }, '*');\n});\n</script>\n` +
  `<iframe id="frame" sandbox="allow-scripts" src="${ORIGIN}/visual" style="border:0;display:block;width:100%;height:900px"></iframe>\n</body></html>\n`;

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

/** How long a page may take to load, and to be looked at whole. */
const LOAD = 10_000;
const WHOLE = 120_000;

/**
 * Each visual, drawn and looked at: `{ file, text, problems, expects,
 * reported, seen }`. `text` is what it shows as drawn; `controls` uses every
 * control once, as `synabit.lint` does with it; `explore` then loads it
 * again and walks every control, keeping the text of each state in `seen`.
 */
export async function trial(files, { theme = 'light', shots = null, controls = true, explore = false, width = 720, options = () => ({}) } = {}) {
  const one = async (browser, file) => {
    const source = readFileSync(file, 'utf8');
    const whole = /^\s*<!doctype/i.test(source);
    const page = await browser.newPage({ viewport: { width, height: 900 } });
    await page.route(`${HOST}/**`, (route) => route.fulfill({ contentType: TYPES['.html'], body: hostPage(theme) }));
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
    const open = async () => {
      await page.goto(`${HOST}/`, { timeout: LOAD });
      const frame = page.frames().find((f) => f.url().startsWith(ORIGIN));
      await frame.evaluate(() => window.synabit && window.synabit.ready);
      return frame;
    };
    const frame = await open();
    const text = await frame.evaluate(() => document.body.innerText);
    if (shots) {
      // As drawn, before the controls are used: the frame as tall as what it shows.
      mkdirSync(shots, { recursive: true });
      const tall = await frame.evaluate(() => document.documentElement.scrollHeight);
      await page.evaluate((h) => (document.getElementById('frame').style.height = `${Math.max(900, h)}px`), tall);
      await page.screenshot({ path: join(shots, `${basename(file, '.html')}.png`), fullPage: true });
      await page.evaluate(() => (document.getElementById('frame').style.height = '900px'));
    }
    let found;
    try {
      found = await frame.evaluate((o) => window.synabit.lint(o), { ...options(file), controls });
    } catch (error) {
      found = { problems: [{ check: 'error', detail: `the kit did not run: ${error.message}` }], expects: [], reported: {} };
    }
    let seen = [];
    if (explore) seen = await (await open()).evaluate(EXPLORE).catch(() => []);
    await page.close();
    return { file, text, ...found, seen, pageErrors: errors };
  };

  // A script that never ends — a Dijkstra whose `while (unvisited.length)`
  // never emptied it, 2026-10-28 — holds the page, and the frame in the app
  // with it. It is the visual's fault, and the browser is started again for
  // the next one.
  let browser = await webkit.launch();
  const results = [];
  try {
    for (const file of files) {
      let timer;
      const stuck = new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error(`did not finish in ${WHOLE / 1000} s`)), WHOLE);
      });
      try {
        results.push(await Promise.race([one(browser, file), stuck]));
      } catch (error) {
        const loading = /Timeout .* exceeded/.test(error.message) && /goto/.test(error.message);
        const detail = loading ? `the page did not finish loading in ${LOAD / 1000} s: a script that never ends` : `the page stopped answering: ${error.message.split('\n')[0]}`;
        results.push({ file, text: '', problems: [{ check: 'hang', detail }], expects: [], reported: {}, seen: [], pageErrors: [] });
        await browser.close().catch(() => {});
        browser = await webkit.launch();
      } finally {
        clearTimeout(timer);
      }
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
  // The numbers a brief says the visual shows: `[{ name, value }]`.
  const proof = option('--proof');
  const explore = args.includes('--explore');
  const results = await trial(pagesIn(args.filter((a) => a !== '--explore')), {
    theme,
    shots,
    explore,
    options: () => (proof ? { proof: JSON.parse(proof) } : {}),
  });
  for (const r of results) {
    console.log(`\n${basename(r.file)}: ${r.problems.length ? `${r.problems.length} problem(s)` : 'clean'}`);
    for (const p of r.problems) console.log(`  ${p.check.padEnd(13)} ${p.detail}`);
  }
  if (json) writeFileSync(json, JSON.stringify(results, null, 2));
  process.exitCode = results.some((r) => r.problems.length) ? 1 : 0;
}
