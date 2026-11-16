import type { Box } from './inkLayer';

/**
 * Where a line meets the things it joins, and which way it goes between them.
 *
 * A line used to be fixed to the side it was drawn from: drag the box it
 * points at round to the other side, and the line went out of the far side
 * and wrapped back across the box. A line set to choose its sides (`sides:
 * 'auto'`) takes the sides that face each other, wherever the two things are
 * now; a stepped one also goes around what is in its way rather than through
 * it.
 */

export type Side = 'top' | 'right' | 'bottom' | 'left';
export interface Point { x: number; y: number }

const centre = (b: Box): Point => ({ x: b.x + b.width / 2, y: b.y + b.height / 2 });

/** The sides of `a` and `b` that face each other. */
export function sidesFacing(a: Box, b: Box): { from: Side; to: Side } {
  const ca = centre(a);
  const cb = centre(b);
  const dx = cb.x - ca.x;
  const dy = cb.y - ca.y;
  // Weighed by the boxes' sizes, so two wide boxes one above the other are
  // joined top to bottom even when they are a little apart sideways.
  const across = Math.abs(dx) / ((a.width + b.width) / 2 || 1);
  const down = Math.abs(dy) / ((a.height + b.height) / 2 || 1);
  if (across >= down) return dx >= 0 ? { from: 'right', to: 'left' } : { from: 'left', to: 'right' };
  return dy >= 0 ? { from: 'bottom', to: 'top' } : { from: 'top', to: 'bottom' };
}

const SIDES: readonly Side[] = ['top', 'right', 'bottom', 'left'];
export const isSide = (s: unknown): s is Side => SIDES.includes(s as Side);

/**
 * Where on a box a line ends, when it says: a side, and how far along it.
 *
 * `t` runs from 0 to 1 — left to right along the top and bottom, top to bottom
 * down the sides — and the middle is 0.5, which is where every line used to
 * end. Four lines into one side met at one point, arrowheads on top of each
 * other; a router that spreads them out (ELK, MSAGL) writes where each one
 * goes, and this is how the line finds it again after the box has moved.
 */
export interface LineEnd { side: Side; t: number }

export function lineEndOf(at: unknown): LineEnd | null {
  const end = at as Partial<LineEnd> | null | undefined;
  if (!end || !isSide(end.side)) return null;
  const t = typeof end.t === 'number' && Number.isFinite(end.t) ? Math.min(1, Math.max(0, end.t)) : 0.5;
  return { side: end.side, t };
}

/** A point `t` of the way along one side of a box. */
export function anchorAt(b: Box, side: Side, t: number): Point {
  switch (side) {
    case 'top': return { x: b.x + b.width * t, y: b.y };
    case 'bottom': return { x: b.x + b.width * t, y: b.y + b.height };
    case 'left': return { x: b.x, y: b.y + b.height * t };
    case 'right': return { x: b.x + b.width, y: b.y + b.height * t };
  }
}

/**
 * A path through these points, its corners rounded.
 *
 * A stepped line drawn with square corners reads as a diagram from a
 * spreadsheet; every tool people compare this board with rounds them. The
 * radius shrinks where a run is shorter than two of them, so a short jog
 * stays a jog rather than becoming a loop.
 */
export function roundedPath(points: Point[], radius = 8): string {
  const pts = points.filter((p, i) => i === 0 || p.x !== points[i - 1].x || p.y !== points[i - 1].y);
  if (!pts.length) return '';
  let d = `M${pts[0].x},${pts[0].y}`;
  for (let i = 1; i < pts.length - 1; i++) {
    const [a, c, b] = [pts[i - 1], pts[i], pts[i + 1]];
    const inLen = Math.hypot(c.x - a.x, c.y - a.y);
    const outLen = Math.hypot(b.x - c.x, b.y - c.y);
    const k = Math.min(radius, inLen / 2, outLen / 2);
    if (k <= 0) {
      d += ` L${c.x},${c.y}`;
      continue;
    }
    const from = { x: c.x - ((c.x - a.x) / inLen) * k, y: c.y - ((c.y - a.y) / inLen) * k };
    const to = { x: c.x + ((b.x - c.x) / outLen) * k, y: c.y + ((b.y - c.y) / outLen) * k };
    d += ` L${from.x},${from.y} Q${c.x},${c.y} ${to.x},${to.y}`;
  }
  if (pts.length > 1) d += ` L${pts[pts.length - 1].x},${pts[pts.length - 1].y}`;
  return d;
}

/** How far an end slides along its side to meet its line square-on. */
export const SQUARE_SNAP = 6;

