import { describe, it, expect } from 'vitest';
import { corners, drawnPath, endOn, hullOf, labelShare, linesToFollow, outlineOf, reachOf, routeLines, routeOf, routePending, runsThrough, type Outline, type Route } from '../autoRoute';
import changeRequest from './fixtures/change-request-drawn.json';
import { anchorAt } from '../routing';
import type { WBEdge, WBNode } from '../boardFile';
import type { Box } from '../inkLayer';

const box = (x: number, y: number, width = 100, height = 50): Box => ({ x, y, width, height });

/** Whether a straight run from `a` to `b` passes through the inside of `b0x`. */
function crosses(a: { x: number; y: number }, b: { x: number; y: number }, r: Box): boolean {
  const [x0, x1] = [Math.min(a.x, b.x), Math.max(a.x, b.x)];
  const [y0, y1] = [Math.min(a.y, b.y), Math.max(a.y, b.y)];
  return x1 > r.x + 1 && x0 < r.x + r.width - 1 && y1 > r.y + 1 && y0 < r.y + r.height - 1;
}

describe('a route as the board keeps it', () => {
  it('says where an end is on its box, and anchorAt gives the point back', () => {
    const b = box(100, 200);
    for (const p of [{ x: 130, y: 200 }, { x: 200, y: 210 }, { x: 175, y: 250 }, { x: 100, y: 240 }]) {
      const at = endOn(b, p);
      const back = anchorAt(b, at.side, at.t);
      expect(Math.abs(back.x - p.x) + Math.abs(back.y - p.y)).toBeLessThan(0.2);
    }
    expect(endOn(b, { x: 130, y: 200 })).toEqual({ side: 'top', t: 0.3 });
  });

  it('makes a run that is all but upright upright', () => {
    expect(corners([{ x: 0, y: 0 }, { x: 0.3, y: 50 }, { x: 100, y: 50.4 }])).toEqual([{ x: 0, y: 0 }, { x: 0, y: 50 }, { x: 100, y: 50 }]);
  });

  it('keeps only the turns of a path', () => {
    const path = [{ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 0 }, { x: 50, y: 0 }, { x: 50, y: 30 }, { x: 50, y: 80 }];
    expect(corners(path)).toEqual([{ x: 0, y: 0 }, { x: 50, y: 0 }, { x: 50, y: 80 }]);
  });

  it('puts the label halfway along the longest run, not on a corner', () => {
    // 10 across, 80 down, 10 across: the middle of the run down.
    const path = [{ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 80 }, { x: 20, y: 80 }];
    expect(labelShare(path)).toBe(0.5);
    // 60 across, then 20 down: 30 of 80.
    expect(labelShare([{ x: 0, y: 0 }, { x: 60, y: 0 }, { x: 60, y: 20 }])).toBe(0.375);
  });

  it('is the corners between the ends, and the ends on their boxes', () => {
    const route = routeOf([{ x: 150, y: 50 }, { x: 150, y: 100 }, { x: 330, y: 100 }, { x: 330, y: 150 }], box(100, 0), box(300, 150))!;
    expect(route.waypoints).toEqual([{ x: 150, y: 100 }, { x: 330, y: 100 }]);
    expect(route.sourceAt).toEqual({ side: 'bottom', t: 0.5 });
    expect(route.targetAt).toEqual({ side: 'top', t: 0.3 });
  });
});

