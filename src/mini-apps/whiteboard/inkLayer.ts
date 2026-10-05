/**
 * Ink drawn on its own layer.
 *
 * The canvas library builds every item it is given as a component of its own,
 * with its own bookkeeping, measuring and drag handling — right for a sticky
 * note, far too much for a pen stroke, of which a drawing has thousands. A
 * board of 5,000 strokes spent over two seconds just being opened.
 *
 * So a stroke goes to the canvas only while it needs to be an item there:
 * while it is selected (to be moved, deleted, copied, turned into a diagram)
 * or once it has been given a place in the stacking order of its own. The
 * rest are drawn as plain paths in one picture, only those in view.
 *
 * The board file does not change. A stroke is a node in it as before; this
 * decides only which of them the canvas library is handed.
 */

export interface Box { x: number; y: number; width: number; height: number }

interface InkLike {
  id: string;
  type: string;
  position: { x: number; y: number };
  data?: Record<string, any>;
  selected?: boolean;
}

/**
 * Whether an item is handed to the canvas library.
 *
 * Everything but ink is. Ink is when selected, and when it was brought
 * forward or sent back — one picture can only be stacked in one place, and
 * an item put somewhere particular has to be stacked where it was put.
 */
export function onCanvas(n: InkLike): boolean {
  return n.type !== 'stroke' || !!n.selected || typeof n.data?.z === 'number';
}

/** Ink drawn on the ink layer rather than by the canvas. */
export function isLooseInk(n: InkLike): boolean {
  return !onCanvas(n);
}

/**
 * How many selected strokes are made canvas items. Selecting a whole drawing
 * — Ctrl+A, a marquee across it — handed thousands of strokes to the canvas
 * at once, the very cost the ink layer exists to avoid. Past this many, the
 * selected strokes stay on the ink layer, marked there, and are moved,
 * deleted and copied from there.
 */
export const PROMOTE_LIMIT = 50;

/** What the canvas is handed, out of everything on the board. */
export function handedToCanvas<T extends InkLike>(all: T[]): T[] {
  let selectedInk = 0;
  for (const n of all) if (n.type === 'stroke' && n.selected && typeof n.data?.z !== 'number') selectedInk++;
  const promote = selectedInk <= PROMOTE_LIMIT;
  return all.filter((n) => n.type !== 'stroke' || typeof n.data?.z === 'number' || (!!n.selected && promote));
}

/**
 * The size of a stroke's box, from what it carries.
 *
 * A stroke drawn now saves its box. One drawn before that is measured from its
 * points, reaching half the pen's width past them, which is where the ink ends.
 */
export function strokeSize(data: Record<string, any> | undefined): { width: number; height: number } {
  if (data?.width && data?.height) return { width: data.width, height: data.height };
  let width = 1;
  let height = 1;
  for (const [x, y] of (data?.points as number[][] | undefined) ?? []) {
    if (x > width) width = x;
    if (y > height) height = y;
  }
  const reach = (data?.size || 3) / 2;
  return { width: Math.ceil(width + reach), height: Math.ceil(height + reach) };
}

export function inkBox(n: InkLike): Box {
  return { x: n.position.x, y: n.position.y, ...strokeSize(n.data) };
}

export function overlaps(a: Box, b: Box): boolean {
  return a.x <= b.x + b.width && b.x <= a.x + a.width && a.y <= b.y + b.height && b.y <= a.y + a.height;
}

export function contains(outer: Box, inner: Box): boolean {
  return inner.x >= outer.x && inner.y >= outer.y
    && inner.x + inner.width <= outer.x + outer.width
    && inner.y + inner.height <= outer.y + outer.height;
}

/**
 * The part of the board in view, in board coordinates, widened by `margin`
 * screen pixels on every side so a pan does not show ink arriving late.
 */
export function viewBox(
  viewport: { x: number; y: number; zoom: number },
  size: { width: number; height: number },
  margin = 200,
): Box {
  const z = viewport.zoom || 1;
  return {
    x: (-viewport.x - margin) / z,
    y: (-viewport.y - margin) / z,
    width: (size.width + margin * 2) / z,
    height: (size.height + margin * 2) / z,
  };
}

/** The strokes whose box is wholly inside a marquee — as the canvas selects. */
export function inkInside<T extends InkLike>(ink: T[], area: Box): T[] {
  return ink.filter((n) => contains(area, inkBox(n)));
}

/**
 * The full list after the canvas reports its own.
 *
 * The canvas hands back only what it was given (`handed`); the ink it was
 * never given is kept. Something it was given and no longer has, it deleted.
 */
export function withLooseInk<T extends InkLike>(fromCanvas: T[], before: T[], handed: Set<string>): T[] {
  const seen = new Set(fromCanvas.map((n) => n.id));
  return [...fromCanvas, ...before.filter((n) => !seen.has(n.id) && !handed.has(n.id) && n.type === 'stroke')];
}

/** The box around some boxes, or null for none. */
export function unionBox(boxes: Box[]): Box | null {
  if (!boxes.length) return null;
  // A loop, not Math.min(...): spreading a long list into arguments runs out
  // of stack somewhere past a hundred thousand boxes.
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const b of boxes) {
    if (b.x < x0) x0 = b.x;
    if (b.y < y0) y0 = b.y;
    if (b.x + b.width > x1) x1 = b.x + b.width;
    if (b.y + b.height > y1) y1 = b.y + b.height;
  }
  return { x: x0, y: y0, width: x1 - x0, height: y1 - y0 };
}
