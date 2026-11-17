import { describe, expect, it } from 'vitest';
import { WHOLE, asScan, flatSize, squareToQuad, warp, type Quad } from '../straighten';

const close = (a: number, b: number) => Math.abs(a - b) < 1e-9;

describe('straightening a photo of a page', () => {
  const skewed: Quad = [
    [0.2, 0.1],
    [0.8, 0.15],
    [0.75, 0.9],
    [0.15, 0.85],
  ];

  it('maps the corners of the square to the corners of the page', () => {
    const map = squareToQuad(skewed);
    [[0, 0], [1, 0], [1, 1], [0, 1]].forEach(([u, v], i) => {
      const [x, y] = map(u, v);
      expect(close(x, skewed[i][0]) && close(y, skewed[i][1])).toBe(true);
    });
    // A parallelogram is the affine case.
    const [x, y] = squareToQuad(WHOLE)(0.25, 0.75);
    expect(close(x, 0.25) && close(y, 0.75)).toBe(true);
  });

  it('comes out the size the page was in the photo', () => {
    expect(flatSize(WHOLE, 400, 300)).toEqual([400, 300]);
    const [w, h] = flatSize(skewed, 1000, 1000);
    expect(w).toBeGreaterThan(590);
    expect(h).toBeGreaterThan(740);
    expect(Math.max(...flatSize(WHOLE, 8000, 6000))).toBe(3000);
  });

  it('pulls the page out of the photo and leaves the table behind', () => {
    // A 100 × 100 photo: dark table, a red page from 20 to 80 each way.
    const [sw, sh] = [100, 100];
    const src = new Uint8ClampedArray(sw * sh * 4);
    for (let y = 0; y < sh; y++) {
      for (let x = 0; x < sw; x++) {
        const page = x >= 20 && x < 80 && y >= 20 && y < 80;
        src.set(page ? [220, 20, 20, 255] : [30, 30, 30, 255], (y * sw + x) * 4);
      }
    }
    const quad: Quad = [[0.2, 0.2], [0.8, 0.2], [0.8, 0.8], [0.2, 0.8]];
    const out = warp(src, sw, sh, quad, 60, 60);
    let red = 0;
    for (let i = 0; i < out.length; i += 4) if (out[i] > 150 && out[i + 1] < 80) red++;
    // All page, but for the half pixel blended with the table at each edge.
    expect(red / (60 * 60)).toBeGreaterThan(0.95);
  });

  it('turns lamp-yellow paper white and keeps the ink dark', () => {
    const px = new Uint8ClampedArray(100 * 4);
    for (let i = 0; i < 100; i++) px.set(i < 10 ? [40, 35, 30, 255] : [220, 200, 150, 255], i * 4);
    asScan(px);
    // Every channel of the paper, not only red: the yellow is gone.
    expect([...px.slice(50 * 4, 50 * 4 + 3)]).toEqual([255, 255, 255]);
    expect(Math.max(...px.slice(0, 3))).toBeLessThan(25);
  });
});
