/**
 * Resizing a box that has been turned.
 *
 * A node on the canvas is stored as an upright box — a top-left corner and a
 * size — and turned by rotating the element about its own middle. Dragging a
 * corner of a turned box is where that representation stops being free:
 *
 *   - the pointer moves in the world's axes, and the box grows along its own,
 *     which are no longer the same. Dragging the handle that *looks* like the
 *     bottom-right corner has to widen the picture the way it looks, not the
 *     way the world is oriented.
 *   - the corner opposite the one being dragged has to stay where it is. For
 *     an upright box that falls out of moving the top-left corner; for a
 *     turned one it does not, because changing the size moves the middle, and
 *     the middle is what the turn is about.
 *
 * Both are handled here, in world units, with no DOM in sight.
 */

export interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Point {
  x: number;
  y: number;
}

/** The four corners a drag can start from. */
export type Corner = 'nw' | 'ne' | 'se' | 'sw';

const DEG = Math.PI / 180;

/** Turn a vector clockwise by `degrees`, in screen axes (y downwards). */
export function rotateVector(v: Point, degrees: number): Point {
  const angle = degrees * DEG;
  const cos = Math.cos(angle);
  const sin = Math.sin(angle);
  return { x: v.x * cos - v.y * sin, y: v.x * sin + v.y * cos };
}

/** Which way each corner pushes the box's own width and height. */
const CORNER_SIGNS: Record<Corner, { x: number; y: number }> = {
  se: { x: 1, y: 1 },
  ne: { x: 1, y: -1 },
  sw: { x: -1, y: 1 },
  nw: { x: -1, y: -1 },
};

/** The corner that must not move while this one is dragged. */
const OPPOSITE: Record<Corner, Corner> = { nw: 'se', se: 'nw', ne: 'sw', sw: 'ne' };

/** A corner's position inside a box of this size, in the box's own axes. */
function cornerOf(corner: Corner, width: number, height: number): Point {
  return {
    x: corner === 'nw' || corner === 'sw' ? 0 : width,
    y: corner === 'nw' || corner === 'ne' ? 0 : height,
  };
}

/** Where a point of the box lands in the world once the box is turned. */
function toWorld(box: Box, degrees: number, local: Point): Point {
  const middle = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  const fromMiddle = { x: local.x - box.width / 2, y: local.y - box.height / 2 };
  const turned = rotateVector(fromMiddle, degrees);
  return { x: middle.x + turned.x, y: middle.y + turned.y };
}

/**
 * The box that results from dragging one corner of a turned box.
 *
 * `delta` is how far the pointer has moved since the drag started, in world
 * units — canvas coordinates, not screen pixels, so the caller divides by the
 * zoom first.
 */
export function resizeRotatedBox(
  start: Box,
  degrees: number,
  corner: Corner,
  delta: Point,
  options: { keepAspect?: boolean; minSize?: number } = {}
): Box {
  const { keepAspect = false, minSize = 20 } = options;

  // The pointer's movement, said in the box's own axes rather than the
  // world's. Turning it back by the same angle is the whole trick.
  const local = rotateVector(delta, -degrees);
  const sign = CORNER_SIGNS[corner];

  let width = Math.max(minSize, start.width + sign.x * local.x);
  let height = Math.max(minSize, start.height + sign.y * local.y);

  if (keepAspect && start.width > 0 && start.height > 0) {
    // Follow whichever axis the user pushed further, so the drag tracks the
    // pointer rather than lagging behind on one side.
    const scale =
      Math.abs(width / start.width - 1) >= Math.abs(height / start.height - 1)
        ? width / start.width
        : height / start.height;
    width = Math.max(minSize, start.width * scale);
    height = Math.max(minSize, start.height * scale);
  }

  // Hold the opposite corner still: work out where it is now, then place the
  // new box so that the same corner of it lands in the same spot.
  const anchor = OPPOSITE[corner];
  const anchorWorld = toWorld(start, degrees, cornerOf(anchor, start.width, start.height));

  const anchorLocal = cornerOf(anchor, width, height);
  const fromMiddle = { x: anchorLocal.x - width / 2, y: anchorLocal.y - height / 2 };
  const turned = rotateVector(fromMiddle, degrees);

  return {
    x: anchorWorld.x - turned.x - width / 2,
    y: anchorWorld.y - turned.y - height / 2,
    width,
    height,
  };
}

/** Where a corner of a turned box sits in the world. Exported for tests. */
export function cornerInWorld(box: Box, degrees: number, corner: Corner): Point {
  return toWorld(box, degrees, cornerOf(corner, box.width, box.height));
}
