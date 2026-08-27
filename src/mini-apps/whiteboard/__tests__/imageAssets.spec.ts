import { describe, it, expect } from 'vitest';
import {
  fitWithin,
  filenameFor,
  mimeFor,
  normalizeAngle,
  rotatedOverhang,
  vaultJoin,
  MAX_PLACED_EDGE,
} from '../imageAssets';

describe('sizing a picture as it lands', () => {
  it('shrinks a photo to fit, keeping its shape', () => {
    // A phone photo is four thousand points wide. Dropped at that size it
    // covers the board and everything already on it.
    const fitted = fitWithin({ width: 4032, height: 3024 });
    expect(Math.max(fitted.width, fitted.height)).toBe(MAX_PLACED_EDGE);
    expect(fitted.width / fitted.height).toBeCloseTo(4032 / 3024, 2);
  });

  it('fits by the long edge, whichever way round the picture is', () => {
    const tall = fitWithin({ width: 300, height: 1200 }, 600);
    expect(tall).toEqual({ width: 150, height: 600 });
  });

  it('leaves a small picture alone rather than blowing it up', () => {
    expect(fitWithin({ width: 64, height: 64 })).toEqual({ width: 64, height: 64 });
  });

  it('gives a picture it could not measure a box to sit in', () => {
    expect(fitWithin({ width: 0, height: 0 })).toEqual({
      width: MAX_PLACED_EDGE,
      height: MAX_PLACED_EDGE,
    });
  });

  it('never rounds a sliver away to nothing', () => {
    const sliver = fitWithin({ width: 5000, height: 2 }, 480);
    expect(sliver.height).toBeGreaterThanOrEqual(1);
  });
});

describe('naming a picture', () => {
  it('keeps the name it came with', () => {
    expect(filenameFor({ name: 'diagram.png', type: 'image/png' })).toBe('diagram.png');
  });

  it('invents one for a paste, which arrives with none', () => {
    expect(filenameFor({ type: 'image/png' })).toBe('pasted-image.png');
    expect(filenameFor({ type: 'image/jpeg' })).toBe('pasted-image.jpeg');
  });

  it('does not put a plus sign in a filename', () => {
    // `image/svg+xml` is a real clipboard type.
    expect(filenameFor({ type: 'image/svg+xml' })).toBe('pasted-image.svg');
  });

  it('falls back to png rather than to no extension at all', () => {
    expect(filenameFor({})).toBe('pasted-image.png');
  });
});

describe('paths into the vault', () => {
  it('joins with the separator the vault path is already using', () => {
    expect(vaultJoin('/home/me/vault', 'assets/a.png')).toBe('/home/me/vault/assets/a.png');
    expect(vaultJoin('C:\\Users\\me\\vault', 'assets/a.png')).toBe('C:\\Users\\me\\vault\\assets\\a.png');
  });
});

describe('content types', () => {
  it('names the common picture formats', () => {
    expect(mimeFor('assets/a.png')).toBe('image/png');
    expect(mimeFor('assets/a.JPG')).toBe('image/jpeg');
    expect(mimeFor('assets/a.webp')).toBe('image/webp');
    expect(mimeFor('assets/a.svg')).toBe('image/svg+xml');
  });

  it('does not guess at something it does not know', () => {
    expect(mimeFor('assets/a.xyz')).toBe('application/octet-stream');
    expect(mimeFor('assets/noextension')).toBe('application/octet-stream');
  });
});

describe('turning a picture', () => {
  it('brings any angle into a single turn', () => {
    expect(normalizeAngle(0)).toBe(0);
    expect(normalizeAngle(360)).toBe(0);
    expect(normalizeAngle(450)).toBe(90);
    // Turned anticlockwise past the top, which is what dragging the handle
    // the short way round produces.
    expect(normalizeAngle(-90)).toBe(270);
    expect(normalizeAngle(-450)).toBe(270);
  });

  it('treats an angle that is not a number as no turn at all', () => {
    expect(normalizeAngle(Number.NaN)).toBe(0);
    expect(normalizeAngle(Number.POSITIVE_INFINITY)).toBe(0);
  });

  it('says an upright picture sticks out of nothing', () => {
    expect(rotatedOverhang(400, 300, 0)).toBe(0);
    expect(rotatedOverhang(400, 300, 360)).toBe(0);
  });

  it('measures the corner a turned picture pushes past its box', () => {
    // A square turned by an eighth of a turn is √2 times as wide, so it
    // overhangs by (√2 − 1)/2 of its side on every edge.
    const side = 100;
    expect(rotatedOverhang(side, side, 45)).toBeCloseTo((side * Math.SQRT2 - side) / 2, 5);
  });

  it('measures a quarter turn by how much longer the long side is', () => {
    // Upright box 400×300; turned on its side it is 300×400, so it is 50
    // past the box top and bottom.
    expect(rotatedOverhang(400, 300, 90)).toBeCloseTo(50, 5);
  });

  it('does not care which way the picture was turned', () => {
    expect(rotatedOverhang(400, 300, 30)).toBeCloseTo(rotatedOverhang(400, 300, 330), 5);
    expect(rotatedOverhang(400, 300, 30)).toBeCloseTo(rotatedOverhang(400, 300, 210), 5);
  });
});
