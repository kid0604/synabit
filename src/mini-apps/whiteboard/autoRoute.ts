import { stampElement } from './boardFile';
import type { WBEdge, WBNode } from './boardFile';
import type { Box } from './inkLayer';
import { anchorAt, orthogonalRoute, type LineEnd, type Side } from './routing';
import { SHAPES_MAP } from './shapes';
import { GLYPH_SHAPE } from './glyph';

/**
 * Lines routed around the boxes by MSAGL's rectilinear router, and written
 * into the board as the bends, ends and label place the app already draws
 * (`waypoints`, `sourceAt`/`targetAt`, `labelAt`; see WaypointEdge).
 *
 * # Why a library, and why only for routing
 *
 * The app's own stepped line (`orthogonalRoute`) finds a way around boxes for
 * one line at a time, from the middle of a side. On a busy board every line
 * out of one box left from the same point and ran on top of the others for
 * the first stretch: the data-centre board Syn drew had 40 lines sharing an
 * end and 13,000 px of line drawn over line. MSAGL routes the lines together,
 * spreads the ends along the side and keeps parallel runs apart; on the same
 * board both came to nought (docs/whiteboard-layout-elk-2026-11-13.md).
 *
 * The boxes stay where they are. Where things go on a board Syn drew is a
 * choice — sites side by side, lanes, steps in order — and a person's board
 * is theirs; this only finds the way between them.
 *
 * # When it runs
 *
 * On a line that asks for it, `data.routing = 'pending'`, which Syn writes on
 * the lines it draws between boxes (`route_later`, board.rs): when the board
 * is opened, the app routes those, writes the route in and takes the flag off.
 * When a box at one end of a routed line moves, the line is routed again
 * (`linesToFollow`), until somebody bends it by hand. And when the user asks
 * — "Tidy lines" on the lines they have selected.
 *
 * Loaded when first needed (about 140 KB compressed): most boards never ask.
 */

export interface Point {
  x: number;
  y: number;
}

/** A line to route: its id, the ids of the boxes at its ends, and its label if it has one. */
export interface LineToRoute {
  id: string;
  source: string;
  target: string;
  data?: { label?: unknown };
}

/** A routed line, as the board keeps one. */
export interface Route {
  /** The corners between the two ends; none for a straight run. */
  waypoints: Point[];
  sourceAt: LineEnd;
  targetAt: LineEnd;
  /** Where the label goes, as a share of the way along. */
  labelAt: number;
}

/** Room kept clear around every box, and between two lines running side by side. */
const PADDING = 6;
const SEPARATION = 8;

/** Coordinates as written to the file: to the tenth of a pixel. */
const tidy = (n: number) => Math.round(n * 10) / 10;

/**
 * Where a point on a box's outline is, as the side it is on and how far
 * along that side: left to right on the top and bottom, top to bottom on the
 * left and right — what `anchorAt` turns back into the point.
 */
export function endOn(box: Box, p: Point): LineEnd {
  const gaps: [Side, number][] = [
    ['top', Math.abs(p.y - box.y)],
    ['bottom', Math.abs(p.y - (box.y + box.height))],
    ['left', Math.abs(p.x - box.x)],
    ['right', Math.abs(p.x - (box.x + box.width))],
  ];
  const [side] = gaps.reduce((best, next) => (next[1] < best[1] ? next : best));
  const along = side === 'top' || side === 'bottom' ? (p.x - box.x) / box.width : (p.y - box.y) / box.height;
  return { side, t: Math.round(Math.min(1, Math.max(0, along)) * 1000) / 1000 };
}

/**
 * A path with every point that is not a turn taken out, and any run that is
 * all but upright or level made so: a step of a fraction of a pixel is two
 * corners nobody can see.
 */
export function corners(points: Point[]): Point[] {
  const out: Point[] = [];
  for (const raw of points) {
    const prev = out[out.length - 1];
    const p = { ...raw };
    if (prev && Math.abs(p.x - prev.x) < 1) p.x = prev.x;
    if (prev && Math.abs(p.y - prev.y) < 1) p.y = prev.y;
    const last = out[out.length - 1];
    if (last && Math.abs(last.x - p.x) < 0.5 && Math.abs(last.y - p.y) < 0.5) continue;
    if (out.length >= 2) {
      const a = out[out.length - 2];
      const cross = (last.x - a.x) * (p.y - a.y) - (last.y - a.y) * (p.x - a.x);
      if (Math.abs(cross) < 0.5) out.pop();
    }
    out.push(p);
  }
  return out;
}

