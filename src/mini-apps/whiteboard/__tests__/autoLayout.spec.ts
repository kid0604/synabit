import { describe, it, expect } from 'vitest';
import type { WBEdge, WBNode } from '../boardFile';
import { layOutPending, shareAt, waitsForLayout } from '../autoLayout';
import { drawnPath, labelSize, outlineOf, placeLabels, pointAt, runsThrough, type Outline, type Route } from '../autoRoute';
import flowchart from './fixtures/elk-flowchart.json';
import erd from './fixtures/elk-erd.json';
import dfd from './fixtures/elk-dfd.json';

/**
 * Boards Syn drew with nothing to say about their shape, laid out again by
 * ELK when opened (autoLayout.ts). The fixtures are what `draw` in board.rs
 * writes for a real flowchart, a schema of four tables and a data flow.
 */

type Board = { nodes: WBNode[]; edges: WBEdge[] };
const copy = (b: unknown): Board => JSON.parse(JSON.stringify(b));
const byLabel = (b: Board, label: string) => b.nodes.find((n) => n.data.label === label)!;

const boxesOf = (b: Board) => {
  const boxes = new Map<string, Outline>();
  for (const n of b.nodes) if (n.type !== 'frame') boxes.set(n.id, outlineOf(n)!);
  return boxes;
};
const routeOfLine = (e: WBEdge): Route => ({
  waypoints: e.data!.waypoints,
  sourceAt: e.data!.sourceAt,
  targetAt: e.data!.targetAt,
  labelAt: e.data!.labelAt,
});

/** Every line routed, none through a box, nothing left waiting. */
function expectLaidOut(b: Board) {
  expect(b.nodes.some(waitsForLayout)).toBe(false);
  const boxes = boxesOf(b);
  for (const e of b.edges) {
    expect(e.data?.routed, e.id).toBe(true);
    expect(e.data?.routing, e.id).toBeUndefined();
    const path = drawnPath(routeOfLine(e), boxes.get(e.source)!, boxes.get(e.target)!);
    expect(runsThrough(path, boxes, e), e.id).toBe(false);
  }
}

/** No two of these overlap. */
function expectApart(nodes: WBNode[]) {
  for (const [i, a] of nodes.entries()) {
    for (const b of nodes.slice(i + 1)) {
      const apart =
        a.position.x + a.data.width <= b.position.x ||
        b.position.x + b.data.width <= a.position.x ||
        a.position.y + a.data.height <= b.position.y ||
        b.position.y + b.data.height <= a.position.y;
      expect(apart, `${a.data.label} / ${b.data.label}`).toBe(true);
    }
  }
}

describe('a flowchart', () => {
  it('is laid out, every line routed around the boxes', async () => {
    const b = copy(flowchart);
    expect(await layOutPending(b)).toBe(true);
    expectLaidOut(b);
    expectApart(b.nodes);
  });

  /** ELK keeps Syn's order: a step a question sends back to stays above it. */
  it('keeps a loop back a loop back', async () => {
    const b = copy(flowchart);
    await layOutPending(b);
    const y = (label: string) => byLabel(b, label).position.y;
    expect(y('Bắt đầu')).toBeLessThan(y('Gửi Change Request'));
    expect(y('Gửi Change Request')).toBeLessThan(y('Thông tin đầy đủ?'));
    expect(y('Thông tin đầy đủ?')).toBeLessThan(y('Bổ sung thông tin'));
    expect(y('Cập nhật hồ sơ, CMDB và đóng Change')).toBeLessThan(y('Kết thúc'));
  });

  /** Two answers out of one corner of a question ran down one another. */
  it('sends each answer of a question out of a corner of its own', async () => {
    const b = copy(flowchart);
    await layOutPending(b);
    const questions = b.nodes.filter((n) => n.data.shapeType === 'diamond');
    expect(questions.length).toBeGreaterThan(0);
    for (const q of questions) {
      const out = b.edges.filter((e) => e.source === q.id).map((e) => e.data!.sourceAt);
      expect(new Set(out.map((at) => at.side)).size, q.data.label).toBe(out.length);
      // At the corner, the middle of the side.
      for (const at of out) expect(at.t).toBe(0.5);
    }
  });

  it('stays where it started on the board', async () => {
    const b = copy(flowchart);
    const corner = (x: Board) => ({
      x: Math.min(...x.nodes.map((n) => n.position.x)),
      y: Math.min(...x.nodes.map((n) => n.position.y)),
    });
    const before = corner(b);
    await layOutPending(b);
    expect(corner(b)).toEqual(before);
  });
});

