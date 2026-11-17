import { corners, drawnPath, labelShare, runsThrough, type Outline, type Point, type Route } from './autoRoute';
import type { Side } from './routing';

/**
 * Lines a router drew, moved off one another: as few crossings as the boxes
 * allow.
 *
 * # Why
 *
 * Because a diagram is read by following its lines, and every place two of
 * them cross is a place the eye has to choose. On one layer of a circuit
 * board two traces never cross — they cannot — and a drawing laid out with
 * that discipline reads at a glance. The router (MSAGL) finds the shortest
 * way around the boxes for each line and knows nothing of the others; on the
 * CMDB board Syn drew, that was 25 crossings among 45 lines, many of them
 * where a line could as well have left its box by another side. Moved off
 * one another without care, they ran along one another instead — worse,
 * since two lines on top of each other read as one — so that costs more
 * still.
 *
 * # How
 *
 * The boxes stay where they are; only the lines move. Each line that crosses
 * or runs along another is taken up and tried again a few dozen ways — every
 * pair of sides that can reach each other, a few places along each side, and
 * the shapes a person would draw between them: an L, a Z with its middle
 * run at one of a few places between them, a detour around what is in
 * between. The one that costs least is kept, a crossing costing far
 * more than any length or bend, and the rounds go on until none of them
 * gets cheaper. Each move makes the whole drawing cheaper, so it ends.
 *
 * Only lines a router drew are moved. A line bent by hand is somebody's
 * decision, and is passed in as `around`: what the others go round, never
 * moved itself. A drawing with crossings on purpose is drawn by hand.
 */

/** What a crossing costs, against a pixel of length and a bend. */
const CROSSING = 1000;
/**
 * Two lines along one another for a stretch read as one line, and which of
 * them goes where is lost: worse than a crossing, which is at least plain.
 */
const ALONG = 1500;
/** Closer than this, two runs side by side are one run (the router keeps them 8 apart). */
const APART = 6;
const BEND = 30;
const PIXEL = 0.25;
/** How far a line runs straight out of its box before it may turn. */
const STUB = 20;
/** How many rounds at most: in practice two or three settle it. */
const ROUNDS = 4;
/** How many pairs of sides are tried at every place along them. */
const PROMISING = 3;

/** A line to untangle: its ends, and the route it has, if any. */
export interface Tangled {
  id: string;
  source: string;
  target: string;
  route?: Route;
}

type Seg = [Point, Point];

const segsOf = (path: Point[]): Seg[] => path.slice(1).map((q, i) => [path[i], q]);

const orient = (p: Point, q: Point, r: Point) => (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);

/** Whether two runs cross: each passes through the other, neither just touches it. */
function crossing([a, b]: Seg, [c, d]: Seg): boolean {
  if (Math.max(a.x, b.x) < Math.min(c.x, d.x) || Math.max(c.x, d.x) < Math.min(a.x, b.x)) return false;
  if (Math.max(a.y, b.y) < Math.min(c.y, d.y) || Math.max(c.y, d.y) < Math.min(a.y, b.y)) return false;
  const d1 = orient(c, d, a);
  const d2 = orient(c, d, b);
  const d3 = orient(a, b, c);
  const d4 = orient(a, b, d);
  return ((d1 > 0.01 && d2 < -0.01) || (d1 < -0.01 && d2 > 0.01)) && ((d3 > 0.01 && d4 < -0.01) || (d3 < -0.01 && d4 > 0.01));
}

/** Whether two runs lie along one another for more than a couple of pixels. */
function along([a, b]: Seg, [c, d]: Seg): boolean {
  if (Math.abs(a.y - b.y) < 0.5 && Math.abs(c.y - d.y) < 0.5 && Math.abs(a.y - c.y) < APART) {
    return Math.min(Math.max(a.x, b.x), Math.max(c.x, d.x)) - Math.max(Math.min(a.x, b.x), Math.min(c.x, d.x)) > 2;
  }
  if (Math.abs(a.x - b.x) < 0.5 && Math.abs(c.x - d.x) < 0.5 && Math.abs(a.x - c.x) < APART) {
    return Math.min(Math.max(a.y, b.y), Math.max(c.y, d.y)) - Math.max(Math.min(a.y, b.y), Math.min(c.y, d.y)) > 2;
  }
  return false;
}