describe('where a line can touch a shape', () => {
  it('is anywhere on a box, and only the straight stretches of other shapes', () => {
    expect(reachOf('rectangle')).toBeUndefined();
    expect(reachOf('process')).toBeUndefined();
    expect(reachOf(undefined)).toBeUndefined();
    // Nothing straight along any side: only the middles.
    expect(reachOf('diamond')).toEqual({});
    expect(reachOf('ellipse')).toEqual({});
    // A pill is straight along the middle of each side; a hexagon along its top and bottom.
    expect(reachOf('pill')).toEqual({ top: [0.292, 0.708], right: [0.292, 0.708], bottom: [0.292, 0.708], left: [0.292, 0.708] });
    expect(reachOf('hexagon')).toEqual({ top: [0.24, 0.76], bottom: [0.24, 0.76] });
  });

  it('is routed around as the shape it is, not its box', () => {
    expect(hullOf('rectangle')).toBeUndefined();
    expect(hullOf('diamond')).toEqual([{ x: 0, y: 0.5 }, { x: 0.5, y: 0 }, { x: 1, y: 0.5 }, { x: 0.5, y: 1 }]);
    // A person (an arc for a head) and a cloud (corners well inside its
    // curves) are routed as their boxes, met anywhere.
    for (const figure of ['umlActor', 'cloud']) {
      expect(hullOf(figure), figure).toBeUndefined();
      expect(reachOf(figure), figure).toBeUndefined();
    }
    for (const figure of ['cylinder', 'hexagon', 'pill']) {
      const hull = hullOf(figure);
      if (!hull) continue;
      for (const p of hull) expect(p.x >= 0 && p.x <= 1 && p.y >= 0 && p.y <= 1).toBe(true);
      const turns = hull.map((a, i) => {
        const [b, c] = [hull[(i + 1) % hull.length], hull[(i + 2) % hull.length]];
        return (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x);
      });
      expect(turns.every((t) => t > 0), `${figure} turns one way all round`).toBe(true);
    }
  });

  /** The data-flow board Syn drew: given the cloud's outline, the router threw. */
  it('routes between figures drawn in several strokes', async () => {
    const figure = (id: string, x: number, y: number, shapeType: string) =>
      [id, outlineOf({ type: 'shape', position: { x, y }, data: { width: 120, height: 100, shapeType } })!] as const;
    const boxes = new Map([
      figure('user', 0, 0, 'umlActor'),
      figure('app', 300, 0, 'ellipse'),
      figure('db', 600, 0, 'cylinder'),
      figure('cloud', 300, 300, 'cloud'),
    ]);
    const routes = await routeLines(boxes, [
      { id: 'a', source: 'user', target: 'app' },
      { id: 'b', source: 'app', target: 'db' },
      { id: 'c', source: 'app', target: 'cloud' },
      { id: 'd', source: 'user', target: 'cloud' },
    ]);
    expect(routes.size).toBe(4);
  });

  it('moves an end to the corner of a diamond, and its first bend with it', () => {
    const diamond = { ...box(0, 0, 200, 100), reach: {} };
    // Out of the bottom a quarter of the way along, down, across, down.
    const route = routeOf([{ x: 50, y: 100 }, { x: 50, y: 150 }, { x: 400, y: 150 }, { x: 400, y: 200 }], diamond, box(350, 200))!;
    expect(route.sourceAt).toEqual({ side: 'bottom', t: 0.5 });
    expect(route.waypoints).toEqual([{ x: 100, y: 150 }, { x: 400, y: 150 }]);
  });

  it('steps halfway when a straight line can no longer be one', () => {
    const diamond = { ...box(0, 0, 200, 100), reach: {} };
    const route = routeOf([{ x: 60, y: 100 }, { x: 60, y: 200 }], diamond, box(0, 200))!;
    expect(route.waypoints).toEqual([{ x: 100, y: 150 }, { x: 60, y: 150 }]);
    expect(route.targetAt).toEqual({ side: 'top', t: 0.6 });
  });
});

