/**
 * The arithmetic of the picture tools, kept apart from the canvas so it can
 * be tested: fewer colours for a smaller PNG, the date a photo was taken
 * written back into a JPEG, the dots-per-inch a print shop reads, where a
 * face goes in an ID photo, and how pictures are laid out side by side.
 */

// ── A PNG with fewer colours ───────────────────────────────────────────────

/**
 * A picture's colours cut down to at most `colours` (median cut, alpha
 * included): the palette, and for each pixel the palette entry it takes.
 *
 * A canvas can only write a PNG with every colour it has, which is why
 * "compress" used to leave PNGs as large as they were. With a palette of 256
 * or fewer, a screenshot or a drawing is usually a third of the size and looks
 * the same.
 */
export function quantize(rgba: Uint8Array | Uint8ClampedArray, colours: number): { palette: Uint8Array; indices: Uint8Array } {
  const n = rgba.length / 4;
  const want = Math.max(2, Math.min(256, Math.floor(colours)));
  // The palette is found from at most 250 000 pixels, evenly spread.
  const step = Math.max(1, Math.floor(n / 250_000));
  const sample: number[] = [];
  for (let i = 0; i < n; i += step) sample.push(i * 4);
  type Bin = number[];
  const range = (bin: Bin) => {
    let best = 0;
    let channel = 0;
    for (let c = 0; c < 4; c++) {
      let lo = 255;
      let hi = 0;
      for (const p of bin) {
        const v = rgba[p + c];
        if (v < lo) lo = v;
        if (v > hi) hi = v;
      }
      if (hi - lo > best) [best, channel] = [hi - lo, c];
    }
    return { best, channel };
  };
  const bins: Bin[] = [sample];
  while (bins.length < want) {
    // Split the bin that spans the most, at its middle pixel.
    let pick = -1;
    let widest = 0;
    let along = 0;
    bins.forEach((bin, i) => {
      if (bin.length < 2) return;
      const { best, channel } = range(bin);
      if (best * Math.log2(bin.length + 1) > widest) [widest, pick, along] = [best * Math.log2(bin.length + 1), i, channel];
    });
    if (pick < 0 || widest === 0) break;
    const bin = bins[pick].sort((a, b) => rgba[a + along] - rgba[b + along]);
    const half = bin.length >> 1;
    bins.splice(pick, 1, bin.slice(0, half), bin.slice(half));
  }
  const palette = new Uint8Array(bins.length * 4);
  bins.forEach((bin, i) => {
    const sum = [0, 0, 0, 0];
    for (const p of bin) for (let c = 0; c < 4; c++) sum[c] += rgba[p + c];
    for (let c = 0; c < 4; c++) palette[i * 4 + c] = Math.round(sum[c] / bin.length);
  });
  // Each pixel to its nearest entry, remembered by a coarse key: neighbouring
  // pixels are mostly the same colour, so few are worked out twice.
  const indices = new Uint8Array(n);
  const memo = new Map<number, number>();
  const entries = bins.length;
  for (let i = 0; i < n; i++) {
    const p = i * 4;
    const [r, g, b, a] = [rgba[p], rgba[p + 1], rgba[p + 2], rgba[p + 3]];
    const key = ((r >> 2) << 18) | ((g >> 2) << 12) | ((b >> 2) << 6) | (a >> 2);
    let found = memo.get(key);
    if (found === undefined) {
      let best = Infinity;
      found = 0;
      for (let e = 0; e < entries; e++) {
        const q = e * 4;
        const d = (r - palette[q]) ** 2 + (g - palette[q + 1]) ** 2 + (b - palette[q + 2]) ** 2 + (a - palette[q + 3]) ** 2;
        if (d < best) [best, found] = [d, e];
      }
      memo.set(key, found);
    }
    indices[i] = found;
  }
  return { palette, indices };
}

const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();

