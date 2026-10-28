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

Copy how this is built, not what it shows:

```html
<div style="max-width:640px;display:grid;grid-template-columns:1fr 1fr;gap:16px;align-items:center">
  <svg id="circuit" viewBox="0 0 260 220" role="img" aria-label="Mạch chia áp: nguồn, R1 và R2 nối tiếp" style="width:100%"></svg>
  <div>
    <div class="muted" style="font-size:12px">Điện áp ra (trên R2)</div>
    <div class="stat" id="vout"></div>
    <div id="check" class="muted" style="font-size:12px;margin:4px 0 14px"></div>
    <div id="controls" style="display:grid;gap:10px"></div>
  </div>
</div>
<script>
const vin = 12;
const params = [
  { id: 'r1', label: 'R1', min: 1, max: 10, step: 0.5, value: 4, unit: 'kΩ' },
  { id: 'r2', label: 'R2', min: 1, max: 10, step: 0.5, value: 2, unit: 'kΩ' },
];
const box = document.getElementById('controls');
for (const p of params) {
  box.insertAdjacentHTML('beforeend',
    `<div><div class="row" style="justify-content:space-between"><label for="${p.id}">${p.label}</label><span id="${p.id}v"></span></div>` +
    `<input id="${p.id}" type="range" min="${p.min}" max="${p.max}" step="${p.step}" value="${p.value}" style="width:100%">` +
    `<div class="row muted" style="justify-content:space-between;font-size:11px"><span>${p.min} ${p.unit}</span><span>${p.max} ${p.unit}</span></div></div>`);
}
const ns = 'http://www.w3.org/2000/svg';
const el = (tag, attrs, text) => {
  const e = document.createElementNS(ns, tag);
  for (const k in attrs) e.setAttribute(k, attrs[k]);
  if (text != null) e.textContent = text;
  document.getElementById('circuit').appendChild(e);
  return e;
};

// The model, once: the current through both, and the voltage on each.
function solve(r1, r2) {
  const i = vin / (r1 + r2); // mA, with kΩ
  return { i, v1: i * r1, v2: i * r2 };
}

function draw() {
  const [r1, r2] = params.map((p) => +document.getElementById(p.id).value);
  params.forEach((p, k) => (document.getElementById(`${p.id}v`).textContent = `${synabit.number([r1, r2][k], 1)} ${p.unit}`));
  const s = solve(r1, r2);
  document.getElementById('vout').textContent = `${synabit.number(s.v2, 2)} V`;
  // The invariant beside the result: the two drops add up to the source.
  const sum = s.v1 + s.v2;
  synabit.expect('the drops add up to the source', sum, vin, 1e-9);
  document.getElementById('check').textContent = `Kiểm: ${synabit.number(s.v1, 2)} + ${synabit.number(s.v2, 2)} = ${synabit.number(sum, 2)} V = nguồn`;
  synabit.report({ 'điện áp ra (V)': s.v2, 'dòng (mA)': s.i });

  const svg = document.getElementById('circuit');
  svg.replaceChildren();
  // Each resistor drawn as long as its share of the total.
  const span = 150, top = 30, h1 = (span * r1) / (r1 + r2);
  el('line', { x1: 40, y1: top, x2: 40, y2: top + span, stroke: 'var(--text-2)', 'stroke-width': 2 });
  el('line', { x1: 40, y1: top, x2: 150, y2: top, stroke: 'var(--text-2)', 'stroke-width': 2 });
  el('line', { x1: 40, y1: top + span, x2: 150, y2: top + span, stroke: 'var(--text-2)', 'stroke-width': 2 });
  el('rect', { x: 140, y: top, width: 20, height: h1, rx: 3, fill: 'var(--c2)', 'fill-opacity': 0.5, stroke: 'var(--c2)' });
  el('rect', { x: 140, y: top + h1, width: 20, height: span - h1, rx: 3, fill: 'var(--c1)', 'fill-opacity': 0.5, stroke: 'var(--c1)' });
  el('text', { x: 170, y: top + h1 / 2 + 4, fill: 'var(--text)', 'font-size': 12 }, `R1 · ${synabit.number(s.v1, 1)} V`);
  el('text', { x: 170, y: top + h1 + (span - h1) / 2 + 4, fill: 'var(--text)', 'font-size': 12 }, `R2 · ${synabit.number(s.v2, 1)} V`);
  el('text', { x: 10, y: top + span / 2 + 4, fill: 'var(--text)', 'font-size': 12 }, `${vin} V`);
  el('text', { x: 40, y: top + span + 26, fill: 'var(--muted)', 'font-size': 11 }, `dòng ${synabit.number(s.i, 2)} mA`);
}
for (const p of params) document.getElementById(p.id).addEventListener('input', draw);
draw();
</script>
```
