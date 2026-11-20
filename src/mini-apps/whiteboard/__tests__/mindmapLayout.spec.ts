import { describe, expect, it } from 'vitest';
import {
  buildMindmap, dropPlan, estimateSize, fixHandles, fromOutline, layoutOf, mapLayouts, MINDMAP_COLORS, neighbour,
  setLayout, tagsOf, taskProgress, tidyTree, toOutline, type OutlineItem,
} from '../mindmap';
import type { WBNode } from '../boardFile';

let n = 0;
const newId = (prefix: string) => `${prefix}_${++n}`;
const leaf = (label: string, extra: Partial<OutlineItem> = {}): OutlineItem => ({ label, children: [], ...extra });

/** A map with five main branches, laid out. */
function map() {
  const tree: OutlineItem = { label: 'Root', children: ['A', 'B', 'C', 'D', 'E'].map((l) => ({ label: l, children: [leaf(`${l}1`), leaf(`${l}2`)] })) };
  const built = buildMindmap(tree, { x: 0, y: 0 }, MINDMAP_COLORS, newId);
  const by = (label: string) => built.nodes.find((x) => x.data.label === label)!;
  const lay = () => {
    const placed = tidyTree(built.rootId, built.nodes, built.edges, estimateSize);
    for (const x of built.nodes) { const p = placed.get(x.id); if (p) x.position = p; }
  };
  return { ...built, by, lay };
}

describe('layouts', () => {
  it('lays an org chart out downwards, rows under their parents', () => {
    const m = map();
    setLayout(m.rootId, 'down', m.nodes, m.edges);
    m.lay();
    expect(layoutOf(m.rootId, m.nodes)).toBe('down');
    const root = m.by('Root');
    const row = ['A', 'B', 'C', 'D', 'E'].map(m.by);
    expect(new Set(row.map((x) => x.position.y)).size).toBe(1);
    expect(row[0].position.y).toBeGreaterThan(root.position.y);
    for (let i = 1; i < row.length; i++) {
      expect(row[i].position.x).toBeGreaterThanOrEqual(row[i - 1].position.x + estimateSize(row[i - 1]).width);
    }
    // Centred: as far out on the left as on the right.
    const mid = root.position.x + estimateSize(root).width / 2;
    const left = mid - row[0].position.x;
    const right = row[4].position.x + estimateSize(row[4]).width - mid;
    expect(Math.abs(left - right)).toBeLessThan(2);
    expect(m.by('A1').position.y).toBeGreaterThan(m.by('A').position.y);
    // Joined bottom to top.
    expect(m.edges.every((e) => e.sourceHandle === 'bottom-source' && e.targetHandle === 'top-target')).toBe(true);
  });

  it('puts every branch on the right for a logic chart, and shares them out again for a map', () => {
    const m = map();
    setLayout(m.rootId, 'right', m.nodes, m.edges);
    expect(m.nodes.filter((x) => x.data.level).every((x) => x.data.direction === 'right')).toBe(true);
    setLayout(m.rootId, 'map', m.nodes, m.edges);
    expect(m.by('Root').data.layout).toBeUndefined();
    expect(['A', 'B', 'C'].map((l) => m.by(l).data.direction)).toEqual(['right', 'right', 'right']);
    expect(['D', 'E', 'E1'].map((l) => m.by(l).data.direction)).toEqual(['left', 'left', 'left']);
    const toD = m.edges.find((e) => e.target === m.by('D').id)!;
    expect(toD.sourceHandle).toBe('left-source');
  });

  it('takes items placed by hand back into the layout when it changes', () => {
    const m = map();
    m.by('A').data.pinned = true;
    setLayout(m.rootId, 'down', m.nodes, m.edges);
    expect(m.by('A').data.pinned).toBeUndefined();
  });

  it('mends a line that joins by the wrong sides', () => {
    const m = map();
    setLayout(m.rootId, 'down', m.nodes, m.edges);
    const line = m.edges[0];
    line.sourceHandle = 'right-source';
    expect(fixHandles(m.rootId, m.nodes, m.edges)).toEqual([line]);
    expect(line.sourceHandle).toBe('bottom-source');
  });

  it('knows each item’s layout from its map', () => {
    const m = map();
    setLayout(m.rootId, 'down', m.nodes, m.edges);
    expect(mapLayouts(m.nodes, m.edges).get(m.by('C2').id)).toBe('down');
  });

  it('goes by arrows down, up and along a row in an org chart', () => {
    const m = map();
    setLayout(m.rootId, 'down', m.nodes, m.edges);
    m.lay();
    expect(neighbour(m.by('A').id, 'ArrowDown', m.nodes, m.edges)).toBe(m.by('A1').id);
    expect(neighbour(m.by('A1').id, 'ArrowUp', m.nodes, m.edges)).toBe(m.by('A').id);
    expect(neighbour(m.by('B').id, 'ArrowRight', m.nodes, m.edges)).toBe(m.by('C').id);
    expect(neighbour(m.by('B').id, 'ArrowLeft', m.nodes, m.edges)).toBe(m.by('A').id);
  });

  it('places a dropped item along the row it is dropped in', () => {
    const m = map();
    setLayout(m.rootId, 'down', m.nodes, m.edges);
    m.lay();
    const e = m.by('E');
    // Dropped between A and B, below them where nothing is.
    e.position = { x: m.by('B').position.x - 30, y: m.by('A').position.y + 200 };
    expect(dropPlan(e.id, m.nodes, m.edges, estimateSize)).toEqual({ parentId: m.rootId, index: 1, direction: 'right' });
  });
});

