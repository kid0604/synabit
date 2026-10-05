import { describe, expect, it } from 'vitest';
import { buildStroke, getSvgPathFromStroke } from '../composables/useFreeDrawing';
import { INK, isInk, paint } from '../ink';

const line = (from: [number, number], to: [number, number], steps = 20) =>
  Array.from({ length: steps + 1 }, (_, i) => [
    from[0] + ((to[0] - from[0]) * i) / steps,
    from[1] + ((to[1] - from[1]) * i) / steps,
    0.5,
  ]);

describe('buildStroke', () => {
  it('boxes the ink, not just the line it was drawn along', () => {
    const size = 16;
    const built = buildStroke(line([100, 200], [300, 200]), size)!;

    // The ink reaches past the ends and above and below the line.
    expect(built.x).toBeLessThan(100);
    expect(built.y).toBeLessThan(200);
    expect(built.x + built.width).toBeGreaterThan(300);
    expect(built.y + built.height).toBeGreaterThan(200);
  });

  it('keeps the points on the board where they were drawn', () => {
    const drawn = line([40.123, 60.987], [90, 75]);
    const built = buildStroke(drawn, 4)!;

    for (let i = 0; i < drawn.length; i++) {
      expect(built.x + built.points[i][0]).toBeCloseTo(drawn[i][0], 1);
      expect(built.y + built.points[i][1]).toBeCloseTo(drawn[i][1], 1);
    }
  });

  it('stores short numbers', () => {
    const built = buildStroke(line([0.333333, 0.777777], [10.111111, 20.999999]), 3)!;
    expect(Number.isInteger(built.x) && Number.isInteger(built.y)).toBe(true);
    for (const [x, y, p] of built.points) {
      for (const n of [x, y, p]) expect(Math.round(n * 100)).toBeCloseTo(n * 100, 6);
    }
    for (const token of built.svgPath.split(' ')) {
      const n = Number(token);
      if (!Number.isNaN(n)) expect((token.split('.')[1] ?? '').length).toBeLessThanOrEqual(2);
    }
  });

  it('draws nothing from a single point', () => {
    expect(buildStroke([[1, 1, 0.5]], 3)).toBeNull();
  });

  it('closes the path', () => {
    expect(getSvgPathFromStroke([[0, 0], [1, 0], [1, 1]]).endsWith('Z')).toBe(true);
    expect(getSvgPathFromStroke([])).toBe('');
  });
});

describe('ink', () => {
  it('paints the palettes’ blacks with the theme ink', () => {
    for (const black of ['#000', '#000000', '#1e1e1e', '#1E1E1E', ' #000000 ']) {
      expect(isInk(black)).toBe(true);
      expect(paint(black)).toBe(INK);
    }
  });

  it('leaves every other colour as it is', () => {
    expect(paint('#ef4444')).toBe('#ef4444');
    expect(paint('#1e1e1ecc')).toBe('#1e1e1ecc');
    expect(paint(undefined)).toBeUndefined();
    expect(paint('')).toBe('');
    expect(isInk(null)).toBe(false);
  });
});
