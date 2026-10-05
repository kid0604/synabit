import { describe, expect, it } from 'vitest';
import { handedToCanvas, inkInside, isLooseInk, onCanvas, PROMOTE_LIMIT, strokeSize, unionBox, viewBox, withLooseInk } from '../inkLayer';
import { readBoardFile } from '../boardFile';

const ink = (id: string, x: number, y: number, extra: Record<string, any> = {}) => ({
  id, type: 'stroke', position: { x, y }, data: { svgPath: 'M0 0', width: 50, height: 20, ...extra },
});

describe('which items the canvas is handed', () => {
  it('hands it everything but loose ink', () => {
    expect(onCanvas({ id: 's', type: 'sticky', position: { x: 0, y: 0 } })).toBe(true);
    expect(onCanvas(ink('a', 0, 0))).toBe(false);
  });

  it('hands it ink that is selected, or stacked somewhere of its own', () => {
    expect(onCanvas({ ...ink('a', 0, 0), selected: true })).toBe(true);
    expect(onCanvas(ink('a', 0, 0, { z: 5 }))).toBe(true);
    expect(isLooseInk(ink('a', 0, 0, { z: 0 }))).toBe(false);
  });

  it('keeps the ink the canvas never saw when the canvas reports its list', () => {
    const before = [{ id: 's', type: 'sticky', position: { x: 0, y: 0 } }, ink('a', 0, 0), ink('b', 0, 0)];
    const fromCanvas = [{ id: 's', type: 'sticky', position: { x: 9, y: 9 } }, { ...ink('b', 4, 4), selected: false }];
    const after = withLooseInk(fromCanvas, before, new Set(['s', 'b']));
    expect(after.map((n) => n.id)).toEqual(['s', 'b', 'a']);
    // The canvas's copy wins over the old one.
    expect(after[1].position).toEqual({ x: 4, y: 4 });
  });
});

describe('a large selection of ink', () => {
  it('stays on the ink layer past the limit, and goes to the canvas below it', () => {
    const many = Array.from({ length: PROMOTE_LIMIT + 1 }, (_, i) => ({ ...ink(`s${i}`, 0, 0), selected: true }));
    expect(handedToCanvas(many)).toEqual([]);
    expect(handedToCanvas(many.slice(0, PROMOTE_LIMIT))).toHaveLength(PROMOTE_LIMIT);
  });

  it('treats what the canvas was handed and lost as deleted, and keeps what it was never handed', () => {
    const before = [ink('kept', 0, 0), { ...ink('gone', 0, 0), selected: true }];
    expect(withLooseInk([], before, new Set(['gone'])).map((n) => n.id)).toEqual(['kept']);
  });
});

describe('measuring ink', () => {
  it('uses the saved box, or measures one from the points and the pen', () => {
    expect(strokeSize({ width: 50, height: 20 })).toEqual({ width: 50, height: 20 });
    expect(strokeSize({ points: [[0, 0], [100, 40]], size: 8 })).toEqual({ width: 104, height: 44 });
  });

  it('gives old strokes their box when the board is read', () => {
    const raw = JSON.stringify({ schemaVersion: 1, nodes: [{ id: 'a', type: 'stroke', position: { x: 0, y: 0 }, data: { points: [[0, 0], [30, 10]], size: 4 } }], edges: [] });
    const read = readBoardFile(raw);
    expect(read.ok && read.data.nodes[0].data).toMatchObject({ width: 32, height: 12 });
  });

  it('works out what is in view, wider by a margin, at any zoom', () => {
    expect(viewBox({ x: -100, y: -50, zoom: 2 }, { width: 800, height: 600 }, 0)).toEqual({ x: 50, y: 25, width: 400, height: 300 });
  });

  it('selects with a marquee only what is wholly inside it', () => {
    const all = [ink('in', 10, 10), ink('half', 90, 10), ink('out', 500, 500)];
    expect(inkInside(all, { x: 0, y: 0, width: 100, height: 100 }).map((n) => n.id)).toEqual(['in']);
  });

  it('boxes several boxes', () => {
    expect(unionBox([{ x: 0, y: 0, width: 10, height: 10 }, { x: 20, y: -5, width: 5, height: 5 }])).toEqual({ x: 0, y: -5, width: 25, height: 15 });
    expect(unionBox([])).toBeNull();
  });
});

describe('reading a board file', () => {
  it('reads a position that is not two numbers as the nearest one that is', () => {
    const raw = JSON.stringify({ schemaVersion: 1, nodes: [
      { id: 'a', type: 'shape', position: { x: '12', y: 'nope' }, data: {} },
      { id: 'b', type: 'shape', data: {} },
    ], edges: [] });
    const read = readBoardFile(raw);
    expect(read.ok && read.data.nodes.map((n) => n.position)).toEqual([{ x: 12, y: 0 }, { x: 0, y: 0 }]);
  });
});