describe('routing with MSAGL', () => {
  /** The line as the app draws it: its ends where it says, its bends between. */
  const drawn = (boxes: Map<string, Box>, route: Route, from: string, to: string) => [
    anchorAt(boxes.get(from)!, route.sourceAt.side, route.sourceAt.t),
    ...route.waypoints,
    anchorAt(boxes.get(to)!, route.targetAt.side, route.targetAt.t),
  ];
  const straightRuns = (points: { x: number; y: number }[]) =>
    points.slice(1).every((q, i) => Math.abs(points[i].x - q.x) < 0.2 || Math.abs(points[i].y - q.y) < 0.2);

  it('goes around a box in the way, at right angles', async () => {
    const boxes = new Map([['a', box(0, 0)], ['b', box(200, 0)], ['c', box(400, 0)]]);
    const routes = await routeLines(boxes, [{ id: 'ac', source: 'a', target: 'c' }]);
    const route = routes.get('ac')!;
    expect(route).toBeTruthy();
    const points = drawn(boxes, route, 'a', 'c');
    expect(straightRuns(points)).toBe(true);
    for (let i = 1; i < points.length; i++) {
      expect(crosses(points[i - 1], points[i], boxes.get('b')!), `run ${i} goes through b`).toBe(false);
    }
  });

  /**
   * Two boxes of different widths, one under the other: the router hands this
   * back as a polyline rather than pieces, and read as pieces it came out as
   * one slanted run with no bends.
   */
  it('steps between two boxes that are not quite in line', async () => {
    const boxes = new Map([['a', box(80, 0, 148, 56)], ['b', box(80, 120, 184, 56)]]);
    const route = (await routeLines(boxes, [{ id: 'ab', source: 'a', target: 'b' }])).get('ab')!;
    const points = drawn(boxes, route, 'a', 'b');
    expect(straightRuns(points), JSON.stringify(points)).toBe(true);
    expect(route.sourceAt.side).toBe('bottom');
    expect(route.targetAt.side).toBe('top');
  });

  /**
   * A question with its "no" answer right under it and its "yes" further
   * down: both leave the diamond from a corner, and "yes" goes around "no".
   */
  it('takes a line from a diamond around the box under it', async () => {
    const shape = (id: string, x: number, y: number, width: number, height: number, shapeType = 'process') =>
      [id, outlineOf({ type: 'shape', position: { x, y }, data: { width, height, shapeType } })!] as const;
    const boxes = new Map([
      shape('ask', 184, 1296, 346, 88, 'diamond'),
      shape('no', 249, 1456, 216, 56),
      shape('check', 224, 1584, 266, 56),
    ]);
    const routes = await routeLines(boxes, [
      { id: 'no', source: 'ask', target: 'no' },
      { id: 'yes', source: 'ask', target: 'check' },
    ]);
    for (const [id, to] of [['no', 'no'], ['yes', 'check']]) {
      const route = routes.get(id)!;
      const points = drawn(boxes, route, 'ask', to);
      expect(straightRuns(points), `${id}: ${JSON.stringify(points)}`).toBe(true);
      expect([0.5], `${id} leaves from a corner`).toContain(route.sourceAt.t);
      if (id === 'yes') {
        for (let i = 1; i < points.length; i++) {
          expect(crosses(points[i - 1], points[i], boxes.get('no')!), `yes, run ${i}: ${JSON.stringify(points)}`).toBe(false);
        }
      }
    }
  });

  /** Boxes as the canvas measures them, to the whole pixel: a fifth of a pixel off, and still straight. */
  it('runs straight down a chain whose middles are a fraction of a pixel apart', async () => {
    const boxes = new Map([['a', box(283.3, 80, 148, 56)], ['b', box(265.5, 208, 184, 56)], ['c', box(217, 336, 281, 88)]]);
    const routes = await routeLines(boxes, [{ id: 'ab', source: 'a', target: 'b' }, { id: 'bc', source: 'b', target: 'c' }]);
    for (const id of ['ab', 'bc']) expect(routes.get(id)!.waypoints, id).toEqual([]);
  });

  it('spreads lines out of one box along its side', async () => {
    // One box above three: the three lines leave it from three places.
    const boxes = new Map([['hub', box(200, 0, 300, 60)], ['x', box(0, 300)], ['y', box(300, 300)], ['z', box(600, 300)]]);
    const routes = await routeLines(boxes, ['x', 'y', 'z'].map((t) => ({ id: t, source: 'hub', target: t })));
    const starts = [...routes.values()].map((r) => `${r.sourceAt.side}:${r.sourceAt.t}`);
    expect(routes.size).toBe(3);
    expect(new Set(starts).size).toBe(3);
  });

  it('leaves out a line from a box to itself, or to something it was not given', async () => {
    const boxes = new Map([['a', box(0, 0)]]);
    const routes = await routeLines(boxes, [{ id: 'self', source: 'a', target: 'a' }, { id: 'gone', source: 'a', target: 'nowhere' }]);
    expect(routes.size).toBe(0);
  });
});

