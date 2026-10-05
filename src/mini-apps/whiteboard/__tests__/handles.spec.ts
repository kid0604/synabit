import { describe, expect, it } from 'vitest';
import { SHAPES } from '../shapes';
import { outlineInsets, parsePath, samplePath } from '../pathGeometry';

/** How far a point is from the drawn outline, in path units. */
function distance([px, py]: [number, number], d: string): number {
  let best = Infinity;
  for (const { points, closed } of samplePath(parsePath(d)!, 32)) {
    const pts = closed ? [...points, points[0]] : points;
    for (let i = 1; i < pts.length; i++) {
      const [ax, ay] = pts[i - 1];
      const [bx, by] = pts[i];
      const dx = bx - ax, dy = by - ay;
      const len = dx * dx + dy * dy;
      const t = len ? Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / len)) : 0;
      best = Math.min(best, Math.hypot(ax + t * dx - px, ay + t * dy - py));
    }
  }
  return best;
}

describe('connection handles', () => {
  it('sit on the outline of every shape that crosses its middle', () => {
    const off: string[] = [];
    for (const shape of SHAPES) {
      const i = outlineInsets(shape.path);
      const at = (pct: number) => 2 + (pct / 100) * 96;
      const handles: Record<string, [number, number]> = {
        top: [50, at(i.top)], bottom: [50, 98 - (at(i.bottom) - 2)], left: [at(i.left), 50], right: [98 - (at(i.right) - 2), 50],
      };
      for (const [side, p] of Object.entries(handles)) {
        const d = distance(p, shape.path);
        // A side the outline never crosses falls back to the drawing's edge,
        // which may be in the air; every other handle is on the line.
        if (d > 1.5 && !(side === 'top' || side === 'bottom' ? crossesNot(shape.path, 'x') : crossesNot(shape.path, 'y'))) off.push(`${shape.id}.${side} ${d.toFixed(1)}`);
      }
    }
    expect(off).toEqual([]);
  });

  it('put the right handle on the right', () => {
    for (const shape of SHAPES) {
      const i = outlineInsets(shape.path);
      expect(i.left + i.right, shape.id).toBeLessThanOrEqual(100);
      expect(i.top + i.bottom, shape.id).toBeLessThanOrEqual(100);
    }
  });
});

/** Whether the outline never crosses the middle line across `axis` (x = 50 or y = 50). */
function crossesNot(d: string, axis: 'x' | 'y'): boolean {
  const k = axis === 'x' ? 0 : 1;
  for (const { points, closed } of samplePath(parsePath(d)!, 16)) {
    const pts = closed ? [...points, points[0]] : points;
    for (let i = 1; i < pts.length; i++) if ((pts[i - 1][k] - 50) * (pts[i][k] - 50) <= 0 && pts[i - 1][k] !== pts[i][k]) return false;
  }
  return true;
}
