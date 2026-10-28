# Pattern: compare

For "why does A differ from B", "compare", or what one parameter changes.

Must have:
- **The cases side by side, drawn at once**: the difference is seen without
  touching a control. Two cases, or a few; never one case and a slider that
  switches between them.
- **The same scale on every case**: the same axes, the same ranges, so equal
  lengths mean equal amounts. One case at a time on its own scale hides the
  difference.
- **One measure, the same for every case**, shown large for each: the number
  that says how they differ.
- Both cases computed by the same function with different inputs. A shared
  control, if any, changes every case at once.

Copy how this is built, not what it shows:

```html
<div style="max-width:640px">
  <div class="row" style="margin-bottom:8px">
    <label for="h">Độ cao thả</label>
    <input id="h" type="range" min="5" max="50" step="5" value="20" style="flex:1">
    <span id="hv" style="min-width:56px;text-align:right"></span>
    <span class="muted" style="font-size:12px">5–50 m</span>
  </div>
  <div id="cases" style="display:grid;grid-template-columns:1fr 1fr;gap:12px"></div>
</div>
<script>
const cases = [
  { name: 'Trái Đất', g: 9.81, colour: 'c1' },
  { name: 'Mặt Trăng', g: 1.62, colour: 'c2' },
];
const ns = 'http://www.w3.org/2000/svg';
const el = (tag, attrs, parent, text) => {
  const e = document.createElementNS(ns, tag);
  for (const k in attrs) e.setAttribute(k, attrs[k]);
  if (text != null) e.textContent = text;
  parent.appendChild(e);
  return e;
};
// The model, once: height after t seconds, and when it reaches the ground.
const height = (h, g, t) => h - 0.5 * g * t * t;
const landing = (h, g) => Math.sqrt((2 * h) / g);

function draw() {
  const h = +document.getElementById('h').value;
  document.getElementById('hv').textContent = `${synabit.number(h, 0)} m`;
  // One scale for both: the slowest case sets the time axis.
  const tMax = Math.max(...cases.map((c) => landing(h, c.g)));
  const W = 300, H = 210, L = 40, R = 10, T = 10, B = 34;
  const x = (t) => L + (t / tMax) * (W - L - R);
  const y = (v) => T + (1 - v / h) * (H - T - B);
  const root = document.getElementById('cases');
  root.replaceChildren();
  for (const c of cases) {
    const t1 = landing(h, c.g);
    synabit.expect(`${c.name}: on the ground at landing`, height(h, c.g, t1), 0, 1e-9);
    const box = document.createElement('div');
    box.innerHTML = `<div style="font-weight:600">${c.name}</div><div class="stat">${synabit.number(t1, 2)} s</div><div class="muted" style="font-size:12px">tới mặt đất</div>`;
    const svg = el('svg', { viewBox: `0 0 ${W} ${H}`, role: 'img', 'aria-label': `Độ cao theo thời gian trên ${c.name}` }, box);
    svg.style.width = '100%';
    for (const v of [0, h / 2, h]) {
      el('line', { x1: L, x2: W - R, y1: y(v), y2: y(v), stroke: 'var(--grid)' }, svg);
      el('text', { x: L - 6, y: y(v) + 4, 'text-anchor': 'end', fill: 'var(--muted)', 'font-size': 11 }, svg, synabit.number(v, 0));
    }
    for (const t of [0, tMax / 2, tMax]) {
      el('text', { x: x(t), y: H - B + 16, 'text-anchor': 'middle', fill: 'var(--muted)', 'font-size': 11 }, svg, synabit.number(t, 1));
    }
    el('text', { x: L, y: H - 2, fill: 'var(--muted)', 'font-size': 11 }, svg, 'thời gian (s) · độ cao (m)');
    const points = [];
    for (let i = 0; i <= 60; i++) {
      const t = (t1 * i) / 60;
      points.push(`${x(t)},${y(Math.max(0, height(h, c.g, t)))}`);
    }
    el('polyline', { points: points.join(' '), fill: 'none', stroke: `var(--${c.colour})`, 'stroke-width': 2 }, svg);
    el('circle', { cx: x(t1), cy: y(0), r: 4, fill: `var(--${c.colour})` }, svg);
    root.appendChild(box);
  }
  synabit.report(Object.fromEntries(cases.map((c) => [`${c.name}: thời gian chạm đất (s)`, landing(h, c.g)])));
}
document.getElementById('h').addEventListener('input', draw);
draw();
</script>
```
