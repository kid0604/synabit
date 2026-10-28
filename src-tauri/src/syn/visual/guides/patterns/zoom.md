# Pattern: zoom

For a named phenomenon or a special point: an overshoot, a jump, a slow
convergence, where an approximation stops holding.

Must have:
- **The whole view, and a magnified view of the phenomenon, both on screen**:
  the region magnified is marked on the whole view.
- **The phenomenon measured**, as a number with its unit or as a share,
  written large next to the magnified view: how high, how wide, how far off.
- The measure found by searching the whole range the model covers, not a
  part of it, and checked with `synabit.expect` against a value worked out
  another way (a formula, a limit, a known constant).
- Both views drawn from the same function; the magnified one only narrows
  the range.

Copy how this is built, not what it shows:

```html
<div style="max-width:640px;display:grid;grid-template-columns:3fr 2fr;gap:12px;align-items:start">
  <div>
    <div style="font-weight:600">Toàn cảnh</div>
    <svg id="whole" viewBox="0 0 360 220" role="img" aria-label="sin x và x từ -2 tới 2" style="width:100%"></svg>
    <div class="row muted" style="font-size:12px;gap:14px">
      <span><b style="color:var(--c1)">—</b> sin x</span><span><b style="color:var(--c2)">—</b> x</span>
    </div>
  </div>
  <div>
    <div style="font-weight:600">Phóng to từ 0 tới 1</div>
    <svg id="near" viewBox="0 0 240 220" role="img" aria-label="sin x và x từ 0 tới 1, và chỗ hai đường lệch 1%" style="width:100%"></svg>
    <div class="muted" style="font-size:12px">lệch 1% từ</div>
    <div class="stat" id="from"></div>
  </div>
</div>
<script>
const ns = 'http://www.w3.org/2000/svg';
const el = (tag, attrs, parent, text) => {
  const e = document.createElementNS(ns, tag);
  for (const k in attrs) e.setAttribute(k, attrs[k]);
  if (text != null) e.textContent = text;
  parent.appendChild(e);
  return e;
};
const curves = [{ f: Math.sin, colour: 'c1' }, { f: (x) => x, colour: 'c2' }];

// One plot function for both views; the magnified one only narrows the range.
function plot(svg, [x0, x1], [y0, y1], W, ticks) {
  const L = 34, R = 8, T = 8, B = 26, H = 220;
  const sx = (x) => L + ((x - x0) / (x1 - x0)) * (W - L - R);
  const sy = (y) => T + (1 - (y - y0) / (y1 - y0)) * (H - T - B);
  for (const v of ticks.y) {
    el('line', { x1: L, x2: W - R, y1: sy(v), y2: sy(v), stroke: 'var(--grid)' }, svg);
    el('text', { x: L - 5, y: sy(v) + 4, 'text-anchor': 'end', fill: 'var(--muted)', 'font-size': 11 }, svg, synabit.number(v, 1));
  }
  for (const v of ticks.x) el('text', { x: sx(v), y: H - 8, 'text-anchor': 'middle', fill: 'var(--muted)', 'font-size': 11 }, svg, synabit.number(v, 1));
  for (const c of curves) {
    const pts = [];
    for (let i = 0; i <= 120; i++) {
      const x = x0 + ((x1 - x0) * i) / 120;
      pts.push(`${sx(x)},${sy(c.f(x))}`);
    }
    el('polyline', { points: pts.join(' '), fill: 'none', stroke: `var(--${c.colour})`, 'stroke-width': 2 }, svg);
  }
  return { sx, sy };
}

// The measure, searched over the whole range drawn: where x stops being
// within 1% of sin x.
const off = (x) => (x - Math.sin(x)) / Math.sin(x);
let from = null;
for (let x = 1e-4; x <= 2 && from === null; x += 1e-4) if (off(x) >= 0.01) from = x;
// Checked another way: the first term the approximation leaves out, x³/6,
// is 1% of x where x² = 0.06.
synabit.expect('1% where x²/6 = 1%', from, Math.sqrt(0.06), 0.005);
synabit.report({ 'lệch 1% từ x (rad)': from });

const wholeSvg = document.getElementById('whole');
const whole = plot(wholeSvg, [-2, 2], [-2, 2], 360, { x: [-2, -1, 0, 1, 2], y: [-2, -1, 0, 1, 2] });
// Mark on the whole view the region the magnified one shows.
el('rect', { x: whole.sx(0), y: whole.sy(1), width: whole.sx(1) - whole.sx(0), height: whole.sy(0) - whole.sy(1), fill: 'none', stroke: 'var(--accent)', 'stroke-dasharray': '4 3' }, wholeSvg);

const nearSvg = document.getElementById('near');
const near = plot(nearSvg, [0, 1], [0, 1], 240, { x: [0, 0.5, 1], y: [0, 0.5, 1] });
el('line', { x1: near.sx(from), x2: near.sx(from), y1: near.sy(0), y2: near.sy(1), stroke: 'var(--accent)', 'stroke-dasharray': '4 3' }, nearSvg);
el('text', { x: near.sx(from) + 4, y: near.sy(0.92), fill: 'var(--accent)', 'font-size': 11 }, nearSvg, '1%');
document.getElementById('from').textContent = `x ≈ ${synabit.number(from, 2)}`;
</script>
```