function crc32(parts: Uint8Array[]): number {
  let c = 0xffffffff;
  for (const bytes of parts) for (const b of bytes) c = CRC_TABLE[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type: string, data: Uint8Array): Uint8Array {
  const out = new Uint8Array(12 + data.length);
  const view = new DataView(out.buffer);
  view.setUint32(0, data.length);
  const name = new TextEncoder().encode(type);
  out.set(name, 4);
  out.set(data, 8);
  view.setUint32(8 + data.length, crc32([name, data]));
  return out;
}

async function zlib(data: Uint8Array): Promise<Uint8Array> {
  // "deflate" is the zlib format PNG's image data is in.
  const source = new ReadableStream<Uint8Array>({
    start(controller) {
      controller.enqueue(data);
      controller.close();
    },
  });
  const stream = source.pipeThrough(new CompressionStream('deflate') as unknown as ReadableWritablePair<Uint8Array, Uint8Array>);
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

/** An 8-bit palette PNG of `indices` (one byte a pixel, row by row). */
export async function paletteSmallPng(width: number, height: number, palette: Uint8Array, indices: Uint8Array): Promise<Uint8Array> {
  const entries = palette.length / 4;
  const header = new Uint8Array(13);
  const hv = new DataView(header.buffer);
  hv.setUint32(0, width);
  hv.setUint32(4, height);
  header.set([8, 3, 0, 0, 0], 8);
  const rgb = new Uint8Array(entries * 3);
  const alpha = new Uint8Array(entries);
  let lastClear = -1;
  for (let e = 0; e < entries; e++) {
    rgb.set(palette.subarray(e * 4, e * 4 + 3), e * 3);
    alpha[e] = palette[e * 4 + 3];
    if (alpha[e] < 255) lastClear = e;
  }
  // Each row starts with its filter: none, which suits palette data best.
  const raw = new Uint8Array((width + 1) * height);
  for (let y = 0; y < height; y++) raw.set(indices.subarray(y * width, (y + 1) * width), y * (width + 1) + 1);
  const parts = [
    new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', header),
    chunk('PLTE', rgb),
    ...(lastClear >= 0 ? [chunk('tRNS', alpha.subarray(0, lastClear + 1))] : []),
    chunk('IDAT', await zlib(raw)),
    chunk('IEND', new Uint8Array()),
  ];
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  parts.reduce((at, p) => (out.set(p, at), at + p.length), 0);
  return out;
}

/** "Quality" 0.3–0.95 as how many colours a PNG keeps: 32 to 256. */
export function coloursFor(quality: number): number {
  return Math.round(32 + Math.min(1, Math.max(0, (quality - 0.3) / 0.65)) * 224);
}

// ── What a JPEG says about itself ──────────────────────────────────────────

/**
 * A JPEG with an EXIF block saying when it was taken (and that it is upright,
 * which a redrawn picture is). A canvas writes no EXIF at all, so a photo
 * compressed or resized came out with no date, and a photo library sorted it
 * by the day it was compressed. `shotAt` is `YYYY-MM-DD HH:MM:SS`.
 */
export function withShotAt(jpeg: Uint8Array, shotAt: string): Uint8Array {
  const m = shotAt.match(/^(\d{4})-(\d{2})-(\d{2}) (\d{2}):(\d{2}):(\d{2})$/);
  if (!m || jpeg[0] !== 0xff || jpeg[1] !== 0xd8) return jpeg;
  const date = new TextEncoder().encode(`${m[1]}:${m[2]}:${m[3]} ${m[4]}:${m[5]}:${m[6]}\0`);
  // Big-endian TIFF: IFD0 with the orientation and a pointer to the EXIF IFD,
  // which holds DateTimeOriginal.
  const tiff = new Uint8Array(56 + 20);
  const v = new DataView(tiff.buffer);
  tiff.set([0x4d, 0x4d, 0, 0x2a]);
  v.setUint32(4, 8);
  v.setUint16(8, 2);
  v.setUint16(10, 0x0112);
  v.setUint16(12, 3);
  v.setUint32(14, 1);
  v.setUint16(18, 1);
  v.setUint16(22, 0x8769);
  v.setUint16(24, 4);
  v.setUint32(26, 1);
  v.setUint32(30, 38);
  v.setUint32(34, 0);
  v.setUint16(38, 1);
  v.setUint16(40, 0x9003);
  v.setUint16(42, 2);
  v.setUint32(44, 20);
  v.setUint32(48, 56);
  v.setUint32(52, 0);
  tiff.set(date, 56);
  const body = new Uint8Array(6 + tiff.length);
  body.set(new TextEncoder().encode('Exif\0\0'));
  body.set(tiff, 6);
  const app1 = new Uint8Array(4 + body.length);
  app1.set([0xff, 0xe1, (body.length + 2) >> 8, (body.length + 2) & 0xff]);
  app1.set(body, 4);
  // After the JFIF block if there is one, as cameras and readers expect.
  const at = jpeg[2] === 0xff && jpeg[3] === 0xe0 ? 4 + ((jpeg[4] << 8) | jpeg[5]) : 2;
  const out = new Uint8Array(jpeg.length + app1.length);
  out.set(jpeg.subarray(0, at));
  out.set(app1, at);
  out.set(jpeg.subarray(at), at + app1.length);
  return out;
}

/**
 * A JPEG that says it is `dpi` dots to the inch, so a print shop prints a
 * 354 × 472 photo at 3 × 4 cm. A canvas writes 72 or none.
 */
export function withDpi(jpeg: Uint8Array, dpi: number): Uint8Array {
  if (jpeg[0] !== 0xff || jpeg[1] !== 0xd8) return jpeg;
  const isJfif = jpeg[2] === 0xff && jpeg[3] === 0xe0 && String.fromCharCode(...jpeg.subarray(6, 10)) === 'JFIF';
  if (isJfif) {
    const out = jpeg.slice();
    out[13] = 1;
    out[14] = dpi >> 8;
    out[15] = dpi & 0xff;
    out[16] = dpi >> 8;
    out[17] = dpi & 0xff;
    return out;
  }
  const app0 = new Uint8Array([0xff, 0xe0, 0, 16, 0x4a, 0x46, 0x49, 0x46, 0, 1, 1, 1, dpi >> 8, dpi & 0xff, dpi >> 8, dpi & 0xff, 0, 0]);
  const out = new Uint8Array(jpeg.length + app0.length);
  out.set(jpeg.subarray(0, 2));
  out.set(app0, 2);
  out.set(jpeg.subarray(2), 2 + app0.length);
  return out;
}

// ── ID photos ──────────────────────────────────────────────────────────────

/** Sizes in centimetres, width × height. */
export const ID_SIZES = {
  '3x4': [3, 4],
  '4x6': [4, 6],
  '2x3': [2, 3],
  '35x45': [3.5, 4.5],
  '5x5': [5, 5],
} as const;
export type IdSize = keyof typeof ID_SIZES;

/** Pixels for a size at 300 dots an inch, what photo printers print at. */
export function idPixels(size: IdSize, dpi = 300): [number, number] {
  const [w, h] = ID_SIZES[size];
  return [Math.round((w / 2.54) * dpi), Math.round((h / 2.54) * dpi)];
}

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/**
 * Where to cut an ID photo around a face, in the picture's pixels: the face
 * Vision found is about half the photo's height, its top a little over a
 * quarter of the way down — room above for the hair, below for the shoulders
 * — and centred across. Kept inside the picture, smaller if it must be.
 */
export function frameFace(face: { x: number; y: number; w: number; h: number }, width: number, height: number, ratio: number): Rect {
  let h = (face.h * height) / 0.5;
  let w = h * ratio;
  const fit = Math.min(1, width / w, height / h);
  [w, h] = [w * fit, h * fit];
  const cx = (face.x + face.w / 2) * width;
  const top = face.y * height - 0.28 * h;
  return {
    x: Math.min(width - w, Math.max(0, cx - w / 2)),
    y: Math.min(height - h, Math.max(0, top)),
    width: w,
    height: h,
  };
}

/** The largest box of this ratio in the middle of a picture: no face to go by. */
export function centredFrame(width: number, height: number, ratio: number): Rect {
  const w = Math.min(width, height * ratio);
  const h = w / ratio;
  return { x: (width - w) / 2, y: (height - h) / 2, width: w, height: h };
}

/** How many photos of `photo` pixels fit on a sheet, with gaps for cutting: columns and rows. */
export function sheetGrid(sheet: [number, number], photo: [number, number], gap: number): [number, number] {
  const fit = (room: number, size: number) => Math.max(0, Math.floor((room + gap) / (size + gap)));
  return [fit(sheet[0] - 2 * gap, photo[0]), fit(sheet[1] - 2 * gap, photo[1])];
}

// ── Pictures side by side ──────────────────────────────────────────────────

export type CollageLayout = 'vertical' | 'horizontal' | 'grid' | 'sheet';

/**
 * Where each picture goes when they are put together, and how large the
 * result is: one above the other at one width, side by side at one height, in
 * a grid of equal cells, or on an A4 sheet at 200 dpi (both sides of an ID
 * card, ready to print). Never more than `most` pixels across.
 */
export function collage(sizes: [number, number][], layout: CollageLayout, gap: number, most = 6000): { width: number; height: number; boxes: Rect[] } {
  const n = sizes.length;
  if (!n) return { width: 1, height: 1, boxes: [] };
  const fitIn = ([w, h]: [number, number], cell: Rect): Rect => {
    const s = Math.min(cell.width / w, cell.height / h);
    return { x: cell.x + (cell.width - w * s) / 2, y: cell.y + (cell.height - h * s) / 2, width: w * s, height: h * s };
  };
  let width: number;
  let height: number;
  let boxes: Rect[];
  if (layout === 'sheet') {
    [width, height] = [1654, 2339];
    const margin = 120;
    const cellH = (height - 2 * margin - gap * (n - 1)) / n;
    boxes = sizes.map((s, i) => fitIn(s, { x: margin, y: margin + i * (cellH + gap), width: width - 2 * margin, height: cellH }));
  } else if (layout === 'vertical') {
    width = Math.min(Math.max(...sizes.map(([w]) => w)), 3000);
    let y = gap;
    boxes = sizes.map(([w, h]) => {
      const box = { x: gap, y, width, height: (h * width) / w };
      y += box.height + gap;
      return box;
    });
    [width, height] = [width + 2 * gap, y];
  } else if (layout === 'horizontal') {
    height = Math.min(Math.max(...sizes.map(([, h]) => h)), 3000);
    let x = gap;
    boxes = sizes.map(([w, h]) => {
      const box = { x, y: gap, width: (w * height) / h, height };
      x += box.width + gap;
      return box;
    });
    [width, height] = [x, height + 2 * gap];
  } else {
    const cols = Math.ceil(Math.sqrt(n));
    const rows = Math.ceil(n / cols);
    const ratios = sizes.map(([w, h]) => h / w).sort((a, b) => a - b);
    const cell = { w: 1200, h: Math.round(1200 * ratios[Math.floor(n / 2)]) };
    boxes = sizes.map((s, i) => fitIn(s, { x: gap + (i % cols) * (cell.w + gap), y: gap + Math.floor(i / cols) * (cell.h + gap), width: cell.w, height: cell.h }));
    [width, height] = [gap + cols * (cell.w + gap), gap + rows * (cell.h + gap)];
  }
  const s = Math.min(1, most / Math.max(width, height));
  return {
    width: Math.max(1, Math.round(width * s)),
    height: Math.max(1, Math.round(height * s)),
    boxes: boxes.map((b) => ({ x: b.x * s, y: b.y * s, width: b.width * s, height: b.height * s })),
  };
}