/** A line end: where it is, the side it is on, and the box it is on. */
export interface SquareEnd extends Point {
  side: Side;
  box?: Box | null;
}

const level = (side: Side) => side === 'left' || side === 'right';

/**
 * A line through bends, made square: it leaves its first box straight out of
 * the side it is on, comes into the last one straight in, and is upright and
 * level all the way.
 *
 * The bends are worked out for the boxes where the file has them, and the
 * ends are drawn on the boxes where the canvas has them: a few pixels apart
 * after any rounding, and the steps between them, up or down first
 * (`stepThrough`), put a sliver of a step at each end. The line came in
 * sideways for its last pixel, and its arrowhead pointed along the sliver.
 *
 * So an end within `SQUARE_SNAP` of its first bend's line slides along its
 * side onto it, as long as it stays on the side; one further off takes a
 * corner that leaves or arrives the way its side faces. Bends within a pixel
 * of each other's line are put on it, and a bend in a straight run is
 * dropped, so no corner is rounded that is not a corner.
 */
export function squareThrough(start: SquareEnd, bends: Point[], end: SquareEnd, snap = SQUARE_SNAP): Point[] {
  const a: Point = { x: start.x, y: start.y };
  const z: Point = { x: end.x, y: end.y };
  const mid = bends.map((p) => ({ x: p.x, y: p.y }));
  const slide = (p: Point, side: Side, box: Box | null | undefined, toward: Point) => {
    if (level(side)) {
      const d = toward.y - p.y;
      if (d !== 0 && Math.abs(d) <= snap && (!box || (toward.y > box.y && toward.y < box.y + box.height))) p.y = toward.y;
    } else {
      const d = toward.x - p.x;
      if (d !== 0 && Math.abs(d) <= snap && (!box || (toward.x > box.x && toward.x < box.x + box.width))) p.x = toward.x;
    }
  };
  // Bends a pixel off each other's line are on it.
  const all = [a, ...mid, z];
  // The end moves too, by less than a pixel: moving the bend before it
  // instead would take that bend off the run before.
  for (let i = 1; i < all.length; i++) {
    const [p, q] = [all[i - 1], all[i]];
    if (Math.abs(q.x - p.x) < 1) q.x = p.x;
    if (Math.abs(q.y - p.y) < 1) q.y = p.y;
  }
  if (mid.length) {
    slide(a, start.side, start.box, mid[0]);
    slide(z, end.side, end.box, mid[mid.length - 1]);
  } else if (level(start.side) === level(end.side)) {
    // Facing each other, nearly in line: the far end meets the near one.
    slide(z, end.side, end.box, a);
  }
  // Upright and level runs, the ones at the ends the way their sides face.
  const out: Point[] = [all[0]];
  for (let i = 1; i < all.length; i++) {
    const [p, q] = [out[out.length - 1], all[i]];
    if (p.x !== q.x && p.y !== q.y) {
      const first = i === 1;
      const last = i === all.length - 1;
      let across: boolean;
      if (first) across = level(start.side);
      else if (last) across = !level(end.side);
      else across = false;
      out.push(across ? { x: q.x, y: p.y } : { x: p.x, y: q.y });
    }
    out.push(q);
  }
  // No bend in a straight run, and no run of no length.
  return out.filter((p, i) => {
    if (i === 0 || i === out.length - 1) return true;
    const [b, c] = [out[i - 1], out[i + 1]];
    if (p.x === b.x && p.y === b.y) return false;
    return !((b.x === p.x && p.x === c.x) || (b.y === p.y && p.y === c.y));
  });
}

/**
 * Points joined by upright and level runs: from each point, up or down first,
 * then across. The same steps a stepped line through bends has always taken
 * (`d3.curveStepBefore`), as points, so the corners can be rounded.
 */
export function stepThrough(points: Point[]): Point[] {
  const out: Point[] = [];
  points.forEach((p, i) => {
    if (i > 0) {
      const prev = points[i - 1];
      if (prev.x !== p.x && prev.y !== p.y) out.push({ x: prev.x, y: p.y });
    }
    out.push(p);
  });
  return out;
}

/** The middle of a box's side. */
export function anchor(b: Box, side: Side): Point {
  switch (side) {
    case 'top': return { x: b.x + b.width / 2, y: b.y };
    case 'bottom': return { x: b.x + b.width / 2, y: b.y + b.height };
    case 'left': return { x: b.x, y: b.y + b.height / 2 };
    case 'right': return { x: b.x + b.width, y: b.y + b.height / 2 };
  }
}

