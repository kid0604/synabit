/**
 * The picture tools' work on a canvas: what is drawn, and how it is written
 * out. The sums behind it are in `photo.ts`; here they meet the pixels.
 *
 * Every picture is read upright (the WebView applies EXIF orientation as it
 * decodes) and through `loadImage`, which asks in CORS mode so the canvas may
 * be read back.
 */
import { fileUrl, loadImage, type MarkPicture } from './render';
import { collage, coloursFor, paletteSmallPng, quantize, sheetGrid, withShotAt, type CollageLayout, type Rect } from './photo';
import { markCentres, turnedBox, type ImageFormat } from './tools';
import { readPhotoFacts } from '../../shared/exif';

function blob(canvas: HTMLCanvasElement, type: string, quality?: number): Promise<Blob> {
  return new Promise((resolve, reject) => canvas.toBlob((b) => (b ? resolve(b) : reject(new Error('the picture could not be encoded'))), type, quality));
}

/** A canvas's pixels as a file of this format. */
export async function encode(canvas: HTMLCanvasElement, format: ImageFormat, quality = 0.92): Promise<Uint8Array> {
  return new Uint8Array(await (await blob(canvas, `image/${format}`, quality)).arrayBuffer());
}

/** A blank canvas; JPEG has no transparency, so one made for it starts white. */
export function blank(width: number, height: number, white = false): HTMLCanvasElement {
  const canvas = document.createElement('canvas');
  canvas.width = Math.max(1, Math.round(width));
  canvas.height = Math.max(1, Math.round(height));
  if (white) {
    const ctx = canvas.getContext('2d')!;
    ctx.fillStyle = '#fff';
    ctx.fillRect(0, 0, canvas.width, canvas.height);
  }
  return canvas;
}

/** A picture on a canvas, upright, at most `most` pixels across. */
export function canvasOf(img: HTMLImageElement, most = 12000, white = false): HTMLCanvasElement {
  const s = Math.min(1, most / Math.max(img.naturalWidth, img.naturalHeight));
  const canvas = blank(img.naturalWidth * s, img.naturalHeight * s, white);
  const ctx = canvas.getContext('2d')!;
  ctx.imageSmoothingQuality = 'high';
  ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
  return canvas;
}

/** A canvas scaled by `s`, smoothly. */
function scaled(canvas: HTMLCanvasElement, s: number, white: boolean): HTMLCanvasElement {
  const out = blank(canvas.width * s, canvas.height * s, white);
  const ctx = out.getContext('2d')!;
  ctx.imageSmoothingQuality = 'high';
  ctx.drawImage(canvas, 0, 0, out.width, out.height);
  return out;
}

/** A PNG with at most `colours` colours: what makes a PNG smaller at all. */
export async function smallPng(canvas: HTMLCanvasElement, colours: number): Promise<Uint8Array> {
  const data = canvas.getContext('2d', { willReadFrequently: true })!.getImageData(0, 0, canvas.width, canvas.height).data;
  const { palette, indices } = quantize(data, colours);
  return paletteSmallPng(canvas.width, canvas.height, palette, indices);
}

/**
 * A picture written as small as `format` and `quality` make it — and, given
 * `target` bytes, no larger than that: the quality searched for first, then
 * the size made smaller until it fits. `fits` says whether it did.
 */
export async function shrink(
  source: HTMLCanvasElement,
  format: ImageFormat,
  quality: number,
  target = 0,
): Promise<{ bytes: Uint8Array; fits: boolean }> {
  const once = (c: HTMLCanvasElement, q: number) => (format === 'png' ? smallPng(c, coloursFor(q)) : encode(c, format, q));
  let canvas = source;
  let best = await once(canvas, quality);
  if (!target || best.length <= target) return { bytes: best, fits: true };
  for (let round = 0; round < 6; round++) {
    // The best quality that fits at this size, from 35% up to the one asked for.
    let [lo, hi] = [0.35, quality];
    let fitting: Uint8Array | null = null;
    for (let step = 0; step < 6; step++) {
      const q = (lo + hi) / 2;
      const bytes = await once(canvas, q);
      if (bytes.length <= target) [fitting, lo] = [bytes, q];
      else hi = q;
      if (bytes.length < best.length) best = bytes;
    }
    if (fitting) {
      if (canvas !== source) canvas.width = canvas.height = 0;
      return { bytes: fitting, fits: true };
    }
    // Still too large at the lowest quality: fewer pixels.
    const lowest = await once(canvas, 0.35);
    const next = scaled(canvas, Math.max(0.3, Math.sqrt(target / lowest.length) * 0.92), format === 'jpeg');
    if (canvas !== source) canvas.width = canvas.height = 0;
    canvas = next;
  }
  if (canvas !== source) canvas.width = canvas.height = 0;
  return { bytes: best, fits: best.length <= target };
}