/**
 * The label's place on a routed line: halfway along its longest run.
 *
 * Halfway along the whole line put it on a corner as often as not, where it
 * covers the turn and reads as belonging to either run.
 */
export function labelShare(points: Point[]): number {
  const runs = points.slice(1).map((p, i) => Math.hypot(p.x - points[i].x, p.y - points[i].y));
  const total = runs.reduce((a, b) => a + b, 0);
  if (!total) return 0.5;
  let longest = 0;
  for (let i = 1; i < runs.length; i++) if (runs[i] > runs[longest]) longest = i;
  const before = runs.slice(0, longest).reduce((a, b) => a + b, 0);
  return Math.round(((before + runs[longest] / 2) / total) * 1000) / 1000;
}

/**
 * Where on each side of a shape a line can end and touch it: the stretch of
 * the side its outline runs along, as shares of the side. A side not listed
 * is touched at one point only, its middle — a diamond's corner, the round
 * end of a pill. No `reach` at all is a box, met anywhere along every side.
 */
export type Reach = Partial<Record<Side, [number, number]>>;

/**
 * A box to route around, where on it a line may end, and — for a shape that
 * is not a box — its outline, as corners on a unit square over the box.
 */
export interface Outline extends Box {
  reach?: Reach;
  hull?: Point[];
}

const shapes = new Map<string, { reach?: Reach; hull?: Point[] }>();

/**
 * Where lines can end on a shape, from its drawing (`ShapeDef.path`, on a 100
 * by 100 grid with the outline 2 in from the edge): every straight piece of
 * the outline that lies along a side of its box.
 *
 * The router knows only boxes. A line it ended a third of the way along a
 * diamond's bottom stopped at the bounding box, short of the diamond, with a
 * gap the size of the corner cut off.
 */
export function reachOf(shapeType: string | undefined): Reach | undefined {
  return shapeOf(shapeType).reach;
}

/**
 * The corners of a shape's outline, on a unit square over its box: what the
 * router goes around and ends lines on, for a shape that is not a box.
 *
 * Given only the box of a diamond, the router took a line out of the bottom
 * of the box well to one side of the diamond's corner, to pass beside the box
 * below; moved to the corner, the line went through that box. Given the
 * diamond, a line straight out of its middle leaves from the corner, and the
 * way around is found from there. The corners alone, a curve's control points
 * aside: an ellipse is routed as the diamond through its four extremes, which
 * are where lines leave it.
 */
export function hullOf(shapeType: string | undefined): Point[] | undefined {
  return shapeOf(shapeType).hull;
}

function shapeOf(shapeType: string | undefined): { reach?: Reach; hull?: Point[] } {
  const key = shapeType ?? '';
  const known = shapes.get(key);
  if (known) return known;
  const path = SHAPES_MAP[key]?.path;
  let reach: Reach | undefined;
  let hull: Point[] | undefined;
  // Only drawings made of the commands read here, absolute: a person's head
  // is an arc, and read without it the person was routed as a head alone.
  if (path && /^[\sMLQCZ\d.,-]+$/.test(path)) {
    // The end point of each command, and whether the piece to it is straight.
    const pieces: { x: number; y: number; straight: boolean }[] = [];
    for (const [, op, args] of path.matchAll(/([MLQCZ])([^MLQCZ]*)/gi)) {
      const n = args.match(/-?\d*\.?\d+/g)?.map(Number) ?? [];
      if (op.toUpperCase() === 'Z') {
        if (pieces.length) pieces.push({ ...pieces[0], straight: true });
      } else if (n.length >= 2) {
        pieces.push({ x: n[n.length - 2], y: n[n.length - 1], straight: op.toUpperCase() === 'L' });
      }
    }
    const found: Reach = {};
    const near = (v: number, edge: number) => Math.abs(v - edge) <= 3;
    const share = (v: number) => Math.round(((v - 2) / 96) * 1000) / 1000;
    for (let i = 1; i < pieces.length; i++) {
      const [a, b] = [pieces[i - 1], pieces[i]];
      if (!b.straight) continue;
      const along: [Side, boolean, number, number][] = [
        ['top', near(a.y, 2) && near(b.y, 2), a.x, b.x],
        ['bottom', near(a.y, 98) && near(b.y, 98), a.x, b.x],
        ['left', near(a.x, 2) && near(b.x, 2), a.y, b.y],
        ['right', near(a.x, 98) && near(b.x, 98), a.y, b.y],
      ];
      for (const [side, on, p, q] of along) {
        if (!on || Math.abs(p - q) < 10) continue;
        const [lo, hi] = [share(Math.min(p, q)), share(Math.max(p, q))];
        const had = found[side];
        found[side] = had ? [Math.min(had[0], lo), Math.max(had[1], hi)] : [lo, hi];
      }
    }
    const full = (['top', 'right', 'bottom', 'left'] as Side[]).every((side) => {
      const r = found[side];
      return r && r[0] <= 0.02 && r[1] >= 0.98;
    });
    const unit = (v: number) => Math.min(1, Math.max(0, (v - 2) / 96));
    const ring = convexHull(pieces.map((p) => ({ x: unit(p.x), y: unit(p.y) })));
    // An outline that fills its box both ways. One that does not — a cloud,
    // whose corners are well inside the curves drawn between them — is
    // routed as its box: the router failed outright on the cloud.
    const span = (axis: 'x' | 'y') => {
      const v = ring.map((p) => p[axis]);
      return Math.min(...v) <= 0.03 && Math.max(...v) >= 0.97;
    };
    if (!full && ring.length >= 3 && span('x') && span('y')) {
      reach = found;
      hull = ring;
    }
  }
  const shape = { reach, hull };
  shapes.set(key, shape);
  return shape;
}

