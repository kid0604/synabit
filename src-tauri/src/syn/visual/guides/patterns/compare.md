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
- Built with `synabit.layout(el, 'compare')`, a `synabit.chart` in each half
  given the same `x` and `y` ranges, and the measure in each half's
  `synabit.stats`.

Copy how this is built, not what it shows:

```html
<div id="v"></div>
<script>
const cases = [{ name: 'Trái Đất', g: 9.81 }, { name: 'Mặt Trăng', g: 1.62 }];
// The model, once: height after t seconds, and when it reaches the ground.
const height = (h, g, t) => h - 0.5 * g * t * t;
const landing = (h, g) => Math.sqrt((2 * h) / g);
const slots = synabit.layout('#v', 'compare');
const halves = [[slots.a, slots.aStats], [slots.b, slots.bStats]];

synabit.controls(slots.controls, [
  { id: 'h', label: 'Độ cao thả', min: 5, max: 50, step: 5, value: 20, unit: 'm', help: 'Cùng một độ cao cho cả hai nơi' },
], ({ h }) => {
  // One scale for both: the slowest case sets the time axis.
  const tMax = Math.max(...cases.map((c) => landing(h, c.g)));
  cases.forEach((c, i) => {
    const t1 = landing(h, c.g);
    synabit.expect(`${c.name}: on the ground at landing`, height(h, c.g, t1), 0, 1e-9);
    const [stage, stats] = halves[i];
    synabit.stats(stats, [{ label: `${c.name}: chạm đất sau`, value: t1, unit: 's', digits: 2, note: `√(2h/g), g = ${synabit.number(c.g, 2)} m/s²` }]);
    synabit.chart(stage, {
      x: { label: 'thời gian', unit: 's', min: 0, max: tMax },
      y: { label: 'độ cao', unit: 'm', min: 0, max: h },
      series: [{ name: c.name, fn: (t) => (t <= t1 ? height(h, c.g, t) : NaN), colour: `c${i + 1}` }],
      marks: [{ x: t1, y: 0, label: `${synabit.number(t1, 2)} s`, colour: `c${i + 1}` }],
    });
  });
  synabit.report(Object.fromEntries(cases.map((c) => [`${c.name}: thời gian chạm đất (s)`, landing(h, c.g)])));
});
</script>
```