/** When a JPEG was taken, from its EXIF (`YYYY-MM-DD HH:MM:SS`), or nothing. */
export async function shotAtOf(path: string, name: string): Promise<string | undefined> {
  if (!/\.(jpe?g|jfif)$/i.test(name)) return undefined;
  try {
    const head = new Uint8Array(await (await fetch(fileUrl(path))).arrayBuffer()).subarray(0, 256 * 1024);
    return readPhotoFacts(head).shotAt;
  } catch {
    return undefined;
  }
}

/** A JPEG given its date back; anything else as it is. */
export function dated(bytes: Uint8Array, format: ImageFormat, shotAt: string | undefined): Uint8Array {
  return format === 'jpeg' && shotAt ? withShotAt(bytes, shotAt) : bytes;
}

// ── Turned ─────────────────────────────────────────────────────────────────

/** A picture turned clockwise by quarter turns, then mirrored, redrawn. */
export function turnedCanvas(img: HTMLImageElement, turn: { quarters: number; flipH: boolean; flipV: boolean }, white: boolean): HTMLCanvasElement {
  const q = ((turn.quarters % 4) + 4) % 4;
  const [w, h] = [img.naturalWidth, img.naturalHeight];
  const canvas = blank(q % 2 ? h : w, q % 2 ? w : h, white);
  const ctx = canvas.getContext('2d')!;
  ctx.translate(canvas.width / 2, canvas.height / 2);
  ctx.scale(turn.flipH ? -1 : 1, turn.flipV ? -1 : 1);
  ctx.rotate((q * Math.PI) / 2);
  ctx.drawImage(img, -w / 2, -h / 2);
  return canvas;
}

// ── Watermark ──────────────────────────────────────────────────────────────

export interface MarkLayout {
  position: number;
  tile: boolean;
  /** The mark's width as a share of the picture's. */
  size: number;
  /** Degrees, clockwise. */
  rotate: number;
}

/**
 * A mark (text or a logo, already drawn with its opacity) laid over a
 * picture: the same places and sizes `stamp.rs` uses on a PDF page, so the
 * two watermark tools agree.
 */
export async function markedCanvas(img: HTMLImageElement, mark: MarkPicture, layout: MarkLayout, most = 12000, white = false): Promise<HTMLCanvasElement> {
  const canvas = canvasOf(img, most, white);
  const [W, H] = [canvas.width, canvas.height];
  const picture = new Image();
  picture.src = mark.url;
  await picture.decode();
  const w = Math.min(1, Math.max(0.05, layout.size)) * W;
  const h = (w * mark.height) / mark.width;
  const ctx = canvas.getContext('2d')!;
  // `markCentres` counts up from the bottom, as PDF does.
  for (const [cx, cy] of markCentres(layout, [W, H], turnedBox(w, h, layout.rotate))) {
    ctx.save();
    ctx.translate(cx, H - cy);
    ctx.rotate((layout.rotate * Math.PI) / 180);
    ctx.drawImage(picture, -w / 2, -h / 2, w, h);
    ctx.restore();
  }
  return canvas;
}

// ── Covered ────────────────────────────────────────────────────────────────

export type Cover = 'blur' | 'pixel' | 'black';

/**
 * Parts of a picture made unreadable, each a box of shares from the top
 * left. Blurred and pixelated by shrinking the part and drawing it back large
 * (a canvas filter would be simpler, but Safari has it only from 18), or
 * blacked out, which is the only one nothing can be read back from.
 */
export function cover(canvas: HTMLCanvasElement, boxes: { x: number; y: number; w: number; h: number }[], style: Cover): void {
  const ctx = canvas.getContext('2d')!;
  for (const b of boxes) {
    const x = Math.max(0, Math.floor(b.x * canvas.width));
    const y = Math.max(0, Math.floor(b.y * canvas.height));
    const w = Math.min(canvas.width - x, Math.ceil(b.w * canvas.width));
    const h = Math.min(canvas.height - y, Math.ceil(b.h * canvas.height));
    if (w < 1 || h < 1) continue;
    if (style === 'black') {
      ctx.fillStyle = '#000';
      ctx.fillRect(x, y, w, h);
      continue;
    }
    // A block about a twelfth (pixel) or a twentieth (blur) of the box's shorter side.
    const block = Math.max(2, Math.round(Math.min(w, h) / (style === 'pixel' ? 12 : 20)));
    const small = blank(Math.max(1, w / block), Math.max(1, h / block));
    const sctx = small.getContext('2d')!;
    sctx.imageSmoothingQuality = 'high';
    sctx.drawImage(canvas, x, y, w, h, 0, 0, small.width, small.height);
    ctx.save();
    ctx.imageSmoothingEnabled = style === 'blur';
    ctx.imageSmoothingQuality = 'high';
    if (style === 'blur') {
      // Twice through a tiny copy: soft, with no blocks to read edges from.
      const smaller = blank(Math.max(1, small.width / 2), Math.max(1, small.height / 2));
      smaller.getContext('2d')!.drawImage(small, 0, 0, smaller.width, smaller.height);
      ctx.drawImage(smaller, x, y, w, h);
      smaller.width = smaller.height = 0;
    } else {
      ctx.drawImage(small, x, y, w, h);
    }
    ctx.restore();
    small.width = small.height = 0;
  }
}

