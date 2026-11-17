/**
 * Things put on a PDF's pages by hand: what Sign and fill places (a signature,
 * a line of text, a date, a tick or a cross) and the boxes Redact blacks out.
 *
 * Every box is kept as shares of the page as it is shown — left and top from
 * the top left corner, width and height, all 0–1 — so the screen can draw it
 * at any size, and Rust (`stamp::place`, `redact`) can find it on the page
 * whatever the page's own turn and crop box.
 *
 * What Sign and fill places is drawn here as a picture, in the system font,
 * and put on the page as one: the standard PDF fonts have no Vietnamese
 * (`stamp.rs`, design §4.2), and a picture of a signature is what a signature
 * on paper is anyway.
 */
import getStroke from 'perfect-freehand';
import { MARK_FONT, type MarkPicture } from './render';

/** A box on a shown page: shares of it, from the top left. */
export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

export type MarkKind = 'signature' | 'text' | 'date' | 'check' | 'cross';

export interface Mark extends Box {
  id: string;
  /** 1-based. */
  page: number;
  kind: MarkKind;
  /** For text and dates. */
  text: string;
  color: string;
  /** For signatures: the picture, as a PNG data URL with a clear background. */
  picture?: string;
  /**
   * The page's width over its height, as shown: what turns a box's shares into
   * a shape, so a signature keeps its proportions and a word its width.
   */
  aspect: number;
}

/** A box Redact blacks out. */
export interface Hidden extends Box {
  id: string;
  page: number;
}

/** A signature ready to place: its picture and its width over its height. */
export interface Ink {
  url: string;
  ratio: number;
}

let serial = 0;
export function newId(prefix: string): string {
  return `${prefix}${Date.now().toString(36)}${(++serial).toString(36)}`;
}

export const INKS = ['#111827', '#1d3f8f'] as const;

/** Inside the page, at least a sliver wide. */
export function clampBox<T extends Box>(b: T): T {
  const w = Math.min(1, Math.max(0.005, b.w));
  const h = Math.min(1, Math.max(0.005, b.h));
  return { ...b, w, h, x: Math.min(1 - w, Math.max(0, b.x)), y: Math.min(1 - h, Math.max(0, b.y)) };
}

/** A box as Rust takes it: left, top, right, bottom. */
export function edges(b: Box): [number, number, number, number] {
  return [b.x, b.y, b.x + b.w, b.y + b.h];
}

/** How wide a line of text is next to how tall its letters are, in this font. */
export function textRatio(text: string): number {
  const ctx = document.createElement('canvas').getContext('2d')!;
  ctx.font = `400 100px ${MARK_FONT}`;
  return Math.max(0.5, ctx.measureText(text || ' ').width / 100);
}

/**
 * The width a text mark needs for its words at its height, as a share of the
 * page: the letters fill 78% of the box's height.
 */
export function textWidth(text: string, h: number, aspect: number): number {
  return Math.min(1, (textRatio(text) * 0.78 * h) / aspect + h * 0.15 / aspect);
}

/** A new mark of a kind, in the middle of the page, at a sensible size. */
export function newMark(kind: MarkKind, page: number, aspect: number, options: { text?: string; color: string; ink?: Ink }): Mark {
  const base = { id: newId('m'), page, kind, text: options.text ?? '', color: options.color, aspect };
  let w: number;
  let h: number;
  if (kind === 'signature' && options.ink) {
    w = 0.3;
    h = (w * aspect) / options.ink.ratio;
  } else if (kind === 'check' || kind === 'cross') {
    w = 0.035;
    h = w * aspect;
  } else {
    // 12-point letters on an A4 page: a box about 15 points tall.
    h = 0.018;
    w = textWidth(base.text, h, aspect);
  }
  return clampBox({ ...base, picture: options.ink?.url, w, h, x: 0.5 - w / 2, y: 0.5 - h / 2 });
}

/** Today, written the way this language writes a date. */
export function today(locale: string): string {
  return new Intl.DateTimeFormat(locale.startsWith('vi') ? 'vi-VN' : locale, { day: '2-digit', month: '2-digit', year: 'numeric' }).format(new Date());
}

function pictureOf(canvas: HTMLCanvasElement): MarkPicture {
  const ctx = canvas.getContext('2d')!;
  const data = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
  return { width: canvas.width, height: canvas.height, rgba: new Uint8Array(data.buffer.slice(0)), url: '' };
}

function loaded(url: string): Promise<HTMLImageElement> {
  const img = new Image();
  img.src = url;
  return img.decode().then(() => img);
}

