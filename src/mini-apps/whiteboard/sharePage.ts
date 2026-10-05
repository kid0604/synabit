import type { Box } from './inkLayer';

/**
 * A board as a web page anyone can open: one file, read-only.
 *
 * Synabit has no server to share from, and a board's file is only readable
 * by Synabit. A picture can be shared, but a board is bigger than a screen:
 * the person it is sent to wants to move around it and zoom into the corner
 * that matters. So the page carries the board as a picture made of the board
 * itself (the same SVG the SVG export writes), and a few lines of script to
 * pan, zoom and step from frame to frame.
 *
 * It needs nothing: no network, no fonts or images from anywhere — the export
 * has already put every image into the picture — and its policy forbids
 * fetching any, so a board sent as a file cannot be made to call home.
 *
 * The page runs in whatever browser the recipient has, so it uses only what
 * every browser has had for years: pointer events, transforms, DOMParser.
 */

export interface SharePageInput {
  title: string;
  /** The board as SVG markup, as the SVG export writes it. */
  svg: string;
  width: number;
  height: number;
  /**
   * The board's own colour. The SVG export has none — an SVG is meant to go
   * on top of something — and the page's own background is not the board's.
   */
  background?: string;
  /** Frames to step through, in the picture's coordinates, in order. */
  frames: { title: string; box: Box }[];
  lang: string;
  labels: {
    prev: string; next: string; fit: string; zoomIn: string; zoomOut: string; hint: string;
    /** The page's name when the board has none, in the reader's language. */
    untitled?: string;
  };
}

const escapeHtml = (text: string) =>
  text.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);

/**
 * Data for the page's script, as a script can hold it: JSON with every `<`
 * written as an escape, so nothing in a board — not even the words
 * "</script>" on a sticky note — can end the script early.
 */
const scriptData = (value: unknown) =>
  JSON.stringify(value).replace(/[<\u2028\u2029]/g, (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, '0')}`);

/** The svg data URL the SVG export makes, as markup. */
export function svgFromDataUrl(dataUrl: string): string {
  const comma = dataUrl.indexOf(',');
  const head = dataUrl.slice(0, comma);
  const body = dataUrl.slice(comma + 1);
  return head.includes(';base64') ? atob(body) : decodeURIComponent(body);
}

export function sharePage(input: SharePageInput): string {
  const { svg, width, height, frames, lang, labels } = input;
  const title = input.title?.trim() || labels.untitled?.trim() || 'Board';
  // A colour as the canvas reports one, and nothing that could end the rule.
  const background = /^[#\w(),.%\s-]+$/.test(input.background ?? '') ? input.background : '#ffffff';
  const data = scriptData({ svg, width, height, frames, labels });
  return `<!doctype html>
