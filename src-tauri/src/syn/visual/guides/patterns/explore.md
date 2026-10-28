# Pattern: explore

For a system the person asks to play with: "let me adjust", "simulate".

Must have:
- **The state, drawn**, and **the quantity the person asked about, written
  large** and updated as the controls move.
- **A check beside it**: the result against theory, or an invariant that
  must hold (energy, a total, a conservation law), with its value. It shows
  the model is right at every setting, not only at the first.
- **Each control has a label, its current value with its unit, and its
  range.** Two or three controls, for the parameters the question names.
- Starts at the values the question gave. Everything shown is computed from
  one function of the controls; moving a control redraws all of it.
- Built with `synabit.layout(el, 'side')`: `synabit.controls` in `controls`,
  whose callback redraws `synabit.stats` (the quantity, with the check as its
  note) and the drawing in `stage`.

Copy how this is built, not what it shows:

```html
<div id="v"></div>
<script>
const vin = 12;
// The model, once: the current through both, and the voltage on each.
const solve = (r1, r2) => {
  const i = vin / (r1 + r2); // mA, with kΩ
  return { i, v1: i * r1, v2: i * r2 };
};
const slots = synabit.layout('#v', 'side');
synabit.controls(slots.controls, [
  { id: 'r1', label: 'R1', min: 1, max: 10, step: 0.5, value: 4, unit: 'kΩ' },
  { id: 'r2', label: 'R2', min: 1, max: 10, step: 0.5, value: 2, unit: 'kΩ', help: 'Điện áp ra lấy trên R2' },
], ({ r1, r2 }) => {
  const s = solve(r1, r2);
  // The invariant beside the result: the two drops add up to the source.
  synabit.expect('the drops add up to the source', s.v1 + s.v2, vin, 1e-9);
  synabit.report({ 'điện áp ra (V)': s.v2, 'dòng (mA)': s.i });
  synabit.stats(slots.stats, [
    { label: 'Điện áp ra', value: s.v2, unit: 'V', digits: 2, note: `kiểm: ${synabit.number(s.v1, 2)} + ${synabit.number(s.v2, 2)} = ${synabit.number(s.v1 + s.v2, 2)} V = nguồn` },
    { label: 'Dòng', value: s.i, unit: 'mA', digits: 2, note: 'I = U / (R1 + R2)' },
  ]);
  // The state, drawn: each resistor as long as its share of the total.
  const ns = 'http://www.w3.org/2000/svg';
  const svg = document.createElementNS(ns, 'svg');
  svg.setAttribute('viewBox', '0 0 300 200');
  svg.setAttribute('role', 'img');
  svg.setAttribute('aria-label', 'Mạch chia áp: nguồn, R1 và R2 nối tiếp');
  svg.style.width = '100%';
  const el = (tag, attrs, text) => {
    const e = document.createElementNS(ns, tag);
    for (const k in attrs) e.setAttribute(k, attrs[k]);
    if (text != null) e.textContent = text;
    svg.appendChild(e);
  };
  const span = 150, top = 25, h1 = (span * r1) / (r1 + r2);
  for (const [x1, y1, x2, y2] of [[50, top, 50, top + span], [50, top, 150, top], [50, top + span, 150, top + span]]) {
    el('line', { x1, y1, x2, y2, stroke: 'var(--text-2)', 'stroke-width': 2 });
  }
  el('rect', { x: 140, y: top, width: 20, height: h1, rx: 3, fill: 'var(--c2)', 'fill-opacity': 0.5, stroke: 'var(--c2)' });
  el('rect', { x: 140, y: top + h1, width: 20, height: span - h1, rx: 3, fill: 'var(--c1)', 'fill-opacity': 0.5, stroke: 'var(--c1)' });
  el('text', { x: 170, y: top + h1 / 2 + 4, 'font-size': 12 }, `R1 · ${synabit.number(s.v1, 1)} V`);
  el('text', { x: 170, y: top + h1 + (span - h1) / 2 + 4, 'font-size': 12 }, `R2 · ${synabit.number(s.v2, 1)} V`);
  el('text', { x: 8, y: top + span / 2 + 4, 'font-size': 12 }, `${vin} V`);
  slots.stage.replaceChildren(svg);
});
</script>
```