/**
 * A mark drawn as it goes on the page, with a clear background: about three
 * pixels to the point, so it prints sharp, and no more than 2400 across.
 * `points` is the page's shown size.
 */
export async function markPicture(mark: Mark, points: { width: number; height: number }): Promise<MarkPicture> {
  const scale = Math.min(3, 2400 / Math.max(mark.w * points.width, mark.h * points.height, 1));
  const canvas = document.createElement('canvas');
  canvas.width = Math.max(2, Math.round(mark.w * points.width * scale));
  canvas.height = Math.max(2, Math.round(mark.h * points.height * scale));
  const ctx = canvas.getContext('2d', { willReadFrequently: true })!;
  const [w, h] = [canvas.width, canvas.height];
  ctx.fillStyle = ctx.strokeStyle = mark.color;
  if (mark.kind === 'signature' && mark.picture) {
    ctx.drawImage(await loaded(mark.picture), 0, 0, w, h);
  } else if (mark.kind === 'check' || mark.kind === 'cross') {
    ctx.lineWidth = Math.max(1.5, Math.min(w, h) * 0.13);
    ctx.lineCap = ctx.lineJoin = 'round';
    ctx.beginPath();
    if (mark.kind === 'check') {
      ctx.moveTo(w * 0.15, h * 0.55);
      ctx.lineTo(w * 0.42, h * 0.8);
      ctx.lineTo(w * 0.87, h * 0.2);
    } else {
      ctx.moveTo(w * 0.2, h * 0.2);
      ctx.lineTo(w * 0.8, h * 0.8);
      ctx.moveTo(w * 0.8, h * 0.2);
      ctx.lineTo(w * 0.2, h * 0.8);
    }
    ctx.stroke();
  } else {
    const px = h * 0.78;
    ctx.font = `400 ${px}px ${MARK_FONT}`;
    ctx.textBaseline = 'middle';
    ctx.fillText(mark.text, h * 0.06, h / 2, w - h * 0.12);
  }
  const out = pictureOf(canvas);
  canvas.width = canvas.height = 0;
  return out;
}

// ── Signatures ─────────────────────────────────────────────────────────────

/** The outline of one stroke from the pad, as an SVG path. */
export function strokePath(points: number[][], size: number): string {
  const outline = getStroke(points, { size, thinning: 0.6, smoothing: 0.55, streamline: 0.5, simulatePressure: points.every((p) => p[2] === undefined || p[2] === 0.5) });
  if (outline.length < 2) return '';
  let d = `M ${outline[0][0]},${outline[0][1]}`;
  for (let i = 1; i < outline.length; i++) {
    const [cx, cy] = outline[i - 1];
    const [x, y] = outline[i];
    d += ` Q ${cx},${cy} ${(cx + x) / 2},${(cy + y) / 2}`;
  }
  return `${d} Z`;
}

/** A canvas cut down to where something is drawn, with a little room; `null` if nothing is. */
export function trimmed(canvas: HTMLCanvasElement, pad = 4): HTMLCanvasElement | null {
  const ctx = canvas.getContext('2d', { willReadFrequently: true })!;
  const { width, height } = canvas;
  const data = ctx.getImageData(0, 0, width, height).data;
  let [left, top, right, bottom] = [width, height, -1, -1];
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (data[(y * width + x) * 4 + 3] > 8) {
        if (x < left) left = x;
        if (x > right) right = x;
        if (y < top) top = y;
        if (y > bottom) bottom = y;
      }
    }
  }
  if (right < 0) return null;
  [left, top] = [Math.max(0, left - pad), Math.max(0, top - pad)];
  [right, bottom] = [Math.min(width - 1, right + pad), Math.min(height - 1, bottom + pad)];
  const out = document.createElement('canvas');
  out.width = right - left + 1;
  out.height = bottom - top + 1;
  out.getContext('2d')!.drawImage(canvas, left, top, out.width, out.height, 0, 0, out.width, out.height);
  return out;
}

/** Strokes drawn on the pad as a signature to place. */
export function inkFromStrokes(strokes: number[][][], width: number, height: number, color: string, size: number): Ink | null {
  const scale = 3;
  const canvas = document.createElement('canvas');
  canvas.width = width * scale;
  canvas.height = height * scale;
  const ctx = canvas.getContext('2d')!;
  ctx.scale(scale, scale);
  ctx.fillStyle = color;
  for (const stroke of strokes) {
    const d = strokePath(stroke, size);
    if (d) ctx.fill(new Path2D(d));
  }
  const cut = trimmed(canvas, 6);
  canvas.width = canvas.height = 0;
  if (!cut) return null;
  return { url: cut.toDataURL('image/png'), ratio: cut.width / cut.height };
}

