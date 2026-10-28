# Pattern: zoom

For a named phenomenon or a special point: an overshoot, a jump, a slow
convergence, where an approximation stops holding.

Must have:
- **The whole view, and a magnified view of the phenomenon, both on screen**:
  the region magnified is marked on the whole view.
- **The phenomenon measured**, as a number with its unit or as a share,
  written large next to the magnified view: how high, how wide, how far off.
- The measure found from the model with `synabit.solve` or `synabit.peak`,
  over the whole range the model covers, not a part of it, and checked with `synabit.expect` against a value worked out
  another way (a formula, a limit, a known constant).
- Both views drawn from the same function; the magnified one only narrows
  the range.
- Built with two `synabit.chart`s given the same series and different
  ranges; the region is marked on the whole view through the scales and the
  `<svg>` the chart returns.

Copy how this is built, not what it shows:

```html
<div style="display:grid;grid-template-columns:3fr 2fr;gap:12px;align-items:start;max-width:640px">
  <div id="whole"></div>
  <div><div id="near"></div><div id="measure"></div></div>
</div>
<script>
const curves = [{ name: 'sin x', fn: Math.sin }, { name: 'x', fn: (x) => x, dashed: true }];
// The measure, solved from the curves themselves: where x stops being
// within 1% of sin x.
const off = (x) => (x - Math.sin(x)) / Math.sin(x);
const from = synabit.solve((x) => off(x) - 0.01, 0.01, 2);
// Checked another way: the first term the approximation leaves out, x³/6,
// is 1% of x where x² = 0.06.
synabit.expect('1% where x²/6 = 1%', from, Math.sqrt(0.06), 0.005);
synabit.report({ 'lệch 1% từ x (rad)': from });

// Both views from the same curves; the magnified one only narrows the range.
const whole = synabit.chart('#whole', { title: 'Toàn cảnh', x: { min: -2, max: 2 }, y: { min: -2, max: 2 }, series: curves });
// Mark on the whole view the region the magnified one shows.
const svg = whole.svgs[0];
const box = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
for (const [k, v] of Object.entries({ x: whole.x(0), y: whole.y[0](1), width: whole.x(1) - whole.x(0), height: whole.y[0](0) - whole.y[0](1), fill: 'none', stroke: 'var(--accent)', 'stroke-dasharray': '4 3' })) box.setAttribute(k, v);
svg.appendChild(box);

synabit.chart('#near', {
  title: 'Phóng to từ 0 tới 1',
  x: { label: 'x', min: 0, max: 1 },
  y: { min: 0, max: 1 },
  series: curves,
  marks: [{ x: from, y: Math.sin(from), label: 'lệch 1%' }],
});
synabit.stats('#measure', [{ label: 'Lệch 1% từ', value: from, unit: 'rad', digits: 2, note: 'tìm trên cả đoạn vẽ' }]);
</script>
```
