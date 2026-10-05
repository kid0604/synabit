import { describe, expect, it } from 'vitest';
import { anchor, orthogonalRoute, sidesFacing, type Point } from '../routing';
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
