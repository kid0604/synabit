import { describe, expect, it } from 'vitest';
import { fitPicture, READABLE, type Bounds } from '../boardCanvas';

const box = (left: number, top: number, w: number, h: number): Bounds => ({ left, top, right: left + w, bottom: top + h, span: w, drop: h });

/** How a board Syn drew sits in the answer. See `fitPicture`. */
describe('fitPicture', () => {
  it('shows a small board at its own size, centred, with nothing to drag', () => {
    const fit = fitPicture(box(100, 50, 400, 200), 900);
    expect(fit.zoom).toBe(1);
    expect(fit.height).toBe(240);
    expect(fit.overflows).toBe(false);
    // Centred across: (900 - 400) / 2, less where the board starts.
    expect(fit.viewport.x).toBe(250 - 100);
  });

  it('shrinks a board a little wider than the answer to fit it', () => {
    const fit = fitPicture(box(0, 0, 1000, 300), 900);
    expect(fit.zoom).toBeCloseTo(860 / 1000);
    expect(fit.overflows).toBe(false);
  });

  it('keeps a long swimlane readable and lets it be dragged, rather than shrinking it to smudges', () => {
    const fit = fitPicture(box(0, 0, 2900, 300), 900);
    expect(fit.zoom).toBe(READABLE);
    expect(fit.overflows).toBe(true);
    // From its left edge, so it starts where a reader starts.
    expect(fit.viewport.x).toBe(20);
  });

  it('does not ask to be dragged for a board that fits once its margin is given up', () => {
    // At the readable zoom this is 690 tall: over 720 with its margins, under without.
    const fit = fitPicture(box(0, 0, 600, 920), 900);
    expect(fit.zoom).toBe(READABLE);
    expect(fit.height).toBe(720);
    expect(fit.overflows).toBe(false);
  });
});