describe('tasks, marks and tags', () => {
  it('counts the tasks done under each idea, every level down', () => {
    const tree: OutlineItem = {
      label: 'Plan',
      children: [
        { label: 'Venue', children: [leaf('Book hall', { task: true, done: true }), leaf('Pay deposit', { task: true })] },
        { label: 'Food', task: true, children: [leaf('Menu', { task: true, done: true })] },
        leaf('Notes'),
      ],
    };
    const m = buildMindmap(tree, { x: 0, y: 0 }, MINDMAP_COLORS, newId);
    const by = (l: string) => m.nodes.find((x) => x.data.label === l)!.id;
    const p = taskProgress(m.nodes, m.edges);
    expect(p.get(by('Venue'))).toEqual({ done: 1, total: 2 });
    // A task with tasks under it counts as they do, not twice.
    expect(p.get(by('Food'))).toEqual({ done: 1, total: 1 });
    expect(p.get(by('Plan'))).toEqual({ done: 2, total: 3 });
    expect(p.has(by('Notes'))).toBe(false);
  });

  it('keeps ticks and marks through an outline, as a note writes a checklist; a mark typed first in the words is read as one', () => {
    const tree: OutlineItem = { label: 'Trip', children: [leaf('Passport', { task: true, done: true, icon: '❗' }), leaf('Tickets', { task: true }), leaf('💡 Visa')] };
    const m = buildMindmap(tree, { x: 0, y: 0 }, MINDMAP_COLORS, newId);
    const text = toOutline(m.rootId, m.nodes, m.edges);
    expect(text).toBe('- Trip\n  - [x] ❗ Passport\n  - [ ] Tickets\n  - 💡 Visa');
    const back = fromOutline(text)!;
    expect(back.children[0]).toMatchObject({ label: 'Passport', task: true, done: true, icon: '❗' });
    expect(back.children[1]).toMatchObject({ label: 'Tickets', task: true });
    expect(back.children[1].done).toBeUndefined();
    expect(back.children[2]).toMatchObject({ icon: '💡', label: 'Visa' });
  });

  it('takes #tags out of the words, once each', () => {
    expect(tagsOf('Book the hall #venue #urgent #venue')).toEqual({ text: 'Book the hall', tags: ['venue', 'urgent'] });
    expect(tagsOf('Phòng #địa-điểm')).toEqual({ text: 'Phòng', tags: ['địa-điểm'] });
    expect(tagsOf('C# is a language').tags).toEqual([]);
  });

  it('makes room for a box, a mark and tags', () => {
    const plain: WBNode = { id: 'a', type: 'mindmap', position: { x: 0, y: 0 }, data: { label: 'Book the hall', level: 1 } };
    const marked: WBNode = { ...plain, data: { ...plain.data, task: true, icon: '⭐', label: 'Book the hall #venue' } };
    expect(estimateSize(marked).width).toBeGreaterThan(estimateSize(plain).width);
    expect(estimateSize(marked).height).toBeGreaterThan(estimateSize(plain).height);
  });
});
