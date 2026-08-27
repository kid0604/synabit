import { describe, it, expect } from 'vitest';
import { cornerInWorld, resizeRotatedBox, rotateVector } from '../boxGeometry';
import type { Box, Corner } from '../boxGeometry';

const box: Box = { x: 100, y: 100, width: 200, height: 100 };

const closeTo = (a: { x: number; y: number }, b: { x: number; y: number }, digits = 6) => {
  expect(a.x).toBeCloseTo(b.x, digits);
  expect(a.y).toBeCloseTo(b.y, digits);
};

describe('turning a vector', () => {
  it('turns clockwise, in screen axes where y points down', () => {
    // A quarter turn clockwise takes "right" to "down".
    closeTo(rotateVector({ x: 1, y: 0 }, 90), { x: 0, y: 1 });
    closeTo(rotateVector({ x: 0, y: 1 }, 90), { x: -1, y: 0 });
  });

  it('leaves a vector alone at no turn and at a full one', () => {
    closeTo(rotateVector({ x: 3, y: -7 }, 0), { x: 3, y: -7 });
    closeTo(rotateVector({ x: 3, y: -7 }, 360), { x: 3, y: -7 });
  });
});

describe('dragging a corner of an upright box', () => {
  it('grows away from the corner that stays put', () => {
    const next = resizeRotatedBox(box, 0, 'se', { x: 50, y: 30 });
    expect(next).toEqual({ x: 100, y: 100, width: 250, height: 130 });
  });

  it('moves the origin when the top-left is the corner being dragged', () => {
    const next = resizeRotatedBox(box, 0, 'nw', { x: -50, y: -30 });
    expect(next.width).toBeCloseTo(250);
    expect(next.height).toBeCloseTo(130);
    expect(next.x).toBeCloseTo(50);
    expect(next.y).toBeCloseTo(70);
  });

  it('never shrinks past the smallest size allowed', () => {
    const next = resizeRotatedBox(box, 0, 'se', { x: -1000, y: -1000 }, { minSize: 20 });
    expect(next.width).toBe(20);
    expect(next.height).toBe(20);
  });
});

describe('dragging a corner of a turned box', () => {
  it('widens the picture along its own axis, not the world’s', () => {
    // Turned a quarter clockwise, the picture's own width now runs down the
    // screen — so dragging downwards is what makes it wider.
    const next = resizeRotatedBox(box, 90, 'se', { x: 0, y: 60 });
    expect(next.width).toBeCloseTo(260);
    expect(next.height).toBeCloseTo(100);
  });

  it('does not widen it when the pointer moves the way the world is wide', () => {
    const next = resizeRotatedBox(box, 90, 'se', { x: 60, y: 0 });
    expect(next.width).toBeCloseTo(200);
    expect(next.height).toBeCloseTo(40);
  });

  it.each<[Corner, number]>([
    ['se', 30],
    ['nw', 30],
    ['ne', 45],
    ['sw', 120],
    ['se', 200],
    ['nw', 337],
  ])('holds the opposite corner still when dragging %s at %s°', (corner, degrees) => {
    const anchor: Record<Corner, Corner> = { nw: 'se', se: 'nw', ne: 'sw', sw: 'ne' };
    const before = cornerInWorld(box, degrees, anchor[corner]);

    const next = resizeRotatedBox(box, degrees, corner, { x: 37, y: -19 });
    const after = cornerInWorld(next, degrees, anchor[corner]);

    // This is the whole point: changing the size moves the middle, and the
    // middle is what the turn is about — so the position has to be corrected
    // or the picture slides out from under the pointer.
    closeTo(after, before, 6);
  });

  it('holds the opposite corner still while keeping the shape', () => {
    const next = resizeRotatedBox(box, 42, 'ne', { x: 25, y: 25 }, { keepAspect: true });

    expect(next.width / next.height).toBeCloseTo(box.width / box.height, 6);
    closeTo(cornerInWorld(next, 42, 'sw'), cornerInWorld(box, 42, 'sw'), 6);
  });

  it('is the plain case again at no turn', () => {
    const turned = resizeRotatedBox(box, 0, 'sw', { x: -40, y: 20 });
    expect(turned).toEqual({ x: 60, y: 100, width: 240, height: 120 });
  });
});
