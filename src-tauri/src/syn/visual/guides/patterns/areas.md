# Pattern: areas

For "who bears how much", "how much is lost", "how far did it go": amounts
that are areas between lines.

Must have:
- **Each region shaded in its own colour, with its amount written inside it**
  (or beside it, joined by a line, when it is too small), and named in a
  legend.
- **The regions on the same axes and scale as the lines that bound them**:
  a region's corners are points of those lines, never coordinates typed by
  hand.
- Every corner and crossing worked out from the model's functions (solve
  `demand(p) = supply(p)` numerically, or from the formula and then
  substituted back with `synabit.expect`), and every amount from those same
  points.
- The total, if the regions make one, written once and checked against the
  sum of the regions.

Copy how this is built, not what it shows:

```html
<div style="max-width:640px">
  <svg id="plot" viewBox="0 0 640 300" role="img" aria-label="Vận tốc theo thời gian; diện tích dưới đường là quãng đường" style="width:100%"></svg>
  <div class="row muted" style="font-size:12px;gap:14px;flex-wrap:wrap">
    <span><b style="color:var(--c1)">■</b> tăng tốc</span>
    <span><b style="color:var(--c2)">■</b> chạy đều</span>
    <span>Quãng đường: <b id="total" style="color:var(--text)"></b></span>
  </div>
</div>
<script>
// The model, once: speeds up evenly to 10 m/s in 4 s, then holds it to 10 s.
const vTop = 10, tUp = 4, tEnd = 10;
const v = (t) => (t < tUp ? (vTop * t) / tUp : vTop);
// Each amount from the same function: the area under it, summed in small steps.
const area = (a, b) => {
  let s = 0;
  const n = 1000, h = (b - a) / n;
  for (let i = 0; i < n; i++) s += ((v(a + i * h) + v(a + (i + 1) * h)) / 2) * h;
  return s;
};
const regions = [
  { name: 'tăng tốc', from: 0, to: tUp, colour: 'c1' },
  { name: 'chạy đều', from: tUp, to: tEnd, colour: 'c2' },
].map((r) => ({ ...r, metres: area(r.from, r.to) }));
const total = area(0, tEnd);
synabit.expect('the regions add up to the whole', regions[0].metres + regions[1].metres, total, 1e-6);
synabit.expect('speeding up: ½ · v · t', regions[0].metres, 0.5 * vTop * tUp, 1e-3);
synabit.report({ 'quãng đường (m)': total, 'tăng tốc (m)': regions[0].metres, 'chạy đều (m)': regions[1].metres });

const ns = 'http://www.w3.org/2000/svg';
const el = (tag, attrs, text) => {
  const e = document.createElementNS(ns, tag);
  for (const k in attrs) e.setAttribute(k, attrs[k]);
  if (text != null) e.textContent = text;
  document.getElementById('plot').appendChild(e);
  return e;
};
const W = 640, H = 300, L = 44, R = 12, T = 12, B = 40;
const x = (t) => L + (t / tEnd) * (W - L - R);
const y = (s) => T + (1 - s / 12) * (H - T - B);
for (const s of [0, 4, 8, 12]) {
  el('line', { x1: L, x2: W - R, y1: y(s), y2: y(s), stroke: 'var(--grid)' });
  el('text', { x: L - 6, y: y(s) + 4, 'text-anchor': 'end', fill: 'var(--muted)', 'font-size': 11 }, synabit.number(s, 0));
}
for (let t = 0; t <= tEnd; t += 2) el('text', { x: x(t), y: H - B + 16, 'text-anchor': 'middle', fill: 'var(--muted)', 'font-size': 11 }, synabit.number(t, 0));
el('text', { x: L, y: H - 4, fill: 'var(--muted)', 'font-size': 11 }, 'thời gian (s) · vận tốc (m/s)');

for (const r of regions) {
  // The region's edge is the line itself, sampled from the same function.
  const top = [];
  for (let i = 0; i <= 40; i++) {
    const t = r.from + ((r.to - r.from) * i) / 40;
    top.push(`${x(t)},${y(v(t))}`);
  }
  el('polygon', { points: `${x(r.from)},${y(0)} ${top.join(' ')} ${x(r.to)},${y(0)}`, fill: `var(--${r.colour})`, 'fill-opacity': 0.25 });
  const mid = (r.from + r.to) / 2;
  el('text', { x: x(mid), y: y(v(mid) / 3), 'text-anchor': 'middle', fill: 'var(--text)', 'font-size': 13, 'font-weight': 600 }, `${synabit.number(r.metres, 0)} m`);
}
const line = [];
for (let i = 0; i <= 100; i++) line.push(`${x((tEnd * i) / 100)},${y(v((tEnd * i) / 100))}`);
el('polyline', { points: line.join(' '), fill: 'none', stroke: 'var(--text-2)', 'stroke-width': 2 });
document.getElementById('total').textContent = `${synabit.number(total, 0)} m`;
</script>
```
