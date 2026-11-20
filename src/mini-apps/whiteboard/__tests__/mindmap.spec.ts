import { describe, expect, it } from 'vitest';
import { branchLine, branchToRecolour, childrenOf, dropPlan, estimateSize, fromOutline, hiddenByCollapse, moveBranch, neighbour, nextBranchColour, parentsOf, rootOf, tidyTree, toOutline, withBranches } from '../mindmap';
import type { WBEdge, WBNode } from '../boardFile';

const mind = (id: string, label = id, extra: Record<string, any> = {}): WBNode => ({
  id, type: 'mindmap', position: { x: 0, y: 0 }, data: { label, level: 1, ...extra },
});
const link = (source: string, target: string): WBEdge => ({ id: `${source}-${target}`, source, target, type: 'default' });
const size = () => ({ width: 100, height: 40 });

// root ─┬─ a ─┬─ a1
//       │     └─ a2
//       └─ b
const nodes = [mind('root', 'Root', { level: 0 }), mind('a'), mind('a1'), mind('a2'), mind('b')];
const edges = [link('root', 'a'), link('a', 'a1'), link('a', 'a2'), link('root', 'b')];

describe('the tree', () => {
  it('knows children and roots', () => {
    expect(childrenOf(nodes, edges).get('a')).toEqual(['a1', 'a2']);
    expect(rootOf('a2', nodes, edges)).toBe('root');
  });

  it('ignores lines to things that are not mind-map items', () => {
    const shape: WBNode = { id: 's', type: 'shape', position: { x: 0, y: 0 }, data: {} };
    expect(childrenOf([...nodes, shape], [...edges, link('a', 's')]).get('a')).toEqual(['a1', 'a2']);
  });

  it('hides what is under a folded branch', () => {
    const folded = nodes.map((n) => (n.id === 'a' ? { ...n, data: { ...n.data, collapsed: true } } : n));
    expect([...hiddenByCollapse(folded, edges)].sort()).toEqual(['a1', 'a2']);
  });
});

describe('tidyTree', () => {
  const laid = tidyTree('root', nodes, edges, size);

  it('puts children to the right of their parent, the root staying put', () => {
    expect(laid.has('root')).toBe(false);
    expect(laid.get('a')!.x).toBeGreaterThan(100);
    expect(laid.get('a1')!.x).toBeGreaterThan(laid.get('a')!.x + 100);
  });

  it('gives each branch room, so nothing overlaps', () => {
    const ys = ['a1', 'a2', 'b'].map((id) => laid.get(id)!.y).sort((x, y) => x - y);
    expect(ys[1] - ys[0]).toBeGreaterThanOrEqual(40);
    expect(ys[2] - ys[1]).toBeGreaterThanOrEqual(40);
    // a sits between its two children.
    expect(laid.get('a')!.y).toBe((laid.get('a1')!.y + laid.get('a2')!.y) / 2);
  });

  it('lays left-hand branches out to the left', () => {
    const left = nodes.map((n) => (n.id === 'b' ? { ...n, data: { ...n.data, direction: 'left' } } : n));
    expect(tidyTree('root', left, edges, size).get('b')!.x).toBeLessThan(0);
  });

  it('gives a folded branch only its own height', () => {
    const folded = nodes.map((n) => (n.id === 'a' ? { ...n, data: { ...n.data, collapsed: true } } : n));
    const out = tidyTree('root', folded, edges, size);
    expect(out.has('a1')).toBe(false);
    expect(Math.abs(out.get('b')!.y - out.get('a')!.y)).toBe(40 + 16);
  });
});

describe('outlines', () => {
  it('writes a mind map as an indented list', () => {
    expect(toOutline('root', nodes, edges)).toBe('- Root\n  - a\n    - a1\n    - a2\n  - b');
  });

  it('reads an indented list back as a tree', () => {
    const tree = fromOutline('- Root\n  - a\n    - a1\n  - b')!;
    expect(tree.label).toBe('Root');
    expect(tree.children.map((c) => c.label)).toEqual(['a', 'b']);
    expect(tree.children[0].children[0].label).toBe('a1');
  });

  it('puts several top-level items under one root', () => {
    const tree = fromOutline('* one\n* two', 'Ideas')!;
    expect(tree.label).toBe('Ideas');
    expect(tree.children).toHaveLength(2);
  });

  it('is not fooled by a sentence or by prose with one dash', () => {
    expect(fromOutline('- just one line')).toBeNull();
    expect(fromOutline('Some words\n- and a list item\n- another')).toBeNull();
  });
});