/** How many times one path crosses others, and how many stretches it shares with them. */
export function meetings(path: Point[], others: Iterable<Point[]>): { crossings: number; along: number } {
  const segs: Seg[] = [];
  for (const other of others) segs.push(...segsOf(other));
  return meetingSegs(path, segs);
}

/** The same, against the others' runs cut up once: asked of every way tried. */
function meetingSegs(path: Point[], segs: Seg[]): { crossings: number; along: number } {
  const mine = segsOf(path);
  let crossings = 0;
  let shared = 0;
  for (const s of segs) {
    for (const m of mine) {
      if (crossing(m, s)) crossings++;
      else if (along(m, s)) shared++;
    }
  }
  return { crossings, along: shared };
}

/** Every crossing on a drawing: each pair of paths, each time they cross. */
export function crossingsIn(paths: Point[][]): number {
  let n = 0;
  for (let i = 0; i < paths.length; i++) n += meetings(paths[i], paths.slice(i + 1)).crossings;
  return n;
}

/** What a drawing's lines cost to read, all of them: what `untangle` brings down one line at a time. */
export function drawingCost(paths: Point[][]): number {
  let cost = 0;
  for (let i = 0; i < paths.length; i++) {
    const met = meetings(paths[i], paths.slice(i + 1));
    cost += CROSSING * met.crossings + ALONG * met.along + BEND * (paths[i].length - 2) + PIXEL * lengthOf(paths[i]);
  }
  return cost;
}

const lengthOf = (path: Point[]) => segsOf(path).reduce((sum, [p, q]) => sum + Math.abs(q.x - p.x) + Math.abs(q.y - p.y), 0);

const SIDES: Side[] = ['top', 'right', 'bottom', 'left'];
const level = (side: Side) => side === 'left' || side === 'right';
const normal: Record<Side, Point> = { top: { x: 0, y: -1 }, bottom: { x: 0, y: 1 }, left: { x: -1, y: 0 }, right: { x: 1, y: 0 } };

const pointOn = (b: Outline, side: Side, t: number): Point => {
  switch (side) {
    case 'top': return { x: b.x + b.width * t, y: b.y };
    case 'bottom': return { x: b.x + b.width * t, y: b.y + b.height };
    case 'left': return { x: b.x, y: b.y + b.height * t };
    case 'right': return { x: b.x + b.width, y: b.y + b.height * t };
  }
};

/** Places along a side, and along the way between two ends, to try: the middle first. */
const SPREAD = [0.5, 0.35, 0.65, 0.2, 0.8];

/**
 * Where on a side a line may end: three places along a side the shape runs
 * along, its middle only where the shape just touches it (a diamond's corner).
 */
function placesOn(b: Outline, side: Side): number[] {
  if (!b.reach) return SPREAD;
  const r = b.reach[side];
  if (!r) return [0.5];
  const [lo, hi] = r;
  return [...new Set(SPREAD.map((t) => Math.round((lo + (hi - lo) * t) * 1000) / 1000))];
}

/**
 * The sides of a box a line to `toward` may leave by: any but the one facing
 * straight away. A side facing half away is how a line goes round the
 * outside of everything, which is sometimes the only way not to cross.
 */
function sidesToward(b: Outline, toward: Point): Side[] {
  const c = { x: b.x + b.width / 2, y: b.y + b.height / 2 };
  const d = { x: toward.x - c.x, y: toward.y - c.y };
  const n = Math.hypot(d.x, d.y) || 1;
  return SIDES.filter((s) => (normal[s].x * d.x + normal[s].y * d.y) / n > -0.9);
}

const stub = (p: Point, side: Side): Point => ({ x: p.x + normal[side].x * STUB, y: p.y + normal[side].y * STUB });

