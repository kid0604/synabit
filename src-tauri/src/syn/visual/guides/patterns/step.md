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
- Built with `synabit.stepper` (the buttons, "step k / n", the step line)
  and, for a graph, `synabit.graph` drawn from each step's state: node and
  edge states, notes such as distances, a legend that names each state.

Copy how this is built, not what it shows:

```html
<div id="v"></div>
<script>
// The graph, and every step of the search, worked out before anything is drawn.
const nodes = [{ id: 'S', x: 0, y: 1 }, { id: 'A', x: 1, y: 0 }, { id: 'B', x: 1, y: 2 }, { id: 'C', x: 2, y: 0 }, { id: 'D', x: 2, y: 2 }, { id: 'E', x: 3, y: 1 }];
const edges = [['S', 'A'], ['S', 'B'], ['A', 'C'], ['B', 'D'], ['C', 'E'], ['D', 'E'], ['A', 'B']].map(([from, to]) => ({ from, to }));
const near = (u) => edges.flatMap((e) => (e.from === u ? [e.to] : e.to === u ? [e.from] : []));

const dist = { S: 0 };
const done = [];
const queue = ['S'];
const tree = {};
const snap = (line, current) => ({
  line,
  nodes: Object.fromEntries(nodes.map((n) => [n.id, n.id === current ? 'current' : done.includes(n.id) ? 'done' : n.id in dist ? 'seen' : 'todo'])),
  edges: Object.fromEntries(Object.entries(tree).map(([v, u]) => [`${u}-${v}`, 'chosen'])),
  notes: Object.fromEntries(Object.entries(dist).map(([k, d]) => [k, `d = ${d}`])),
  queue: [...queue],
});
const steps = [snap('Bắt đầu từ S: d(S) = 0, hàng đợi [S]', null)];
while (queue.length) {
  const u = queue.shift();
  const added = near(u).filter((v) => !(v in dist));
  for (const v of added) {
    dist[v] = dist[u] + 1;
    tree[v] = u;
    queue.push(v);
  }
  done.push(u);
  steps.push(snap(added.length ? `Lấy ${u}: thêm ${added.join(', ')} với d = ${dist[u]} + 1 = ${dist[u] + 1}` : `Lấy ${u}: không có đỉnh mới`, u));
}
// Checked: along every edge, the distances differ by at most one.
for (const e of edges) synabit.expect(`${e.from}-${e.to} differ by at most 1`, Math.abs(dist[e.from] - dist[e.to]) <= 1, true);
synabit.report(Object.fromEntries(Object.entries(dist).map(([k, d]) => [`d(${k})`, d])));

synabit.stepper('#v', {
  steps,
  render: (step, i, stage) => {
    synabit.graph(stage, {
      nodes, edges, directed: false,
      state: { nodes: step.nodes, edges: step.edges, notes: step.notes },
      legend: { done: 'đã duyệt', current: 'đang duyệt', seen: 'trong hàng đợi', todo: 'chưa thấy', chosen: 'cạnh của cây BFS' },
    });
    const q = document.createElement('div');
    q.className = 'muted';
    q.textContent = `Hàng đợi: [${step.queue.join(', ')}]`;
    stage.appendChild(q);
  },
});
</script>
```