describe('the tree’s own lines', () => {
  it('finds a parent by them, not by an arrow drawn in', () => {
    const sticky: WBNode = { id: 's', type: 'sticky', position: { x: 0, y: 0 }, data: {} };
    const parents = parentsOf([...nodes, sticky], [link('s', 'root'), ...edges]);
    expect(parents.has('root')).toBe(false);
    expect(parents.get('a1')).toBe('a');
  });
});

describe('deleting', () => {
  it('takes a mind-map item’s branch with it, and nothing else', () => {
    expect(withBranches(['a'], nodes, edges).sort()).toEqual(['a', 'a1', 'a2']);
    const shape: WBNode = { id: 's', type: 'shape', position: { x: 0, y: 0 }, data: {} };
    expect(withBranches(['s'], [...nodes, shape], [...edges, link('s', 'b')])).toEqual(['s']);
  });
});

describe('branch colours', () => {
  const palette = ['#root', '#one', '#two', '#three'];
  it('gives a new main branch the first colour none has, after the root’s', () => {
    const coloured = nodes.map((n) => ({ ...n, data: { ...n.data, color: n.id === 'root' ? '#root' : n.id === 'a' ? '#one' : '#x' } }));
    expect(nextBranchColour('root', coloured, edges, palette)).toBe('#two');
    expect(nextBranchColour('root', coloured, [], palette)).toBe('#one');
  });
});

/** The map above, laid out: root at 0,0, a and b to its right, a's children further right. */
const laidOut = (): WBNode[] => {
  const at = tidyTree('root', nodes, edges, size);
  return nodes.map((n) => ({ ...n, position: at.get(n.id) ?? { ...n.position }, data: { ...n.data, color: n.id === 'root' ? '#p' : n.id === 'b' ? '#b' : '#a' } }));
};
const put = (ns: WBNode[], id: string, x: number, y: number): WBNode[] => ns.map((n) => (n.id === id ? { ...n, position: { x, y } } : n));

describe('dropping an item', () => {
  it('onto another moves its branch under it, last', () => {
    const ns = laidOut();
    const a1 = ns.find((n) => n.id === 'a1')!;
    const b = ns.find((n) => n.id === 'b')!;
    const plan = dropPlan('a', put(ns, 'a', b.position.x + 10, b.position.y + 5), edges, size);
    expect(plan).toEqual({ parentId: 'b', index: 0, direction: 'right' });
    // Never onto its own branch.
    expect(dropPlan('a', put(ns, 'a', a1.position.x, a1.position.y), edges, size)!.parentId).toBe('root');
  });

  it('elsewhere puts it among its siblings by height, and by side beside a central idea', () => {
    const ns = laidOut();
    const b = ns.find((n) => n.id === 'b')!;
    expect(dropPlan('a', put(ns, 'a', 400, b.position.y + 200), edges, size)).toEqual({ parentId: 'root', index: 1, direction: 'right' });
    expect(dropPlan('a', put(ns, 'a', -400, 0), edges, size)!.direction).toBe('left');
  });

  it('leaves a central idea alone: dragging it moves the map', () => {
    expect(dropPlan('root', laidOut(), edges, size)).toBeNull();
  });
});

describe('moving a branch', () => {
  const copy = () => ({ ns: laidOut(), es: edges.map((e) => ({ ...e })) });

  it('re-hangs it, levels and colours its items, and sides its lines', () => {
    const { ns, es } = copy();
    const changed = moveBranch('a', { parentId: 'b', index: 0, direction: 'right' }, ns, es, ['#p', '#a', '#b', '#c']);
    expect(childrenOf(ns, es).get('b')).toEqual(['a']);
    expect(childrenOf(ns, es).get('root')).toEqual(['b']);
    const a1 = ns.find((n) => n.id === 'a1')!;
    expect(a1.data.level).toBe(3);
    expect(a1.data.color).toBe('#b');
    expect(changed.nodes.map((n) => n.id).sort()).toEqual(['a', 'a1', 'a2']);
  });

  it('reorders siblings by the order of their lines', () => {
    const { ns, es } = copy();
    moveBranch('b', { parentId: 'root', index: 0, direction: 'right' }, ns, es, ['#p', '#a', '#b']);
    expect(childrenOf(ns, es).get('root')).toEqual(['b', 'a']);
    expect(ns.find((n) => n.id === 'b')!.data.color).toBe('#b');
  });

  it('turns a branch to the left side, its lines with it', () => {
    const { ns, es } = copy();
    moveBranch('a', { parentId: 'root', index: 1, direction: 'left' }, ns, es, ['#p', '#a', '#b']);
    expect(ns.find((n) => n.id === 'a2')!.data.direction).toBe('left');
    expect(es.find((e) => e.target === 'a2')!.sourceHandle).toBe('left-source');
    expect(es.find((e) => e.target === 'a')!.targetHandle).toBe('right-target');
  });

  it('changes nothing when dropped back where it was', () => {
    const { ns, es } = copy();
    expect(moveBranch('a', { parentId: 'root', index: 0, direction: 'right' }, ns, es, ['#p']).nodes).toEqual([]);
  });

  it('gives a branch moved up to a central idea a colour of its own', () => {
    const { ns, es } = copy();
    moveBranch('a1', { parentId: 'root', index: 2, direction: 'right' }, ns, es, ['#p', '#a', '#b', '#c']);
    expect(ns.find((n) => n.id === 'a1')!.data.color).toBe('#c');
    expect(ns.find((n) => n.id === 'a1')!.data.level).toBe(1);
  });
});

