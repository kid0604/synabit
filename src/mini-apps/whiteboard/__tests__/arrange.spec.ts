import { describe, expect, it } from 'vitest';
import { alignBoxes, distributeBoxes, parseClip, rekeyClip, CLIPBOARD_KIND } from '../composables/useArrange';
import { nearestSnap } from '../composables/useSmartGuides';
import type { WBEdge, WBNode } from '../boardFile';

const box = (id: string, x: number, y: number, width = 100, height = 50) => ({ id, x, y, width, height });

describe('alignBoxes', () => {
  const boxes = [box('a', 10, 0), box('b', 50, 30, 40, 20), box('c', 200, 100, 60, 80)];

  it('lines up left and right edges', () => {
    expect([...alignBoxes(boxes, 'left').values()].map((p) => p.x)).toEqual([10, 10, 10]);
    const right = alignBoxes(boxes, 'right');
    expect(right.get('a')!.x + 100).toBe(260);
    expect(right.get('b')!.x + 40).toBe(260);
  });

  it('centres on the middle of the selection', () => {
    const centred = alignBoxes(boxes, 'centerY');
    // Selection spans 0..180, middle 90.
    expect(centred.get('a')!.y + 25).toBe(90);
    expect(centred.get('c')!.y + 40).toBe(90);
  });

  it('leaves the other axis alone', () => {
    const top = alignBoxes(boxes, 'top');
    expect([...top.values()].map((p) => p.x)).toEqual([10, 50, 200]);
  });
});

describe('distributeBoxes', () => {
  it('makes the gaps equal and keeps the outermost two in place', () => {
    const boxes = [box('a', 0, 0, 100), box('c', 400, 0, 100), box('b', 130, 0, 100)];
    const out = distributeBoxes(boxes, 'x');
    expect(out.get('a')!.x).toBe(0);
    expect(out.get('c')!.x).toBe(400);
    // Span 500, filled 300, two gaps of 100.
    expect(out.get('b')!.x).toBe(200);
  });
});

describe('copying items', () => {
  const nodes: WBNode[] = [
    { id: 'a', type: 'shape', position: { x: 0, y: 0 }, data: { groupId: 'g1', editing: true } },
    { id: 'b', type: 'shape', position: { x: 50, y: 0 }, data: { groupId: 'g1' } },
  ];
  const edges: WBEdge[] = [
    { id: 'e1', source: 'a', target: 'b', type: 'default' },
    { id: 'e2', source: 'a', target: 'not-copied', type: 'default' },
  ];
  let n = 0;
  const gen = (p: string) => `${p}-${++n}`;

  it('gives the copies new ids and keeps only edges between them', () => {
    const out = rekeyClip({ nodes, edges }, gen);
    const ids = out.nodes.map((x) => x.id);
    expect(ids.some((id) => id === 'a' || id === 'b')).toBe(false);
    expect(out.edges).toHaveLength(1);
    expect(out.edges[0].source).toBe(ids[0]);
    expect(out.edges[0].target).toBe(ids[1]);
  });

  it('makes a copied group its own group, and leaves no node in edit mode', () => {
    const out = rekeyClip({ nodes, edges }, gen);
    expect(out.nodes[0].data.groupId).toBe(out.nodes[1].data.groupId);
    expect(out.nodes[0].data.groupId).not.toBe('g1');
    expect(out.nodes[0].data.editing).toBeUndefined();
    // The originals are untouched.
    expect(nodes[0].data.groupId).toBe('g1');
  });

  it('recognises its own clipboard text and nothing else', () => {
    const text = JSON.stringify({ kind: CLIPBOARD_KIND, version: 1, nodes, edges });
    expect(parseClip(text)?.nodes).toHaveLength(2);
    expect(parseClip('{"kind":"other","nodes":[]}')).toBeNull();
    expect(parseClip('just words')).toBeNull();
    expect(parseClip('{not json')).toBeNull();
  });
});

describe('nearestSnap', () => {
  it('picks the closest line within reach', () => {
    expect(nearestSnap([100, 150, 200], [104, 152, 300], 6)).toEqual({ delta: 2, at: 152 });
  });
  it('picks nothing out of reach', () => {
    expect(nearestSnap([100], [110], 6)).toBeNull();
  });
});

describe('copying what points at other items', () => {
  let n = 0;
  const gen = (p: string) => `${p}-${++n}`;

  it('points a copied comment at the copied item, and lets go of one whose item was not copied', () => {
    const out = rekeyClip({
      nodes: [
        { id: 's', type: 'sticky', position: { x: 0, y: 0 }, data: { label: 'x' } },
        { id: 'c1', type: 'comment', position: { x: 0, y: 0 }, data: { label: 'on s', on: 's' } },
        { id: 'c2', type: 'comment', position: { x: 0, y: 0 }, data: { label: 'on other', on: 'elsewhere' } },
      ],
      edges: [],
    }, gen);
    expect(out.nodes[1].data.on).toBe(out.nodes[0].id);
    expect('on' in out.nodes[2].data).toBe(false);
  });

  it('gives copied live cards to the copied frame', () => {
    const out = rekeyClip({
      nodes: [
        { id: 'f', type: 'frame', position: { x: 0, y: 0 }, data: { query: 'is:task' } },
        { id: 'k', type: 'card', position: { x: 0, y: 0 }, data: { ref: 'Tasks/a.md', fromQuery: 'f' } },
      ],
      edges: [],
    }, gen);
    expect(out.nodes[1].data.fromQuery).toBe(out.nodes[0].id);
  });
});