/**
 * A photo or scan of a signature on paper as one to place: the paper made
 * clear, the ink kept, darker ink more solid, then cut to the ink.
 */
export async function inkFromPicture(img: HTMLImageElement, color: string | null): Promise<Ink | null> {
  const scale = Math.min(1, 1600 / Math.max(img.naturalWidth, img.naturalHeight));
  const canvas = document.createElement('canvas');
  canvas.width = Math.max(1, Math.round(img.naturalWidth * scale));
  canvas.height = Math.max(1, Math.round(img.naturalHeight * scale));
  const ctx = canvas.getContext('2d', { willReadFrequently: true })!;
  ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
  const data = ctx.getImageData(0, 0, canvas.width, canvas.height);
  const tint = color ? [parseInt(color.slice(1, 3), 16), parseInt(color.slice(3, 5), 16), parseInt(color.slice(5, 7), 16)] : null;
  for (let i = 0; i < data.data.length; i += 4) {
    const [r, g, b, a] = [data.data[i], data.data[i + 1], data.data[i + 2], data.data[i + 3]];
    const light = 0.299 * r + 0.587 * g + 0.114 * b;
    // Paper (light) to clear; ink (dark) to solid, with soft edges between.
    const ink = Math.max(0, Math.min(1, (200 - light) / 120));
    data.data[i + 3] = Math.round(a * ink);
    if (tint) [data.data[i], data.data[i + 1], data.data[i + 2]] = tint;
  }
  ctx.putImageData(data, 0, 0);
  const cut = trimmed(canvas, 4);
  canvas.width = canvas.height = 0;
  if (!cut) return null;
  return { url: cut.toDataURL('image/png'), ratio: cut.width / cut.height };
}

// ── Form values, flattened ─────────────────────────────────────────────────

/**
 * A form box's value drawn as it will stay on the page once the form is
 * flattened: text in the system font (a line fitted to the box, or wrapped
 * for a box of several lines), a tick for a checkbox, a dot for a radio
 * button.
 */
export function valuePicture(
  kind: 'text' | 'check' | 'dot',
  text: string,
  box: Box,
  points: { width: number; height: number },
  multiline = false,
): MarkPicture {
  const [pw, ph] = [box.w * points.width, box.h * points.height];
  const scale = Math.min(3, 2400 / Math.max(pw, ph, 1));
  const canvas = document.createElement('canvas');
  canvas.width = Math.max(2, Math.round(pw * scale));
  canvas.height = Math.max(2, Math.round(ph * scale));
  const ctx = canvas.getContext('2d', { willReadFrequently: true })!;
  const [w, h] = [canvas.width, canvas.height];
  ctx.fillStyle = ctx.strokeStyle = '#111827';
  if (kind === 'check') {
    ctx.lineWidth = Math.max(1.5, Math.min(w, h) * 0.13);
    ctx.lineCap = ctx.lineJoin = 'round';
    ctx.beginPath();
    ctx.moveTo(w * 0.2, h * 0.55);
    ctx.lineTo(w * 0.42, h * 0.78);
    ctx.lineTo(w * 0.82, h * 0.22);
    ctx.stroke();
  } else if (kind === 'dot') {
    ctx.beginPath();
    ctx.arc(w / 2, h / 2, Math.min(w, h) * 0.28, 0, Math.PI * 2);
    ctx.fill();
  } else {
    // 11 points at most, smaller for a short box: what forms are filled at.
    const px = Math.min(11 * scale, (multiline ? 0.9 : 0.7) * Math.min(h, 11 * 1.4 * scale));
    ctx.font = `400 ${px}px ${MARK_FONT}`;
    ctx.textBaseline = 'alphabetic';
    const pad = Math.min(2 * scale, w * 0.05);
    if (multiline) {
      const lines: string[] = [];
      for (const para of text.split('\n')) {
        let line = '';
        for (const word of para.split(/\s+/)) {
          const next = line ? `${line} ${word}` : word;
          if (ctx.measureText(next).width > w - 2 * pad && line) {
            lines.push(line);
            line = word;
          } else {
            line = next;
          }
        }
        lines.push(line);
      }
      const lh = px * 1.2;
      lines.forEach((line, i) => ctx.fillText(line, pad, pad + px + i * lh, w - 2 * pad));
    } else {
      ctx.fillText(text, pad, h / 2 + px * 0.35, w - 2 * pad);
    }
  }
  const out = pictureOf(canvas);
  canvas.width = canvas.height = 0;
  return out;
}
