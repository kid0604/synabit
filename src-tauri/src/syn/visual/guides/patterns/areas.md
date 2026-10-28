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
- Built with `synabit.chart`: each region an `areas` item, `below` the line
  that bounds it from above (and `above` the one below, if not zero), with
  its amount as `label`.

Copy how this is built, not what it shows:

```html
<div id="v"></div>
<script>
const end = 10;
// The model, once: speeds up evenly to `top` in `up` seconds, then holds it.
const v = (t, top, up) => (t < up ? (top * t) / up : top);
// Each amount from the same function: the area under it, summed in small steps.
const area = (a, b, top, up) => {
  let s = 0;
  const n = 1000, h = (b - a) / n;
  for (let i = 0; i < n; i++) s += ((v(a + i * h, top, up) + v(a + (i + 1) * h, top, up)) / 2) * h;
  return s;
};
const slots = synabit.layout('#v', 'side');
synabit.controls(slots.controls, [
  { id: 'top', label: 'Vận tốc tối đa', min: 2, max: 30, step: 1, value: 10, unit: 'm/s' },
  { id: 'up', label: 'Thời gian tăng tốc', min: 1, max: 8, step: 0.5, value: 4, unit: 's' },
], ({ top, up }) => {
  const first = area(0, up, top, up), second = area(up, end, top, up);
  synabit.expect('the regions add up to the whole', first + second, area(0, end, top, up), 1e-6);
  synabit.expect('speeding up: ½ · v · t', first, 0.5 * top * up, 1e-3);
  synabit.report({ 'quãng đường (m)': first + second, 'tăng tốc (m)': first, 'chạy đều (m)': second });
  synabit.stats(slots.stats, [
    { label: 'Quãng đường', value: first + second, unit: 'm', digits: 0, note: 'diện tích dưới đường vận tốc' },
  ]);
  synabit.chart(slots.stage, {
    x: { label: 'thời gian', unit: 's', min: 0, max: end },
    y: { label: 'vận tốc', unit: 'm/s' },
    series: [{ name: 'vận tốc', fn: (t) => v(t, top, up), colour: 'text-2' }],
    areas: [
      { name: 'tăng tốc', below: 'vận tốc', from: 0, to: up, label: `${synabit.number(first, 0)} m`, colour: 'c2' },
      { name: 'chạy đều', below: 'vận tốc', from: up, to: end, label: `${synabit.number(second, 0)} m`, colour: 'c3' },
    ],
  });
});
</script>
```
