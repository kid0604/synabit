import { describe, expect, it } from 'vitest';
import { inflateSync } from 'node:zlib';
import { readPhotoFacts } from '../../../shared/exif';
import { centredFrame, collage, coloursFor, frameFace, idPixels, paletteSmallPng, quantize, sheetGrid, withDpi, withShotAt } from '../photo';

/** A small JPEG-shaped file: SOI, a JFIF block, a scan, EOI. */
const jfif = () =>
  new Uint8Array([0xff, 0xd8, 0xff, 0xe0, 0, 16, 0x4a, 0x46, 0x49, 0x46, 0, 1, 1, 0, 0, 72, 0, 72, 0, 0, 0xff, 0xda, 0, 2, 1, 0xff, 0xd9]);

describe('a PNG with fewer colours', () => {
  it('keeps a picture of few colours exactly', () => {
    const rgba = new Uint8Array(4 * 4);
    rgba.set([255, 0, 0, 255, 0, 0, 255, 255, 255, 0, 0, 255, 0, 0, 0, 0]);
    const { palette, indices } = quantize(rgba, 8);
    const back = Array.from(indices, (i) => Array.from(palette.subarray(i * 4, i * 4 + 4)));
    expect(back).toEqual([[255, 0, 0, 255], [0, 0, 255, 255], [255, 0, 0, 255], [0, 0, 0, 0]]);
  });

  it('writes a palette PNG a reader can take apart', async () => {
    const palette = new Uint8Array([255, 255, 255, 255, 0, 0, 0, 128]);
    const indices = new Uint8Array([0, 1, 1, 0, 0, 1]);
    const png = await paletteSmallPng(3, 2, palette, indices);
    expect(Array.from(png.subarray(0, 8))).toEqual([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
    const view = new DataView(png.buffer);
    const chunks: Record<string, Uint8Array> = {};
    for (let at = 8; at < png.length; ) {
      const len = view.getUint32(at);
      chunks[String.fromCharCode(...png.subarray(at + 4, at + 8))] = png.subarray(at + 8, at + 8 + len);
      at += 12 + len;
    }
    expect(Array.from(chunks.IHDR.subarray(8, 10))).toEqual([8, 3]);
    expect(Array.from(chunks.tRNS)).toEqual([255, 128]);
    // Two rows, each its filter byte and its three indices.
    expect(Array.from(inflateSync(chunks.IDAT))).toEqual([0, 0, 1, 1, 0, 0, 0, 1]);
  });

  it('maps the quality slider to between 32 and 256 colours', () => {
    expect(coloursFor(0.3)).toBe(32);
    expect(coloursFor(0.95)).toBe(256);
  });
});

describe('what a JPEG says about itself', () => {
  it('carries the date it was taken after being redrawn', () => {
    const out = withShotAt(jfif(), '2026-05-01 07:30:15');
    expect(readPhotoFacts(out).shotAt).toBe('2026-05-01 07:30:15');
    expect(readPhotoFacts(out).orientation).toBe(1);
    // After the JFIF block, and the rest untouched.
    expect(out[20]).toBe(0xff);
    expect(out[21]).toBe(0xe1);
    expect(Array.from(out.subarray(out.length - 7))).toEqual([0xff, 0xda, 0, 2, 1, 0xff, 0xd9]);
    expect(withShotAt(jfif(), 'not a date')).toEqual(jfif());
  });

  it('says how many dots to the inch it is', () => {
    const out = withDpi(jfif(), 300);
    expect(Array.from(out.subarray(13, 18))).toEqual([1, 1, 44, 1, 44]);
    const bare = withDpi(new Uint8Array([0xff, 0xd8, 0xff, 0xda, 0, 2, 1, 0xff, 0xd9]), 300);
    expect(String.fromCharCode(...bare.subarray(6, 10))).toBe('JFIF');
  });
});

describe('an ID photo', () => {
  it('is the size a photo printer prints', () => {
    expect(idPixels('3x4')).toEqual([354, 472]);
    expect(idPixels('4x6')).toEqual([472, 709]);
  });

  it('puts the face where an ID photo has it', () => {
    // A 3000 × 4000 photo, a face 600 px tall in the middle.
    const r = frameFace({ x: 0.4, y: 0.35, w: 0.2, h: 0.15 }, 3000, 4000, 3 / 4);
    expect(r.height).toBeCloseTo(1200);
    expect(r.width).toBeCloseTo(900);
    expect(r.x + r.width / 2).toBeCloseTo(1500);
    expect((0.35 * 4000 - r.y) / r.height).toBeCloseTo(0.28);
    // A face near the edge: the frame stays inside the picture.
    const edge = frameFace({ x: 0, y: 0, w: 0.2, h: 0.15 }, 3000, 4000, 3 / 4);
    expect(edge.x).toBe(0);
    expect(edge.y).toBe(0);
    expect(centredFrame(400, 300, 1)).toEqual({ x: 50, y: 0, width: 300, height: 300 });
  });

  it('fits as many as there is room for on a sheet', () => {
    // 10 × 15 cm at 300 dpi, 3 × 4 cm photos, 24 px apart.
    expect(sheetGrid([1181, 1772], idPixels('3x4'), 24)).toEqual([3, 3]);
  });
});

describe('pictures put together', () => {
  it('stacks them at one width, or side by side at one height', () => {
    const v = collage([[1000, 500], [500, 500]], 'vertical', 10);
    expect(v.boxes.map((b) => [b.width, b.height])).toEqual([[1000, 500], [1000, 1000]]);
    expect([v.width, v.height]).toEqual([1020, 1530]);
    const h = collage([[1000, 500], [500, 500]], 'horizontal', 0);
    expect(h.boxes.map((b) => [b.width, b.height])).toEqual([[1000, 500], [500, 500]]);
  });

  it('puts both sides of a card on one A4 sheet, and never grows past the limit', () => {
    const s = collage([[856, 540], [856, 540]], 'sheet', 60);
    expect([s.width, s.height]).toEqual([1654, 2339]);
    expect(s.boxes[0].y + s.boxes[0].height).toBeLessThanOrEqual(s.boxes[1].y);
    const big = collage(Array.from({ length: 30 }, () => [4000, 3000] as [number, number]), 'vertical', 0, 6000);
    expect(Math.max(big.width, big.height)).toBe(6000);
  });
});
