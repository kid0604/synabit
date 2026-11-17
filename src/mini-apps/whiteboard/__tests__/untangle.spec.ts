import { describe, expect, it } from 'vitest';
import type { WBEdge, WBNode } from '../boardFile';
import { layOutPending } from '../autoLayout';
import { drawnPath, outlineOf, routeLines, runsThrough, type Outline, type Point, type Route } from '../autoRoute';
import { crossingsIn, meetings, untangle } from '../untangle';
import flowchart from './fixtures/elk-flowchart.json';
import dfd from './fixtures/elk-dfd.json';
import figures from './fixtures/elk-dfd-figures.json';

/**
 * Lines that do not cross: on one layer of a circuit board two traces never
 * do, and a drawing laid out with that discipline reads at a glance. See
 * untangle.ts, and the tries in autoLayout.ts.
 */

const box = (x: number, y: number, width = 120, height = 56): Outline => ({ x, y, width, height });
const pathOf = (route: Route, boxes: Map<string, Outline>, line: { source: string; target: string }) =>
  drawnPath(route, boxes.get(line.source)!, boxes.get(line.target)!);

/** A on the top left, B top right, C under A, D under B. */
const square = () =>
  new Map<string, Outline>([
    ['a', box(0, 0)],
    ['b', box(400, 0)],
    ['c', box(0, 240)],
    ['d', box(400, 240)],
  ]);

/** A to D and C to B, each a Z across the middle: they cross once. */
const crossed = (): { id: string; source: string; target: string; route: Route }[] => [
  {
    id: 'ad', source: 'a', target: 'd',
    route: { waypoints: [{ x: 250, y: 20 }, { x: 250, y: 276 }], sourceAt: { side: 'right', t: 20 / 56 }, targetAt: { side: 'left', t: 36 / 56 }, labelAt: 0.5 },
  },
  {
    id: 'cb', source: 'c', target: 'b',
    route: { waypoints: [{ x: 280, y: 252 }, { x: 280, y: 40 }], sourceAt: { side: 'right', t: 12 / 56 }, targetAt: { side: 'left', t: 40 / 56 }, labelAt: 0.5 },
  },
];

describe('untangle', () => {
  it('takes crossing lines another way round, through nothing', () => {
    const boxes = square();
    const lines = crossed();
    expect(crossingsIn(lines.map((l) => pathOf(l.route, boxes, l)))).toBe(1);

    const better = untangle(lines, boxes);
    const paths = lines.map((l) => pathOf(better.get(l.id) ?? l.route, boxes, l));
    expect(crossingsIn(paths)).toBe(0);
    for (const [i, l] of lines.entries()) expect(runsThrough(paths[i], boxes, l), l.id).toBe(false);
  });

  it('leaves alone a line that crosses nothing', () => {
    const boxes = square();
    const line = {
      id: 'ab', source: 'a', target: 'b',
      route: { waypoints: [], sourceAt: { side: 'right' as const, t: 0.5 }, targetAt: { side: 'left' as const, t: 0.5 }, labelAt: 0.5 },
    };
    expect(untangle([line], boxes).size).toBe(0);
  });

  it('keeps off a line drawn by hand, and never moves it', () => {
    const boxes = square();
    // Somebody's line from C up to B, straight through where A to D goes.
    const byHand: Point[] = [{ x: 120, y: 252 }, { x: 280, y: 252 }, { x: 280, y: 40 }, { x: 400, y: 40 }];
    const [ad] = crossed();
    expect(meetings(pathOf(ad.route, boxes, ad), [byHand]).crossings).toBe(1);
    const better = untangle([ad], boxes, [byHand]);
    const path = pathOf(better.get('ad') ?? ad.route, boxes, ad);
    expect(meetings(path, [byHand]).crossings).toBe(0);
    expect([...better.keys()]).toEqual(['ad']);
  });

  it('does not trade a crossing for two lines on top of each other', () => {
    const boxes = square();
    const lines = crossed();
    const better = untangle(lines, boxes);
    const paths = lines.map((l) => pathOf(better.get(l.id) ?? l.route, boxes, l));
    expect(meetings(paths[0], [paths[1]]).along).toBe(0);
  });

  it('gives a line the router could not route a way that goes through nothing', () => {
    const boxes = square();
    const better = untangle([{ id: 'ad', source: 'a', target: 'd' }], boxes);
    const route = better.get('ad');
    expect(route).toBeDefined();
    expect(runsThrough(pathOf(route!, boxes, { source: 'a', target: 'd' }), boxes, { source: 'a', target: 'd' })).toBe(false);
  });
});

describe('the router', () => {
  it('routes lines that cross nothing, the board\'s other lines included', async () => {
    const boxes = square();
    const routes = await routeLines(boxes, [
      { id: 'ad', source: 'a', target: 'd' },
      { id: 'cb', source: 'c', target: 'b' },
    ]);
    const paths = ['ad', 'cb'].map((id) => pathOf(routes.get(id)!, boxes, { source: id[0], target: id[1] }));
    expect(crossingsIn(paths)).toBe(0);
  });
});

type Board = { nodes: WBNode[]; edges: WBEdge[] };
const copy = (b: unknown): Board => JSON.parse(JSON.stringify(b));
const crossingsOn = (b: Board) => {
  const boxes = new Map<string, Outline>();
  for (const n of b.nodes) if (n.type !== 'frame') boxes.set(n.id, outlineOf(n)!);
  return crossingsIn(
    b.edges.map((e) => drawnPath({ waypoints: e.data!.waypoints, sourceAt: e.data!.sourceAt, targetAt: e.data!.targetAt, labelAt: 0.5 }, boxes.get(e.source)!, boxes.get(e.target)!)),
  );
};

const shape = (id: string, x: number, y: number): WBNode =>
  ({ id, type: 'shape', position: { x, y }, data: { label: id, width: 120, height: 56, shapeType: 'roundedRect', layout: 'pending' }, updated: 1 }) as WBNode;
const link = (source: string, target: string): WBEdge =>
  ({ id: `${source}${target}`, source, target, type: 'step', data: { routing: 'pending' }, updated: 1 }) as WBEdge;

describe('a board laid out by ELK', () => {
  it('keeps the layout with fewest crossings, not the one in the order Syn listed things', async () => {
    // Listed C before D, but A goes to D and B to C: drawn in that order,
    // the two lines cross.
    const board: Board = {
      nodes: [shape('A', 0, 0), shape('B', 200, 0), shape('C', 0, 200), shape('D', 200, 200)],
      edges: [link('A', 'D'), link('B', 'C')],
    };
    await layOutPending(board);
    expect(crossingsOn(board)).toBe(0);
  });

  it('draws the flowchart and the data flows Syn drew with no crossings', async () => {
    for (const fixture of [flowchart, dfd, figures]) {
      const board = copy(fixture);
      await layOutPending(board);
      expect(crossingsOn(board), (fixture as { title?: string }).title).toBe(0);
    }
  });
});