<html lang="${escapeHtml(lang)}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data: blob:; font-src data:; style-src 'unsafe-inline'; script-src 'unsafe-inline'">
<meta name="generator" content="Synabit">
<title>${escapeHtml(title)}</title>
<style>
  :root { color-scheme: light dark; --bg: #f4f4f5; --bar: rgb(255 255 255 / 0.9); --text: #18181b; --muted: #71717a; --line: #e4e4e7; }
  @media (prefers-color-scheme: dark) { :root { --bg: #18181b; --bar: rgb(39 39 42 / 0.9); --text: #fafafa; --muted: #a1a1aa; --line: #3f3f46; } }
  * { box-sizing: border-box; }
  html, body { margin: 0; height: 100%; overflow: hidden; background: var(--bg); color: var(--text); font: 14px/1.4 system-ui, -apple-system, "Segoe UI", sans-serif; }
  #stage { position: fixed; inset: 0; touch-action: none; cursor: grab; }
  #stage.moving { cursor: grabbing; }
  #board { position: absolute; left: 0; top: 0; transform-origin: 0 0; background: ${background}; box-shadow: 0 1px 12px rgb(0 0 0 / 0.12); }
  #board.moving { will-change: transform; }
  #board > svg { display: block; }
  header, nav { position: fixed; display: flex; align-items: center; gap: 4px; padding: 6px 8px; border: 1px solid var(--line); border-radius: 12px; background: var(--bar); backdrop-filter: blur(8px); }
  header { top: 12px; left: 12px; max-width: calc(100% - 24px); font-weight: 600; }
  header span { overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
  nav { bottom: 12px; left: 50%; transform: translateX(-50%); max-width: calc(100% - 24px); }
  button { all: unset; display: grid; place-items: center; min-width: 32px; height: 32px; padding: 0 6px; border-radius: 8px; cursor: pointer; font-size: 16px; }
  button:hover:not([disabled]) { background: rgb(127 127 127 / 0.15); }
  button:focus-visible { outline: 2px solid #3b82f6; outline-offset: 1px; }
  button[disabled] { opacity: 0.35; cursor: default; }
  #frames { display: flex; align-items: center; gap: 4px; }
  #where { padding: 0 6px; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 40ch; }
  .sep { width: 1px; height: 20px; margin: 0 4px; background: var(--line); }
  [hidden] { display: none !important; }
</style>
</head>
<body>
<div id="stage" aria-label="${escapeHtml(labels.hint)}"><div id="board"></div></div>
<header><span>${escapeHtml(title)}</span></header>
<nav>
  <span id="frames" hidden>
    <button id="prev" aria-label="${escapeHtml(labels.prev)}" title="${escapeHtml(labels.prev)}">&#8249;</button>
    <span id="where" aria-live="polite"></span>
    <button id="next" aria-label="${escapeHtml(labels.next)}" title="${escapeHtml(labels.next)}">&#8250;</button>
    <span class="sep"></span>
  </span>
  <button id="out" aria-label="${escapeHtml(labels.zoomOut)}" title="${escapeHtml(labels.zoomOut)}">&#8722;</button>
  <button id="fit" aria-label="${escapeHtml(labels.fit)}" title="${escapeHtml(labels.fit)}">&#9634;</button>
  <button id="in" aria-label="${escapeHtml(labels.zoomIn)}" title="${escapeHtml(labels.zoomIn)}">+</button>
</nav>
<script>
(function () {
  var D = ${data};
  var stage = document.getElementById('stage');
  var board = document.getElementById('board');
  var picture = new DOMParser().parseFromString(D.svg, 'image/svg+xml').documentElement;
  board.appendChild(document.importNode(picture, true));
  board.style.width = D.width + 'px';
  board.style.height = D.height + 'px';

  var still = window.matchMedia && matchMedia('(prefers-reduced-motion: reduce)').matches;
  var view = { x: 0, y: 0, s: 1 };
  var at = -1;
  var idle = 0;
  function draw() {
    board.style.transform = 'translate(' + view.x + 'px,' + view.y + 'px) scale(' + view.s + ')';
    // Smooth while moving; sharp again once still.
    board.classList.add('moving');
    clearTimeout(idle);
    idle = setTimeout(function () { board.classList.remove('moving'); }, 200);
  }
  function clamp(s) { return Math.min(8, Math.max(0.05, s)); }
  function target(box) {
    var w = innerWidth, h = innerHeight - 120;
    var s = clamp(Math.min(w / box.width, h / box.height) * 0.94);
    return { s: s, x: (w - box.width * s) / 2 - box.x * s, y: 60 + (h - box.height * s) / 2 - box.y * s };
  }
  var anim = 0;
  function moveTo(next) {
    cancelAnimationFrame(anim);
    if (still) { view = next; draw(); return; }
    var from = { x: view.x, y: view.y, s: view.s }, t0 = performance.now();
    (function step(now) {
      var k = Math.min(1, (now - t0) / 350), e = 1 - Math.pow(1 - k, 3);
      view = { x: from.x + (next.x - from.x) * e, y: from.y + (next.y - from.y) * e, s: from.s + (next.s - from.s) * e };
      draw();
      if (k < 1) anim = requestAnimationFrame(step);
    })(t0);
  }
  function whole() { return { x: 0, y: 0, width: D.width, height: D.height }; }
  function show(i) {
    at = i;
    moveTo(target(i < 0 ? whole() : D.frames[i].box));
    if (!D.frames.length) return;
    document.getElementById('where').textContent = i < 0 ? '' : (i + 1) + ' / ' + D.frames.length + (D.frames[i].title ? ' \\u00b7 ' + D.frames[i].title : '');
    document.getElementById('prev').disabled = i <= 0;
    document.getElementById('next').disabled = i >= D.frames.length - 1;
  }
  function zoom(factor, cx, cy) {
    cancelAnimationFrame(anim);
    var s = clamp(view.s * factor);
    view = { s: s, x: cx - (cx - view.x) * (s / view.s), y: cy - (cy - view.y) * (s / view.s) };
    draw();
  }

  if (D.frames.length) document.getElementById('frames').hidden = false;
  document.getElementById('prev').onclick = function () { show(Math.max(0, at - 1)); };
  document.getElementById('next').onclick = function () { show(Math.min(D.frames.length - 1, at + 1)); };
  document.getElementById('fit').onclick = function () { show(-1); };
  document.getElementById('in').onclick = function () { zoom(1.25, innerWidth / 2, innerHeight / 2); };
  document.getElementById('out').onclick = function () { zoom(0.8, innerWidth / 2, innerHeight / 2); };

  stage.addEventListener('wheel', function (e) {
    e.preventDefault();
    if (e.ctrlKey || e.metaKey) zoom(Math.exp(-e.deltaY * 0.01), e.clientX, e.clientY);
    else { cancelAnimationFrame(anim); view.x -= e.deltaX; view.y -= e.deltaY; draw(); }
  }, { passive: false });

  var pointers = {};
  var pinch = null;
  stage.addEventListener('pointerdown', function (e) {
    stage.setPointerCapture(e.pointerId);
    pointers[e.pointerId] = { x: e.clientX, y: e.clientY };
    stage.classList.add('moving');
    cancelAnimationFrame(anim);
  });
  stage.addEventListener('pointermove', function (e) {
    var p = pointers[e.pointerId];
    if (!p) return;
    var ids = Object.keys(pointers);
    if (ids.length === 2) {
      var a = pointers[ids[0]], b = pointers[ids[1]];
      var before = Math.hypot(a.x - b.x, a.y - b.y);
      p.x = e.clientX; p.y = e.clientY;
      var after = Math.hypot(a.x - b.x, a.y - b.y);
      if (before > 0) zoom(after / before, (a.x + b.x) / 2, (a.y + b.y) / 2);
      return;
    }
    view.x += e.clientX - p.x; view.y += e.clientY - p.y;
    p.x = e.clientX; p.y = e.clientY;
    draw();
  });
  function up(e) { delete pointers[e.pointerId]; if (!Object.keys(pointers).length) stage.classList.remove('moving'); }
  stage.addEventListener('pointerup', up);
  stage.addEventListener('pointercancel', up);

  addEventListener('keydown', function (e) {
    if (e.target.closest && e.target.closest('button') && (e.key === 'Enter' || e.key === ' ')) return;
    var n = D.frames.length;
    if ((e.key === 'ArrowRight' || e.key === 'PageDown' || e.key === ' ') && n) show(Math.min(n - 1, at + 1));
    else if ((e.key === 'ArrowLeft' || e.key === 'PageUp') && n) show(Math.max(0, at - 1));
    else if (e.key === '0' || e.key === 'f') show(-1);
    else if (e.key === '+' || e.key === '=') zoom(1.25, innerWidth / 2, innerHeight / 2);
    else if (e.key === '-') zoom(0.8, innerWidth / 2, innerHeight / 2);
    else return;
    e.preventDefault();
  });
  addEventListener('resize', function () { show(at); });
  show(-1);
})();
</script>
</body>
</html>
`;
}