describe('tables', () => {
  it('are laid out across, each line from the row it names to the row it means', async () => {
    const b = copy(erd);
    await layOutPending(b);
    expectLaidOut(b);
    expectApart(b.nodes.filter((n) => n.type === 'frame'));
    for (const e of b.edges) {
      expect(e.data!.sourceAt.side).toBe('right');
      expect(e.data!.targetAt.side).toBe('left');
      const [from, to] = [b.nodes.find((n) => n.id === e.source)!, b.nodes.find((n) => n.id === e.target)!];
      expect(from.data.field && to.data.field).toBe(true);
      expect(from.position.x + from.data.width).toBeLessThan(to.position.x);
    }
  });

  it('take their rows with them', async () => {
    const b = copy(erd);
    await layOutPending(b);
    for (const frame of b.nodes.filter((n) => n.type === 'frame')) {
      const rows = (erd.nodes as WBNode[]).filter((r) => {
        const f = (erd.nodes as WBNode[]).find((n) => n.id === frame.id)!;
        return r.data.field && r.position.y > f.position.y && r.position.y < f.position.y + f.data.height && r.position.x > f.position.x && r.position.x < f.position.x + f.data.width;
      });
      expect(rows.length).toBeGreaterThan(0);
      for (const row of rows) {
        const now = b.nodes.find((n) => n.id === row.id)!;
        expect(now.position.x).toBeGreaterThan(frame.position.x);
        expect(now.position.x + now.data.width).toBeLessThan(frame.position.x + frame.data.width);
        expect(now.position.y).toBeGreaterThan(frame.position.y);
        expect(now.position.y + now.data.height).toBeLessThanOrEqual(frame.position.y + frame.data.height);
      }
    }
  });
});

describe('a data flow', () => {
  /** Two lines each way between a pair of boxes had their labels on top of each other. */
  it('has no label over another', async () => {
    const b = copy(dfd);
    await layOutPending(b);
    expectLaidOut(b);
    expectApart(b.nodes);
    const boxes = boxesOf(b);
    const rects = b.edges
      .filter((e) => e.data?.label)
      .map((e) => {
        const p = pointAt(drawnPath(routeOfLine(e), boxes.get(e.source)!, boxes.get(e.target)!), e.data!.labelAt);
        const { width, height } = labelSize(e.data!.label);
        return { label: e.data!.label, x0: p.x - width / 2, x1: p.x + width / 2, y0: p.y - height / 2, y1: p.y + height / 2 };
      });
    for (const [i, a] of rects.entries()) {
      for (const c of rects.slice(i + 1)) {
        const over = a.x0 < c.x1 && c.x0 < a.x1 && a.y0 < c.y1 && c.y0 < a.y1;
        expect(over, `${a.label} / ${c.label}`).toBe(false);
      }
    }
  });
});

describe('what is not laid out', () => {
  it('is a board with nothing waiting', async () => {
    const b = copy(flowchart);
    for (const n of b.nodes) delete n.data.layout;
    const before = JSON.stringify(b);
    expect(await layOutPending(b)).toBe(false);
    expect(JSON.stringify(b)).toBe(before);
  });

  /** An edit put a zone on it: somebody has said what goes where. */
  it('is a board with a frame of boxes on it, which only loses its marks', async () => {
    const b = copy(flowchart);
    const [x, y] = [b.nodes[0].position.x - 10, b.nodes[0].position.y - 10];
    b.nodes.unshift({ id: 'zone', type: 'frame', position: { x, y }, data: { label: 'Zone', width: 400, height: 200 } });
    const places = b.nodes.map((n) => ({ ...n.position }));
    expect(await layOutPending(b)).toBe(true);
    expect(b.nodes.map((n) => n.position)).toEqual(places);
    expect(b.nodes.some(waitsForLayout)).toBe(false);
    // Its lines are still the router's to do.
    expect(b.edges.every((e) => e.data?.routing === 'pending')).toBe(true);
  });
});

describe('labels', () => {
  const line = (id: string, x: number) => ({ id, label: 'trạng thái', path: [{ x, y: 0 }, { x, y: 200 }], share: 0.5 });

  it('slide along their lines off one another', () => {
    const shares = placeLabels([line('a', 0), line('b', 12)], []);
    expect(shares.get('a')).toBe(0.5);
    const [pa, pb] = [pointAt(line('a', 0).path, 0.5), pointAt(line('b', 12).path, shares.get('b')!)];
    expect(Math.abs(pa.y - pb.y)).toBeGreaterThanOrEqual(labelSize('trạng thái').height);
  });

  it('stay where they are when that is clear', () => {
    const shares = placeLabels([line('a', 0), line('b', 300)], []);
    expect([shares.get('a'), shares.get('b')]).toEqual([0.5, 0.5]);
  });

  it('are found along a line by where they were put', () => {
    const path = [{ x: 0, y: 0 }, { x: 100, y: 0 }, { x: 100, y: 100 }];
    expect(shareAt(path, { x: 100, y: 50 })).toBe(0.75);
    expect(shareAt(path, { x: 50, y: 10 })).toBe(0.25);
  });
});
