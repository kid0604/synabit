# Pattern: step

For an algorithm or a procedure the person wants to follow, step by step.

Must have:
- **Step 0 is the starting state**, before anything is done, and it agrees
  with everything else on screen.
- **Every item has a state, and each state has a colour named in a legend**:
  done, current, still to come. The thing that just changed is marked.
- **A step line that shows the arithmetic** of the step, not only its name:
  "13 = 2 · 6 + 1", "min(4, 2 + 1) = 3".
- Back / next buttons and "step k / n". "Back" at step 0 and "next" at the
  last step are disabled.
- All steps worked out once, from the algorithm, before anything is drawn;
  the buttons only choose which one to show. The result is checked with
  `synabit.expect` against what it must equal.

Copy how this is built, not what it shows:

```html
<div style="max-width:640px">
  <div class="row" style="margin-bottom:10px">
    <button id="back">Lùi</button>
    <button id="next">Tiếp</button>
    <span id="where" class="muted"></span>
  </div>
  <div id="line" style="font:15px var(--mono);margin-bottom:10px"></div>
  <table style="border-collapse:collapse;font-variant-numeric:tabular-nums">
    <thead><tr class="muted"><th style="text-align:left;padding:4px 12px 4px 0">Số bị chia</th><th style="padding:4px 12px">Thương</th><th style="padding:4px 12px">Dư</th></tr></thead>
    <tbody id="rows"></tbody>
  </table>
  <div class="row muted" style="margin-top:10px;font-size:12px;gap:14px">
    <span><b style="color:var(--success)">■</b> đã xong</span>
    <span><b style="color:var(--accent)">■</b> đang làm</span>
    <span><b style="color:var(--border)">■</b> chưa tới</span>
  </div>
  <div style="margin-top:10px">Nhị phân: <span id="bits" class="stat"></span></div>
</div>
<script>
const n = 13;
// Every step, worked out once from the algorithm.
const steps = [];
for (let q = n; q > 0; q = Math.floor(q / 2)) steps.push({ q, next: Math.floor(q / 2), r: q % 2 });
let at = 0; // 0 is the start: nothing divided yet.

function draw() {
  document.getElementById('where').textContent = `Bước ${at} / ${steps.length}`;
  document.getElementById('back').disabled = at === 0;
  document.getElementById('next').disabled = at === steps.length;
  const s = steps[at - 1];
  document.getElementById('line').textContent = at === 0
    ? `Bắt đầu: chia ${n} cho 2 tới khi thương bằng 0`
    : `${s.q} = 2 · ${s.next} + ${s.r}`;
  const rows = document.getElementById('rows');
  rows.replaceChildren();
  steps.forEach((step, i) => {
    const state = i < at - 1 ? 'done' : i === at - 1 ? 'current' : 'todo';
    const tr = document.createElement('tr');
    tr.style.color = state === 'todo' ? 'var(--muted)' : 'var(--text)';
    tr.style.background = state === 'current' ? 'color-mix(in srgb, var(--accent) 12%, transparent)' : '';
    tr.style.borderLeft = `3px solid var(--${state === 'done' ? 'success' : state === 'current' ? 'accent' : 'border'})`;
    for (const value of [step.q, i < at ? step.next : '·', i < at ? step.r : '·']) {
      const td = document.createElement('td');
      td.style.padding = '4px 12px';
      td.textContent = typeof value === 'number' ? synabit.number(value, 0) : value;
      tr.appendChild(td);
    }
    rows.appendChild(tr);
  });
  // The remainders, read from the last to the first.
  const bits = steps.slice(0, at).map((s) => s.r).reverse().join('');
  document.getElementById('bits').textContent = at === steps.length ? bits : bits ? `…${bits}` : '—';
  if (at === steps.length) synabit.expect('read back', parseInt(bits, 2), n);
}
document.getElementById('back').addEventListener('click', () => { at = Math.max(0, at - 1); draw(); });
document.getElementById('next').addEventListener('click', () => { at = Math.min(steps.length, at + 1); draw(); });
const all = steps.map((s) => s.r).reverse().join('');
synabit.report({ 'nhị phân của 13': all });
synabit.expect('13 in binary, read back', parseInt(all, 2), n);
draw();
</script>
```