describe('relationships', () => {
  const related = [...edges, { ...link('a1', 'b'), data: { relation: true } }];
  it('are not branches: no parent, no child, no part of a branch', () => {
    expect(childrenOf(nodes, related).get('a1')).toBeUndefined();
    expect(parentsOf(nodes, related).get('b')).toBe('root');
    expect(withBranches(['a'], nodes, related).sort()).toEqual(['a', 'a1', 'a2']);
  });
  it('are drawn as lines of their own, branches in their colour', () => {
    const coloured = laidOut();
    const nodeOf = (id: string) => coloured.find((n) => n.id === id);
    expect(branchLine(related[related.length - 1], nodeOf)).toBeNull();
    expect(branchLine(link('root', 'a'), nodeOf)).toEqual({ color: '#a', width: 4 });
    expect(branchLine(link('a', 'a1'), nodeOf)!.width).toBe(4);
  });
});

describe('placed by hand', () => {
  it('is left where it was, its siblings closing up and its own branch laid out from it', () => {
    const pinned = nodes.map((n) => (n.id === 'a' ? { ...n, position: { x: 500, y: 300 }, data: { ...n.data, pinned: true } } : n));
    const out = tidyTree('root', pinned, edges, size);
    expect(out.has('a')).toBe(false);
    expect(out.get('a1')!.x).toBe(500 + 100 + 64);
    // b alone in the root's column now, level with it.
    expect(out.get('b')!.y).toBe(0);
  });
});

describe('the keyboard', () => {
  const ns = laidOut();
  it('goes out to a child, in to the parent, and along among siblings', () => {
    expect(neighbour('root', 'ArrowRight', ns, edges)).not.toBeNull();
    expect(neighbour('a', 'ArrowRight', ns, edges)).toMatch(/^a[12]$/);
    expect(neighbour('a1', 'ArrowLeft', ns, edges)).toBe('a');
    expect(neighbour('a1', 'ArrowDown', ns, edges)).toBe('a2');
    expect(neighbour('a2', 'ArrowDown', ns, edges)).toBeNull();
    expect(neighbour('root', 'ArrowLeft', ns, edges)).toBeNull();
  });
  it('goes the other way round on the left', () => {
    const left = ns.map((n) => (n.id === 'b' ? { ...n, data: { ...n.data, direction: 'left' } } : n));
    expect(neighbour('root', 'ArrowLeft', left, edges)).toBe('b');
    expect(neighbour('b', 'ArrowRight', left, edges)).toBe('root');
  });
  it('stops at a folded item', () => {
    const folded = ns.map((n) => (n.id === 'a' ? { ...n, data: { ...n.data, collapsed: true } } : n));
    expect(neighbour('a', 'ArrowRight', folded, edges)).toBeNull();
  });
});

describe('recolouring', () => {
  it('takes the branch, not an item coloured apart from it', () => {
    const ns = laidOut().map((n) => (n.id === 'a2' ? { ...n, data: { ...n.data, color: '#own' } } : n));
    expect(branchToRecolour('a', ns, edges).sort()).toEqual(['a', 'a1']);
  });
});

describe('a label of several lines', () => {
  it('is guessed taller', () => {
    expect(estimateSize(mind('x', 'one\ntwo\nthree')).height).toBeGreaterThan(estimateSize(mind('x', 'one')).height + 30);
  });
});