describe('a move', () => {
  const line = (id: string, source: string, target: string, routed = true): WBEdge =>
    ({ id, source, target, type: 'step', data: routed ? { routed: true } : { waypoints: [{ x: 1, y: 1 }] } });

  it('leaves behind the router\'s lines with one end moved, not those moved whole or bent by hand', () => {
    const edges = [line('ab', 'a', 'b'), line('bc', 'b', 'c'), line('cd', 'c', 'd'), line('ae', 'a', 'e', false), line('de', 'd', 'e')];
    // a and b moved together; c moved alone; d moved by a different amount from c.
    const moved = new Map([['a', { dx: 10, dy: 0 }], ['b', { dx: 10, dy: 0 }], ['c', { dx: 0, dy: 20 }], ['d', { dx: 5, dy: 0 }]]);
    expect(linesToFollow(edges, moved)).toEqual(['bc', 'cd', 'de']);
  });
});

describe('a board Syn drew', () => {
  const shape = (id: string, x: number, y: number): WBNode =>
    ({ id, type: 'shape', position: { x, y }, data: { label: id, width: 120, height: 56 } }) as WBNode;
  const line = (id: string, source: string, target: string, data: Record<string, any> = {}): WBEdge =>
    ({ id, source, target, type: 'step', sourceHandle: 'bottom', targetHandle: 'top', data: { sides: 'auto', ...data } });

  it('has the lines that ask routed when it is opened, and the flag taken off', async () => {
    const board = {
      nodes: [
        shape('a', 0, 0), shape('b', 0, 200), shape('c', 300, 200),
        { id: 'f', type: 'frame', position: { x: -50, y: -50 }, data: { width: 600, height: 400 } } as WBNode,
      ],
      edges: [
        line('ab', 'a', 'b', { routing: 'pending', label: 'yes' }),
        line('ac', 'a', 'c', { routing: 'pending' }),
        line('bc', 'b', 'c', { waypoints: [{ x: 1, y: 2 }] }),
        line('af', 'a', 'f', { routing: 'pending' }),
      ],
    };
    expect(await routePending(board)).toBe(true);
    const [ab, ac, bc, af] = board.edges;
    for (const e of [ab, ac]) {
      expect(e.data!.routing).toBeUndefined();
      expect(e.data!.sourceAt?.side).toBeTruthy();
      expect(e.data!.targetAt?.side).toBe('top');
      expect(typeof e.data!.labelAt).toBe('number');
      expect(e.updated).toBeTypeOf('number');
    }
    expect(ab.data!.label).toBe('yes');
    expect(ab.data!.sides).toBe('auto');
    expect(ab.data!.routed, 'its bends are the router\'s').toBe(true);
    // A line nobody asked about is left as it was.
    expect(bc.data!.waypoints).toEqual([{ x: 1, y: 2 }]);
    expect(bc.updated).toBeUndefined();
    // A line to a frame cannot be routed around it, and is not asked about again.
    expect(af.data!.routing).toBeUndefined();
    expect(af.data!.waypoints).toBeUndefined();

    expect(await routePending(board)).toBe(false);
  });
});

/**
 * The Change Request flowchart as `draw_board` lays it out (board.rs, from
 * the sketch in src-tauri/src/syn/testdata/board-flowchart-loop.json), every
 * line asking to be routed.
 */
describe('the Change Request flowchart Syn drew', () => {
  it('has every line routed, and none through a box', async () => {
    const board = JSON.parse(JSON.stringify(changeRequest)) as { nodes: WBNode[]; edges: WBEdge[] };
    expect(board.edges.every((e) => e.data?.routing === 'pending')).toBe(true);
    await routePending(board);
    const boxes = new Map<string, Outline>(board.nodes.map((n) => [n.id, outlineOf(n)!]));
    const label = (id: string) => String(board.nodes.find((n) => n.id === id)?.data?.label);
    let routed = 0;
    for (const e of board.edges) {
      expect(e.data!.routing).toBeUndefined();
      if (!e.data!.sourceAt) continue;
      routed++;
      const route = e.data as unknown as Route;
      const path = drawnPath(route, boxes.get(e.source)!, boxes.get(e.target)!);
      expect(runsThrough(path, boxes, e), `${label(e.source)} → ${label(e.target)}: ${JSON.stringify(path)}`).toBe(false);
    }
    expect(routed).toBe(board.edges.length);
  });
});

