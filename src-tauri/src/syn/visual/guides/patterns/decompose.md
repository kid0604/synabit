# Pattern: decompose

For what a whole is made of, where a number comes from, or a conditional
probability (the whole split by one condition, then by the other).

Must have:
- **The whole, split into its parts, in one picture**: a bar or a grid of
  areas where each part's size is its share.
- **Every part labelled with its amount and its share**, on the part or in a
  legend beside it with the same colour.
- **The parts add up to the whole, and the visual says so**: the total
  written once, and checked with `synabit.expect`.
- Parts computed from the whole and the rule that splits it, not typed in
  twice. Where a part is a share of a share (1% have it, 99% of those test
  positive), the path from the whole to that part is visible.

Copy how this is built, not what it shows:

```html
<div style="max-width:640px">
  <div class="row" style="justify-content:space-between;margin-bottom:6px">
    <span style="font-weight:600">Một ly cà phê sữa</span>
    <span class="stat" id="total"></span>
  </div>
  <svg id="bar" viewBox="0 0 640 56" role="img" aria-label="Giá một ly cà phê chia theo khoản" style="width:100%"></svg>
  <div id="legend" style="display:grid;grid-template-columns:auto 1fr auto auto;gap:6px 12px;margin-top:10px;font-variant-numeric:tabular-nums"></div>
</div>
<script>
// The whole and the rule that splits it: each part a share of the price.
const price = 45000;
const shares = [
  ['Nguyên liệu', 0.27], ['Nhân công', 0.24], ['Mặt bằng', 0.2],
  ['Điện, nước', 0.07], ['Thuế', 0.1], ['Lãi', 0.12],
];
const parts = shares.map(([name, share], i) => ({ name, share, amount: price * share, colour: `var(--c${(i % 6) + 1})` }));
const sum = parts.reduce((s, p) => s + p.amount, 0);
synabit.expect('the parts add up to the price', sum, price, 0.5);
synabit.report(Object.fromEntries(parts.map((p) => [`${p.name} (đ)`, p.amount])));

document.getElementById('total').textContent = `${synabit.number(price, 0)} đ`;
const svg = document.getElementById('bar');
const ns = 'http://www.w3.org/2000/svg';
let x = 0;
for (const p of parts) {
  const w = p.share * 640;
  const r = document.createElementNS(ns, 'rect');
  for (const [k, v] of Object.entries({ x, y: 0, width: w - 2, height: 56, rx: 4, fill: p.colour })) r.setAttribute(k, v);
  const tip = document.createElementNS(ns, 'title');
  tip.textContent = `${p.name}: ${synabit.number(p.amount, 0)} đ`;
  r.appendChild(tip);
  svg.appendChild(r);
  x += w;
}
const legend = document.getElementById('legend');
for (const p of parts) {
  legend.insertAdjacentHTML('beforeend',
    `<span style="width:12px;height:12px;border-radius:3px;background:${p.colour};align-self:center"></span>` +
    `<span>${p.name}</span>` +
    `<span style="text-align:right">${synabit.number(p.amount, 0)} đ</span>` +
    `<span class="muted" style="text-align:right">${synabit.number(p.share * 100, 0)}%</span>`);
}
legend.insertAdjacentHTML('beforeend',
  `<span></span><span style="font-weight:600">Cộng</span><span style="text-align:right;font-weight:600">${synabit.number(sum, 0)} đ</span><span class="muted" style="text-align:right">100%</span>`);
</script>
```
