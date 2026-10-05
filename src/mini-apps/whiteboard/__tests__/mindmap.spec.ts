import { describe, expect, it } from 'vitest';
import { childrenOf, fromOutline, hiddenByCollapse, rootOf, tidyTree, toOutline } from '../mindmap';
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
