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