// ── Put together ───────────────────────────────────────────────────────────

/** Pictures put together on a white background (`photo.ts` lays them out). */
export async function collageCanvas(paths: string[], layout: CollageLayout, gap: number, most = 6000): Promise<HTMLCanvasElement> {
  const imgs = await Promise.all(paths.map((p) => loadImage(p)));
  const plan = collage(imgs.map((i) => [i.naturalWidth, i.naturalHeight]), layout, gap, most);
  const canvas = blank(plan.width, plan.height, true);
  const ctx = canvas.getContext('2d')!;
  ctx.imageSmoothingQuality = 'high';
  imgs.forEach((img, i) => {
    const b = plan.boxes[i];
    ctx.drawImage(img, b.x, b.y, b.width, b.height);
  });
  return canvas;
}

// ── ID photos ──────────────────────────────────────────────────────────────

/**
 * An ID photo: the part of the picture in `rect` (its pixels), at exactly
 * `size` pixels, on `backdrop` where the person is not (with a subject mask)
 * or as it was.
 */
export function idPhotoCanvas(
  img: HTMLImageElement,
  rect: Rect,
  size: [number, number],
  backdrop: string | null,
  mask?: { width: number; height: number; mask: Uint8Array },
): HTMLCanvasElement {
  let source: HTMLCanvasElement | HTMLImageElement = img;
  let scale = 1;
  if (backdrop && mask) {
    // The person on the new colour, at the mask's size.
    const cut = blank(mask.width, mask.height);
    const cctx = cut.getContext('2d', { willReadFrequently: true })!;
    cctx.drawImage(img, 0, 0, mask.width, mask.height);
    const px = cctx.getImageData(0, 0, mask.width, mask.height);
    for (let i = 0; i < mask.mask.length; i++) px.data[i * 4 + 3] = mask.mask[i];
    cctx.putImageData(px, 0, 0);
    const on = blank(mask.width, mask.height);
    const octx = on.getContext('2d')!;
    octx.fillStyle = backdrop;
    octx.fillRect(0, 0, on.width, on.height);
    octx.drawImage(cut, 0, 0);
    cut.width = cut.height = 0;
    source = on;
    scale = mask.width / img.naturalWidth;
  }
  const out = blank(size[0], size[1], true);
  const ctx = out.getContext('2d')!;
  ctx.imageSmoothingQuality = 'high';
  ctx.drawImage(source, rect.x * scale, rect.y * scale, rect.width * scale, rect.height * scale, 0, 0, size[0], size[1]);
  if (source instanceof HTMLCanvasElement) source.width = source.height = 0;
  return out;
}

/**
 * Copies of an ID photo on a sheet to print — 10 × 15 cm at 300 dpi, as many
 * as fit — with a thin grey line between them to cut along.
 */
export function sheetCanvas(photo: HTMLCanvasElement, sheet: [number, number] = [1181, 1772], gap = 24): HTMLCanvasElement {
  const [cols, rows] = sheetGrid(sheet, [photo.width, photo.height], gap);
  const canvas = blank(sheet[0], sheet[1], true);
  const ctx = canvas.getContext('2d')!;
  const [w, h] = [cols * photo.width + (cols - 1) * gap, rows * photo.height + (rows - 1) * gap];
  const [x0, y0] = [(sheet[0] - w) / 2, (sheet[1] - h) / 2];
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) ctx.drawImage(photo, x0 + c * (photo.width + gap), y0 + r * (photo.height + gap));
  }
  ctx.strokeStyle = '#c8c8c8';
  ctx.lineWidth = 1;
  ctx.setLineDash([6, 6]);
  for (let c = 0; c <= cols; c++) {
    const x = Math.round(x0 + c * (photo.width + gap) - gap / 2) + 0.5;
    ctx.beginPath();
    ctx.moveTo(x, y0 - gap / 2);
    ctx.lineTo(x, y0 + h + gap / 2);
    ctx.stroke();
  }
  for (let r = 0; r <= rows; r++) {
    const y = Math.round(y0 + r * (photo.height + gap) - gap / 2) + 0.5;
    ctx.beginPath();
    ctx.moveTo(x0 - gap / 2, y);
    ctx.lineTo(x0 + w + gap / 2, y);
    ctx.stroke();
  }
  return canvas;
}