const out = (p: Point, side: Side, by: number): Point => {
  switch (side) {
    case 'top': return { x: p.x, y: p.y - by };
    case 'bottom': return { x: p.x, y: p.y + by };
    case 'left': return { x: p.x - by, y: p.y };
    case 'right': return { x: p.x + by, y: p.y };
  }
};

/** Whether an upright or level segment passes through the inside of a box. */
function crosses(a: Point, b: Point, box: Box): boolean {
  const x0 = Math.min(a.x, b.x);
  const x1 = Math.max(a.x, b.x);
  const y0 = Math.min(a.y, b.y);
  const y1 = Math.max(a.y, b.y);
  return x1 > box.x && x0 < box.x + box.width && y1 > box.y && y0 < box.y + box.height;
}

function cost(points: Point[], obstacles: Box[]): number {
  let hits = 0;
  let length = 0;
  for (let i = 1; i < points.length; i++) {
    length += Math.abs(points[i].x - points[i - 1].x) + Math.abs(points[i].y - points[i - 1].y);
    for (const o of obstacles) if (crosses(points[i - 1], points[i], o)) hits++;
  }
  return hits * 100_000 + length + points.length * 20;
}

/** The same path without points that do not turn. */
function tidy(points: Point[]): Point[] {
  const outp: Point[] = [];
  for (const p of points) {
    const a = outp[outp.length - 2];
    const b = outp[outp.length - 1];
    if (b && p.x === b.x && p.y === b.y) continue;
    if (a && b && ((a.x === b.x && b.x === p.x) || (a.y === b.y && b.y === p.y))) outp[outp.length - 1] = p;
    else outp.push(p);
  }
  return outp;
}

const MARGIN = 24;

/**
 * A path of upright and level runs from `start` (leaving by `startSide`) to
 * `end` (arriving by `endSide`), crossing as few of `obstacles` as it can and,
 * among those, the shortest. The candidates are the few shapes a person
 * would draw — an L, a Z either way, a detour around everything in the way —
 * which is enough between two boxes and costs nothing to work out.
 */
export function orthogonalRoute(start: Point, startSide: Side, end: Point, endSide: Side, obstacles: Box[]): Point[] {
  const p1 = out(start, startSide, MARGIN);
  const p2 = out(end, endSide, MARGIN);
  const mid = { x: (p1.x + p2.x) / 2, y: (p1.y + p2.y) / 2 };
  const between = obstacles.filter((o) =>
    o.x < Math.max(p1.x, p2.x) + MARGIN && o.x + o.width > Math.min(p1.x, p2.x) - MARGIN
    && o.y < Math.max(p1.y, p2.y) + MARGIN && o.y + o.height > Math.min(p1.y, p2.y) - MARGIN);
  const around = between.length
    ? {
      left: Math.min(...between.map((o) => o.x)) - MARGIN,
      right: Math.max(...between.map((o) => o.x + o.width)) + MARGIN,
      top: Math.min(...between.map((o) => o.y)) - MARGIN,
      bottom: Math.max(...between.map((o) => o.y + o.height)) + MARGIN,
    }
    : null;

  const middles: Point[][] = [
    [{ x: p2.x, y: p1.y }],
    [{ x: p1.x, y: p2.y }],
    [{ x: mid.x, y: p1.y }, { x: mid.x, y: p2.y }],
    [{ x: p1.x, y: mid.y }, { x: p2.x, y: mid.y }],
  ];
  if (around) {
    for (const x of [around.left, around.right]) middles.push([{ x, y: p1.y }, { x, y: p2.y }]);
    for (const y of [around.top, around.bottom]) middles.push([{ x: p1.x, y }, { x: p2.x, y }]);
  }

  // Every candidate stays inside this box, so only what is in it can be in
  // the way: tested against those, not against every item on the board, once
  // per run of every candidate.
  const reach = {
    x0: Math.min(start.x, end.x, around?.left ?? Infinity) - MARGIN,
    x1: Math.max(start.x, end.x, around?.right ?? -Infinity) + MARGIN,
    y0: Math.min(start.y, end.y, around?.top ?? Infinity) - MARGIN,
    y1: Math.max(start.y, end.y, around?.bottom ?? -Infinity) + MARGIN,
  };
  const near = obstacles.filter((o) => o.x < reach.x1 && o.x + o.width > reach.x0 && o.y < reach.y1 && o.y + o.height > reach.y0);

  let best: Point[] = [];
  let bestCost = Infinity;
  for (const m of middles) {
    const path = [start, p1, ...m, p2, end];
    const c = cost(path, near);
    if (c < bestCost) { bestCost = c; best = path; }
  }
  return tidy(best);
}