/**
 * The shapes a line can take from one end to the other: straight when the two
 * ends face each other in line, an L either way, a Z with its middle at a
 * few places between them, and the way around everything between.
 */
function shapesBetween(start: Point, startSide: Side, end: Point, endSide: Side, boxes: Outline[]): Point[][] {
  const p1 = stub(start, startSide);
  const p2 = stub(end, endSide);
  const out: Point[][] = [];
  const facing = normal[startSide].x === -normal[endSide].x && normal[startSide].y === -normal[endSide].y;
  if (facing && (level(startSide) ? Math.abs(start.y - end.y) < 0.5 : Math.abs(start.x - end.x) < 0.5)) out.push([start, end]);
  const middles: Point[][] = [[{ x: p2.x, y: p1.y }], [{ x: p1.x, y: p2.y }]];
  for (const f of SPREAD) {
    const x = p1.x + (p2.x - p1.x) * f;
    const y = p1.y + (p2.y - p1.y) * f;
    middles.push([{ x, y: p1.y }, { x, y: p2.y }], [{ x: p1.x, y }, { x: p2.x, y }]);
  }
  const x0 = Math.min(p1.x, p2.x);
  const x1 = Math.max(p1.x, p2.x);
  const y0 = Math.min(p1.y, p2.y);
  const y1 = Math.max(p1.y, p2.y);
  const between = boxes.filter((o) => o.x < x1 && o.x + o.width > x0 && o.y < y1 && o.y + o.height > y0);
  if (between.length) {
    const left = Math.min(...between.map((o) => o.x)) - STUB;
    const right = Math.max(...between.map((o) => o.x + o.width)) + STUB;
    const top = Math.min(...between.map((o) => o.y)) - STUB;
    const bottom = Math.max(...between.map((o) => o.y + o.height)) + STUB;
    for (const x of [left, right]) middles.push([{ x, y: p1.y }, { x, y: p2.y }]);
    for (const y of [top, bottom]) middles.push([{ x: p1.x, y }, { x: p2.x, y }]);
  }
  for (const m of middles) out.push(corners([start, p1, ...m, p2, end]));
  return out;
}

/** Whether a path, past its first and last runs, goes back into either of its own boxes. */
function backInto(path: Point[], own: Outline[]): boolean {
  for (let i = 2; i < path.length - 1; i++) {
    const [p, q] = [path[i - 1], path[i]];
    for (const o of own) {
      const x0 = Math.max(Math.min(p.x, q.x), o.x + 1);
      const x1 = Math.min(Math.max(p.x, q.x), o.x + o.width - 1);
      const y0 = Math.max(Math.min(p.y, q.y), o.y + 1);
      const y1 = Math.min(Math.max(p.y, q.y), o.y + o.height - 1);
      if (x0 <= x1 && y0 <= y1) return true;
    }
  }
  return false;
}

/** Whether a path turns back on itself: a run straight back along the one before. */
function doublesBack(path: Point[]): boolean {
  for (let i = 2; i < path.length; i++) {
    const [a, b, c] = [path[i - 2], path[i - 1], path[i]];
    if ((b.x - a.x) * (c.x - b.x) + (b.y - a.y) * (c.y - b.y) < 0) return true;
  }
  return false;
}

/**
 * The routes to change: by id, only those that got cheaper — fewer crossings
 * and stretches shared with other lines, then fewer bends and less length.
 * A line with no route yet (the router could not find one) gets the best
 * way there is, if there is one that goes through nothing.
 *
 * `boxes` is what lines must keep out of, each line's own ends included;
 * `around`, the paths of lines that are not to move.
 */