/**
 * The smallest convex outline around these points, without repeats, turning
 * the way a diamond drawn top, right, bottom, left does.
 *
 * Convex, so that a figure drawn in more than one stroke is still one
 * outline, not a ring through all its corners that crosses itself.
 */
export function convexHull(points: Point[]): Point[] {
  const sorted = [...points].sort((a, b) => a.x - b.x || a.y - b.y)
    .filter((p, i, all) => i === 0 || p.x !== all[i - 1].x || p.y !== all[i - 1].y);
  if (sorted.length < 3) return sorted;
  const turn = (o: Point, a: Point, b: Point) => (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
  const chain = (list: Point[]) => {
    const out: Point[] = [];
    for (const p of list) {
      while (out.length >= 2 && turn(out[out.length - 2], out[out.length - 1], p) <= 0) out.pop();
      out.push(p);
    }
    out.pop();
    return out;
  };
  const hull = [...chain(sorted), ...chain([...sorted].reverse())];
  let area = 0;
  for (let i = 0; i < hull.length; i++) {
    const [a, b] = [hull[i], hull[(i + 1) % hull.length]];
    area += a.x * b.y - b.x * a.y;
  }
  return area < 0 ? hull.reverse() : hull;
}

/** Moved along a side to where the shape is: within its straight stretch, or its middle. */
function fitTo(at: LineEnd, box: Outline): LineEnd {
  if (!box.reach) return at;
  const r = box.reach[at.side];
  return { side: at.side, t: r ? Math.min(r[1], Math.max(r[0], at.t)) : 0.5 };
}

/**
 * A path the router drew, from one box's outline to the other's, as a line on
 * the board.
 *
 * An end that had to move along its side to touch the shape (`reachOf`) takes
 * the corner next to it along, so the line stays at right angles; a line with
 * no corners that is no longer straight gets a step halfway.
 */
export function routeOf(path: Point[], from: Outline, to: Outline): Route | null {
  const points = corners(path);
  if (points.length < 2) return null;
  const sourceAt = fitTo(endOn(from, points[0]), from);
  const targetAt = fitTo(endOn(to, points[points.length - 1]), to);
  const start = anchorAt(from, sourceAt.side, sourceAt.t);
  const end = anchorAt(to, targetAt.side, targetAt.t);
  const across = (side: Side) => side === 'top' || side === 'bottom';
  let waypoints = points.slice(1, -1);
  if (waypoints.length) {
    // The run from each end leaves its side square, so the corner at its far
    // end moves along with the end.
    const first = { ...waypoints[0] };
    if (across(sourceAt.side)) first.x += start.x - points[0].x;
    else first.y += start.y - points[0].y;
    waypoints[0] = first;
    const k = waypoints.length - 1;
    const last = { ...waypoints[k] };
    if (across(targetAt.side)) last.x += end.x - points[points.length - 1].x;
    else last.y += end.y - points[points.length - 1].y;
    waypoints[k] = last;
  } else if (Math.abs(start.x - end.x) > 0.5 && Math.abs(start.y - end.y) > 0.5) {
    waypoints = across(sourceAt.side)
      ? [{ x: start.x, y: (start.y + end.y) / 2 }, { x: end.x, y: (start.y + end.y) / 2 }]
      : [{ x: (start.x + end.x) / 2, y: start.y }, { x: (start.x + end.x) / 2, y: end.y }];
  }
  return {
    waypoints: waypoints.map((p) => ({ x: tidy(p.x), y: tidy(p.y) })),
    sourceAt,
    targetAt,
    labelAt: labelShare([start, ...waypoints, end]),
  };
}

/**
 * Route lines around boxes, all of them together.
 *
 * `boxes` is everything a line must go around, its own two ends included —
 * frames left out by the caller, since a line inside a frame is meant to be
 * in it. A line whose ends are not both among them, or that goes from a box
 * to itself, is left out of the answer.
 *
 * Throws when the router cannot be loaded or fails; the caller keeps what it
 * had.
 */
export async function routeLines(boxes: Map<string, Outline>, lines: LineToRoute[]): Promise<Map<string, Route>> {
  const routes = new Map<string, Route>();
  const wanted = lines.filter((l) => l.source !== l.target && boxes.has(l.source) && boxes.has(l.target));
  if (!wanted.length) return routes;

  const msagl = await import('@msagl/core');
  const { GeomEdge } = msagl;
  let edges: [LineToRoute, InstanceType<typeof msagl.Edge>][];
  try {
    edges = routeWithMsagl(msagl, boxes, wanted, true);
  } catch {
    // Its outlines are the one thing here the router has been seen to fail
    // on; boxes it has not.
    edges = routeWithMsagl(msagl, boxes, wanted, false);
  }

  const plain = [...boxes.values()].map(({ x, y, width, height }) => ({ x, y, width, height }));
  for (const [line, edge] of edges) {
    const curve = (GeomEdge.getGeom(edge) as InstanceType<typeof GeomEdge>).curve as any;
    if (!curve) continue;
    const from = boxes.get(line.source)!;
    const to = boxes.get(line.target)!;
    let route = routeOf(pathOf(curve), from, to);
    if (!route) continue;
    if (runsThrough(drawnPath(route, from, to), boxes, line)) {
      // The router's way goes through a box (see `runsThrough`): the app's
      // own way between the same two ends, and none if that does too.
      const start = anchorAt(from, route.sourceAt.side, route.sourceAt.t);
      const end = anchorAt(to, route.targetAt.side, route.targetAt.t);
      const path = corners(orthogonalRoute(start, route.sourceAt.side, end, route.targetAt.side, plain));
      route = {
        ...route,
        waypoints: path.slice(1, -1).map((p) => ({ x: tidy(p.x), y: tidy(p.y) })),
        labelAt: labelShare(path),
      };
      if (runsThrough(drawnPath(route, from, to), boxes, line)) continue;
    }
    routes.set(line.id, route);
  }

  // Labels slid along their lines off one another (`placeLabels`).
  const labelled: LabelToPlace[] = [];
  for (const line of wanted) {
    const route = routes.get(line.id);
    const label = typeof line.data?.label === 'string' ? line.data.label.trim() : '';
    if (!route || !label) continue;
    const path = drawnPath(route, boxes.get(line.source)!, boxes.get(line.target)!);
    labelled.push({ id: line.id, label, path, share: route.labelAt });
  }
  for (const [id, share] of placeLabels(labelled, boxes.values())) routes.get(id)!.labelAt = share;
  return routes;
}

/** One run of the router over the board, the shapes as outlines or as boxes. */
function routeWithMsagl(
  msagl: typeof import('@msagl/core'),
  boxes: Map<string, Outline>,
  lines: LineToRoute[],
  outlines: boolean,
): [LineToRoute, InstanceType<typeof msagl.Edge>][] {
  const { Graph, Node, Edge, GeomGraph, GeomNode, GeomEdge, CurveFactory, Polyline, Point: MPoint, routeRectilinearEdges } = msagl;
  const graph = new Graph();
  const nodes = new Map<string, InstanceType<typeof Node>>();
  for (const [id, b] of boxes) {
    if (!(b.width > 0 && b.height > 0)) continue;
    const node = graph.addNode(new Node(id));
    new GeomNode(node).boundaryCurve = outlines && b.hull
      ? Polyline.mkClosedFromPoints(b.hull.map((p) => new MPoint(b.x + p.x * b.width, b.y + p.y * b.height)))
      : CurveFactory.createRectangle(b.width, b.height, new MPoint(b.x + b.width / 2, b.y + b.height / 2));
    nodes.set(id, node);
  }
  const edges: [LineToRoute, InstanceType<typeof Edge>][] = [];
  for (const line of lines) {
    const a = nodes.get(line.source);
    const b = nodes.get(line.target);
    if (!a || !b) continue;
    const edge = new Edge(a, b);
    edge.add();
    new GeomEdge(edge);
    edges.push([line, edge]);
  }
  // The library's own names: padding round a box, the radius of a corner
  // (none: the app rounds its own, see `roundedPath`), the room between
  // parallel runs.
  routeRectilinearEdges(
    new GeomGraph(graph),
    edges.map(([, e]) => GeomEdge.getGeom(e) as InstanceType<typeof GeomEdge>),
    null as never,
    PADDING,
    0,
    SEPARATION,
  );
  return edges;
}

/** A label's size as the app draws it (12px semibold, 8 and 4 px of padding), near enough. */
export const labelSize = (text: string) => ({ width: Math.ceil(text.length * 7) + 18, height: 26 });

/** The point a share of the way along a path. */
export function pointAt(path: Point[], share: number): Point {
  const runs = path.slice(1).map((p, i) => Math.hypot(p.x - path[i].x, p.y - path[i].y));
  let left = runs.reduce((a, b) => a + b, 0) * share;
  for (let i = 0; i < runs.length; i++) {
    if (left <= runs[i] || i === runs.length - 1) {
      const k = runs[i] ? Math.min(1, left / runs[i]) : 0;
      return { x: path[i].x + (path[i + 1].x - path[i].x) * k, y: path[i].y + (path[i + 1].y - path[i].y) * k };
    }
    left -= runs[i];
  }
  return path[0];
}

/** A labelled line whose label is to be placed: its drawn path and where the label would go. */
export interface LabelToPlace {
  id: string;
  label: string;
  path: Point[];
  share: number;
}

/**
 * Where each label goes along its line so that it covers no other label and
 * no box, as a share of the way along — its own place if that is clear, else
 * the nearest clear one, slid along the line a step at a time each way.
 *
 * The app draws a label on its line, at a share of the way along. Two lines
 * each way between the same pair of boxes run side by side, a dozen pixels
 * apart, and with both labels halfway along, one covered the other. A label
 * nowhere clear goes where it covers least.
 */
export function placeLabels(lines: LabelToPlace[], boxes: Iterable<Box>): Map<string, number> {
  type Rect = { x0: number; y0: number; x1: number; y1: number };
  const solid: Rect[] = [...boxes].map((b) => ({ x0: b.x, y0: b.y, x1: b.x + b.width, y1: b.y + b.height }));
  const placed: Rect[] = [];
  const overlap = (a: Rect, b: Rect) =>
    Math.max(0, Math.min(a.x1, b.x1) - Math.max(a.x0, b.x0)) * Math.max(0, Math.min(a.y1, b.y1) - Math.max(a.y0, b.y0));
  const shares = new Map<string, number>();
  for (const line of lines) {
    const { width, height } = labelSize(line.label);
    const rectAt = (share: number): Rect => {
      const p = pointAt(line.path, share);
      return { x0: p.x - width / 2, y0: p.y - height / 2, x1: p.x + width / 2, y1: p.y + height / 2 };
    };
    const tries = [line.share];
    for (let k = 1; k <= 8; k++) tries.push(line.share + 0.06 * k, line.share - 0.06 * k);
    let best = { share: line.share, cost: Infinity };
    for (const share of tries.filter((t) => t >= 0.1 && t <= 0.9)) {
      const r = rectAt(share);
      // Another label is worse to cover than a box's edge.
      const cost = placed.reduce((sum, o) => sum + overlap(r, o) * 4, 0) + solid.reduce((sum, o) => sum + overlap(r, o), 0);
      if (cost < best.cost) best = { share, cost };
      if (!cost) break;
    }
    if (best.cost === Infinity) best.cost = 0;
    const share = Math.round(best.share * 1000) / 1000;
    shares.set(line.id, share);
    placed.push(rectAt(share));
  }
  return shares;
}

/** A route as the app draws it: from one end, through its corners, to the other. */
export function drawnPath(route: Route, from: Box, to: Box): Point[] {
  return [
    anchorAt(from, route.sourceAt.side, route.sourceAt.t),
    ...route.waypoints,
    anchorAt(to, route.targetAt.side, route.targetAt.t),
  ];
}

/** Whether a point is inside an item: inside its box, and inside its shape if it has one. */
function within(o: Outline, x: number, y: number): boolean {
  if (x <= o.x + 1 || x >= o.x + o.width - 1 || y <= o.y + 1 || y >= o.y + o.height - 1) return false;
  if (!o.hull) return true;
  const u = (x - o.x) / o.width;
  const v = (y - o.y) / o.height;
  let inside = false;
  for (let i = 0, j = o.hull.length - 1; i < o.hull.length; j = i++) {
    const [a, b] = [o.hull[i], o.hull[j]];
    if (a.y > v !== b.y > v && u < ((b.x - a.x) * (v - a.y)) / (b.y - a.y) + a.x) inside = !inside;
  }
  return inside;
}

/**
 * Whether a path goes through any item but the two it joins.
 *
 * Asked of every route, because the router does not always keep out of the
 * way. On the Change Request flowchart Syn drew it took "yes" from a question
 * straight down through the "no" answer under it, and "rejected → end" down
 * through three steps — before its own nudging, so not something a setting
 * turns off. Its other routes there were good; these two are drawn the app's
 * way instead.
 */
export function runsThrough(path: Point[], boxes: Map<string, Outline>, ends: { source: string; target: string }): boolean {
  for (const [id, o] of boxes) {
    if (id === ends.source || id === ends.target) continue;
    for (let i = 1; i < path.length; i++) {
      const [p, q] = [path[i - 1], path[i]];
      // Only the stretch of the run that is over the box can be in it.
      const x0 = Math.max(Math.min(p.x, q.x), o.x);
      const x1 = Math.min(Math.max(p.x, q.x), o.x + o.width);
      const y0 = Math.max(Math.min(p.y, q.y), o.y);
      const y1 = Math.min(Math.max(p.y, q.y), o.y + o.height);
      if (x0 > x1 || y0 > y1) continue;
      const steps = Math.max(1, Math.ceil(Math.max(x1 - x0, y1 - y0) / 2));
      for (let k = 0; k <= steps; k++) {
        if (within(o, x0 + ((x1 - x0) * k) / steps, y0 + ((y1 - y0) * k) / steps)) return true;
      }
    }
  }
  return false;
}

/**
 * The points of a route as the router hands it back, which is in one of two
 * forms: a `Polyline`, which lists its points, or a `Curve` of straight
 * pieces, whose corners are where one piece ends and the next begins. Read as
 * the second, a polyline is one piece from its start to its end: a line
 * between two boxes not quite in line was drawn as a slant, and in the app as
 * a stepped line from those ends, through the middle.
 */
function pathOf(curve: any): Point[] {
  if (typeof curve?.[Symbol.iterator] === 'function') {
    return Array.from(curve as Iterable<Point>, (p) => ({ x: p.x, y: p.y }));
  }
  const pieces: any[] = Array.isArray(curve?.segs) ? curve.segs : [curve];
  const path = pieces.map((s) => ({ x: s.start.x, y: s.start.y }));
  const last = pieces[pieces.length - 1];
  path.push({ x: last.end.x, y: last.end.y });
  return path;
}

/**
 * The lines the router drew that a move has left behind: one end moved and
 * the other did not, or not by as much. Lines whose ends moved together keep
 * their shape (`carryWaypoints`).
 */
export function linesToFollow(edges: WBEdge[], moved: Map<string, { dx: number; dy: number }>): string[] {
  return edges
    .filter((e) => {
      if (!e.data?.routed) return false;
      const [a, b] = [moved.get(e.source), moved.get(e.target)];
      if (!a && !b) return false;
      return !(a && b && a.dx === b.dx && a.dy === b.dy);
    })
    .map((e) => e.id);
}

/** Whether a line is waiting to be routed. */
export const waitsForRoute = (e: WBEdge): boolean => e.data?.routing === 'pending';

/**
 * Write a route into a line. It becomes a stepped line with these bends, the
 * flag off; `sides: 'auto'` stays, so that if its bends are all taken away it
 * steps around the boxes again, from the ends it has now.
 *
 * Marked `routed`: its bends are the router's, not somebody's, so when a box
 * at either end is moved the line follows it and is routed again (see
 * `linesToFollow`, and WaypointEdge during the drag). Bending it by hand
 * takes the mark off, and from then on its bends stay where they were put.
 */
export function applyRoute(edge: WBEdge, route: Route): void {
  const data = { ...(edge.data ?? {}) };
  delete data.routing;
  data.routed = true;
  data.waypoints = route.waypoints;
  data.sourceAt = route.sourceAt;
  data.targetAt = route.targetAt;
  data.labelAt = route.labelAt;
  edge.type = 'step';
  edge.data = data;
  stampElement(edge);
}

/**
 * A board item as something to route around: where it is, how big, and where
 * a line can touch it. Null for one that does not say its size.
 */
export function outlineOf(
  n: { type: string; position: { x: number; y: number }; data?: Record<string, any> },
  measured?: { width: number; height: number },
): Outline | null {
  // The file's size first. The canvas measures to whole pixels, and a box of
  // 183.6 measured as 184 put its middle a fifth of a pixel off the box under
  // it: the router took that for two lines side by side and set them 8 apart.
  const width = Number(n.data?.width) || measured?.width || 0;
  const height = Number(n.data?.height) || measured?.height || 0;
  if (!width || !height) return null;
  const box: Outline = { x: n.position.x, y: n.position.y, width, height };
  if (n.type === 'shape') {
    const shape = shapeOf(n.data?.shapeType);
    if (shape.reach) box.reach = shape.reach;
    if (shape.hull) box.hull = shape.hull;
  }
  return box;
}

/**
 * The words under a figure or an icon — a person, a server — which the app
 * draws below its box, out of it (`labelBelow`, ShapeNode.vue): about where
 * they are, or null for an item with its words inside it, or none.
 *
 * Its box is the drawing alone. Laid out from that, the two lines between a
 * customer and the app under them came out of the person's feet and ran
 * through "Khách hàng".
 */
export function captionOf(n: { type: string; position: { x: number; y: number }; data?: Record<string, any> }): Outline | null {
  if (n.type !== 'shape') return null;
  const label = typeof n.data?.label === 'string' ? n.data.label.trim() : '';
  const below = n.data?.shapeType === GLYPH_SHAPE || !!SHAPES_MAP[n.data?.shapeType]?.labelBelow;
  const width = Number(n.data?.width) || 0;
  const height = Number(n.data?.height) || 0;
  if (!label || !below || !width || !height) return null;
  // The words have the box's width and 48 more each side, from 4 under it,
  // and wrap past that; at 13px a letter is about 7 across, a line 18 down.
  const font = Number(n.data?.fontSize) || 13;
  const room = width + 96;
  const text = Math.ceil((label.length * 7 * font) / 13);
  const lines = Math.ceil(text / room);
  const across = Math.min(text, room);
  return { x: n.position.x + (width - across) / 2, y: n.position.y + height, width: across, height: 4 + lines * Math.ceil((font * 18) / 13) };
}

/**
 * Route the lines of a board that are waiting for it, in place. Returns
 * whether any line changed.
 *
 * The boxes are read from the file rather than from the canvas, so this can
 * run before the board is drawn. Every line Syn asks to have routed is
 * between boxes it sized. A line that cannot be routed — an end with no size,
 * a box to itself — has its flag taken off all the same: asking again on the
 * next open would get the same answer.
 */
export async function routePending(board: { nodes: WBNode[]; edges: WBEdge[] }): Promise<boolean> {
  const waiting = board.edges.filter(waitsForRoute);
  if (!waiting.length) return false;
  const boxes = new Map<string, Outline>();
  for (const n of board.nodes) {
    if (n.type === 'frame' || n.type === 'stroke') continue;
    const box = outlineOf(n);
    if (box) boxes.set(n.id, box);
  }
  const routes = await routeLines(boxes, waiting);
  for (const edge of waiting) {
    const route = routes.get(edge.id);
    if (route) {
      applyRoute(edge, route);
    } else {
      const data = { ...(edge.data ?? {}) };
      delete data.routing;
      edge.data = data;
      stampElement(edge);
    }
  }
  return true;
}
