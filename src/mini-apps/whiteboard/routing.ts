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
