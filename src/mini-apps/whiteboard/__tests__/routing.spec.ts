import { describe, expect, it } from 'vitest';
import { anchor, anchorAt, lineEndOf, orthogonalRoute, roundedPath, sidesFacing, stepThrough, type Point } from '../routing';
import type { Box } from '../inkLayer';

const box = (x: number, y: number, width = 100, height = 60): Box => ({ x, y, width, height });

const throughAny = (path: Point[], boxes: Box[]) =>
  path.some((p, i) => i > 0 && boxes.some((b) => {
    const a = path[i - 1];
    return Math.max(a.x, p.x) > b.x && Math.min(a.x, p.x) < b.x + b.width && Math.max(a.y, p.y) > b.y && Math.min(a.y, p.y) < b.y + b.height;
  }));

describe('the sides a line takes', () => {
  it('takes the sides that face each other', () => {
    expect(sidesFacing(box(0, 0), box(300, 10))).toEqual({ from: 'right', to: 'left' });
    expect(sidesFacing(box(300, 10), box(0, 0))).toEqual({ from: 'left', to: 'right' });
    expect(sidesFacing(box(0, 0), box(20, 300))).toEqual({ from: 'bottom', to: 'top' });
  });

  it('joins two wide boxes stacked a little apart top to bottom', () => {
    expect(sidesFacing(box(0, 0, 400, 40), box(120, 100, 400, 40))).toEqual({ from: 'bottom', to: 'top' });
  });
});

describe('a stepped line', () => {
  it('goes around a box in its way', () => {
    const a = box(0, 0);
    const b = box(400, 0);
    const wall = box(180, -100, 60, 260);
    const path = orthogonalRoute(anchor(a, 'right'), 'right', anchor(b, 'left'), 'left', [a, b, wall]);
    expect(throughAny(path, [wall])).toBe(false);
    expect(path[0]).toEqual(anchor(a, 'right'));
    expect(path[path.length - 1]).toEqual(anchor(b, 'left'));
  });

  it('runs only upright and level, and straight when nothing is in the way', () => {
    const a = box(0, 0);
    const b = box(400, 0);
    const path = orthogonalRoute(anchor(a, 'right'), 'right', anchor(b, 'left'), 'left', [a, b]);
    expect(path).toHaveLength(2);
    const bent = orthogonalRoute(anchor(a, 'right'), 'right', anchor(box(400, 300), 'left'), 'left', [a]);
    bent.forEach((p, i) => { if (i) expect(p.x === bent[i - 1].x || p.y === bent[i - 1].y).toBe(true); });
  });
});

/**
 * A line's end anywhere along a side, rather than always at its middle: what
 * lets four lines into one side arrive at four points, and a router's spread
 * survive being drawn.
 */
describe('where along a side a line ends', () => {
  const b = box(100, 200, 200, 80);

  it('is the middle at one half, and the corners at the ends', () => {
    expect(anchorAt(b, 'top', 0.5)).toEqual(anchor(b, 'top'));
    expect(anchorAt(b, 'top', 0)).toEqual({ x: 100, y: 200 });
    expect(anchorAt(b, 'bottom', 1)).toEqual({ x: 300, y: 280 });
    expect(anchorAt(b, 'left', 0.25)).toEqual({ x: 100, y: 220 });
    expect(anchorAt(b, 'right', 0.75)).toEqual({ x: 300, y: 260 });
  });

  it('reads what a file says, and nothing that is not a side', () => {
    expect(lineEndOf({ side: 'left', t: 0.2 })).toEqual({ side: 'left', t: 0.2 });
    expect(lineEndOf({ side: 'left' })).toEqual({ side: 'left', t: 0.5 });
    expect(lineEndOf({ side: 'left', t: 7 })).toEqual({ side: 'left', t: 1 });
    expect(lineEndOf({ side: 'middle', t: 0.2 })).toBeNull();
    expect(lineEndOf(undefined)).toBeNull();
  });
});

describe('a stepped line, drawn', () => {
  it('rounds its corners and keeps its ends', () => {
    const d = roundedPath([{ x: 0, y: 0 }, { x: 100, y: 0 }, { x: 100, y: 100 }], 8);
    expect(d).toBe('M0,0 L92,0 Q100,0 100,8 L100,100');
  });

  it('rounds a short jog less, so it stays a jog', () => {
    const d = roundedPath([{ x: 0, y: 0 }, { x: 100, y: 0 }, { x: 100, y: 6 }, { x: 200, y: 6 }], 8);
    expect(d).toContain('L97,0 Q100,0 100,3');
  });

  it('steps between bends the way it always has: up or down, then across', () => {
    expect(stepThrough([{ x: 0, y: 0 }, { x: 50, y: 40 }, { x: 50, y: 90 }])).toEqual([
      { x: 0, y: 0 }, { x: 0, y: 40 }, { x: 50, y: 40 }, { x: 50, y: 90 },
    ]);
  });
});