export function untangle(lines: Tangled[], boxes: Map<string, Outline>, around: Point[][] = []): Map<string, Route> {
  const all = [...boxes.values()];
  const movable = lines.filter((l) => l.source !== l.target && boxes.has(l.source) && boxes.has(l.target));
  const paths = new Map<string, Point[]>();
  for (const l of movable) {
    if (l.route) paths.set(l.id, drawnPath(l.route, boxes.get(l.source)!, boxes.get(l.target)!));
  }
  const routes = new Map<string, Route>(movable.filter((l) => l.route).map((l) => [l.id, l.route!]));
  const changed = new Map<string, Route>();

  const others = (id: string) => [...[...paths].filter(([k]) => k !== id).map(([, p]) => p), ...around];
  const costOf = (path: Point[], id: string) => {
    const met = meetings(path, others(id));
    return CROSSING * met.crossings + ALONG * met.along + BEND * (path.length - 2) + PIXEL * lengthOf(path);
  };

  for (let round = 0; round < ROUNDS; round++) {
    let better = false;
    // The most tangled first: freeing it often frees the others.
    const order = movable
      .map((l) => {
        const path = paths.get(l.id);
        const met = path ? meetings(path, others(l.id)) : { crossings: Infinity, along: 0 };
        return { l, knots: met.crossings * 2 + met.along };
      })
      .filter((x) => x.knots > 0)
      .sort((a, b) => b.knots - a.knots);
    for (const { l } of order) {
      const from = boxes.get(l.source)!;
      const to = boxes.get(l.target)!;
      const current = paths.get(l.id);
      let best = current ? costOf(current, l.id) : Infinity;
      let bestRoute: Route | null = null;
      let bestPath: Point[] | null = null;
      const near: Seg[] = others(l.id).flatMap(segsOf);
      const toMiddle = { x: to.x + to.width / 2, y: to.y + to.height / 2 };
      const fromMiddle = { x: from.x + from.width / 2, y: from.y + from.height / 2 };
      /** One way: from this place on this side to that one on that side; the cheapest of its shapes. */
      const tryWay = (sa: Side, ta: number, sb: Side, tb: number): number => {
        let cheapest = Infinity;
        const start = pointOn(from, sa, ta);
        const end = pointOn(to, sb, tb);
        for (const path of shapesBetween(start, sa, end, sb, all)) {
          // Length and bends first, then crossings: cheap, and most ways
          // lose on them alone. Through a box last, for the few that win.
          const floor = BEND * (path.length - 2) + PIXEL * lengthOf(path);
          if (floor >= best || doublesBack(path)) continue;
          const met = meetingSegs(path, near);
          const cost = floor + CROSSING * met.crossings + ALONG * met.along;
          if (cost >= best) continue;
          if (backInto(path, [from, to]) || runsThrough(path, boxes, l)) continue;
          cheapest = Math.min(cheapest, cost);
          best = cost;
          bestPath = path;
          bestRoute = {
            waypoints: path.slice(1, -1).map((p) => ({ x: Math.round(p.x * 10) / 10, y: Math.round(p.y * 10) / 10 })),
            sourceAt: { side: sa, t: ta },
            targetAt: { side: sb, t: tb },
            labelAt: labelShare(path),
          };
        }
        return cheapest;
      };
      // Every pair of sides from their middles, then the most promising
      // pairs from each place along them: a fifth of the ways, and the ones
      // that win are almost always among them.
      const pairs: { sa: Side; sb: Side; cost: number }[] = [];
      for (const sa of sidesToward(from, toMiddle)) {
        for (const sb of sidesToward(to, fromMiddle)) {
          const [ta, tb] = [placesOn(from, sa)[0], placesOn(to, sb)[0]];
          pairs.push({ sa, sb, cost: Math.min(tryWay(sa, ta, sb, tb), best) });
        }
      }
      if (l.route) pairs.push({ sa: l.route.sourceAt.side, sb: l.route.targetAt.side, cost: -1 });
      const promising = pairs.sort((x, y) => x.cost - y.cost).slice(0, PROMISING);
      for (const { sa, sb } of promising) {
        for (const ta of placesOn(from, sa)) {
          for (const tb of placesOn(to, sb)) tryWay(sa, ta, sb, tb);
        }
      }
      if (bestRoute && bestPath) {
        paths.set(l.id, bestPath);
        routes.set(l.id, bestRoute);
        changed.set(l.id, bestRoute);
        better = true;
      }
    }
    if (!better) break;
  }
  return changed;
}
