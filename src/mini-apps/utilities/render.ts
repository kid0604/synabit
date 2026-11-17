/**
 * Drawing in the WebView: pages of a PDF as pictures, and pictures as JPEGs.
 *
 * pdf.js draws page thumbnails and does PDF → images; a canvas turns a HEIC,
 * PNG or WebP into a JPEG for images → PDF. Both use what the WebView already
 * has, so Rust needs no renderer and no image decoder (see
 * `commands::thumbnails` for why it has none).
 */
import { convertFileSrc } from '@tauri-apps/api/core';
import { DOCUMENT_OPTIONS, pageText, pdfjsLib, type PDFDocumentProxy, type PDFPageProxy } from '../../shared/pdfjs';
import { foldChar } from './tools';
import { asScan, flatSize, warp, type Quad } from './straighten';

/** A workspace file, as a URL the WebView can fetch. */
export function fileUrl(path: string): string {
  return convertFileSrc(path);
}

const documents = new Map<string, Promise<PDFDocumentProxy>>();

/**
 * A PDF opened once per path and kept: the file cards, the page grid and the
 * export all read the same document rather than parsing it three times.
 */
export function openPdf(path: string): Promise<PDFDocumentProxy> {
  let doc = documents.get(path);
  if (!doc) {
    doc = fetch(fileUrl(path))
      .then((r) => {
        if (!r.ok) throw new Error(`could not read the file (${r.status})`);
        return r.arrayBuffer();
      })
      .then(async (data) => {
        const task = pdfjsLib.getDocument({
          data: new Uint8Array(data),
          ...DOCUMENT_OPTIONS,
        });
        try {
          return await task.promise;
        } catch (e) {
          // A file that would not open (a password, a broken file) still holds
          // a worker-side document until its task is torn down.
          void task.destroy().catch(() => {});
          throw e;
        }
      });
    // A failed open is not kept, so adding the file again tries again.
    const opening = doc;
    opening.catch(() => {
      if (documents.get(path) === opening) documents.delete(path);
    });
    documents.set(path, opening);
  }
  return doc;
}

/** Let go of a PDF nothing on screen needs any more. */
export async function closePdf(path: string): Promise<void> {
  const doc = documents.get(path);
  documents.delete(path);
  // pdfjs-dist 6 tears a document down through its loading task.
  if (doc) await doc.then((d) => d.loadingTask.destroy()).catch(() => {});
}

/**
 * Draw one page into a canvas, `width` CSS pixels wide, sharp on this screen.
 * `rotate` is added to the page's own rotation, so a turned page shows turned.
 */
export async function drawPage(path: string, page: number, canvas: HTMLCanvasElement, width: number, rotate = 0): Promise<void> {
  const doc = await openPdf(path);
  const p = await doc.getPage(page);
  const rotation = (((p.rotate + rotate) % 360) + 360) % 360;
  const base = p.getViewport({ scale: 1, rotation });
  const ratio = window.devicePixelRatio || 1;
  const viewport = p.getViewport({ scale: (width / base.width) * ratio, rotation });
  canvas.width = Math.round(viewport.width);
  canvas.height = Math.round(viewport.height);
  canvas.style.width = `${width}px`;
  canvas.style.height = `${Math.round(viewport.height / ratio)}px`;
  try {
    await p.render({ canvas, viewport }).promise;
  } finally {
    letGo(p);
  }
}

/**
 * Let go of what drawing a page decoded. pdf.js keeps a page's pictures and
 * drawing commands until it is told to — on its own it does so only after
 * printing — so a run over a 300-page scan held every page's decoded images
 * at once and the WebView ran out of memory. A page drawn again is decoded
 * again. While another draw of the same page is still going this does
 * nothing, so it is safe after any draw.
 */
function letGo(page: PDFPageProxy) {
  page.cleanup();
}

function toBlob(canvas: HTMLCanvasElement, type: string, quality?: number): Promise<Blob> {
  return new Promise((resolve, reject) =>
    canvas.toBlob((b) => (b ? resolve(b) : reject(new Error('the picture could not be encoded'))), type, quality),
  );
}

/** One page as a picture at `dpi`, encoded as PNG or JPEG. */
export async function pageAsImage(path: string, page: number, dpi: number, format: 'png' | 'jpeg'): Promise<Uint8Array> {
  const doc = await openPdf(path);
  const p = await doc.getPage(page);
  const viewport = p.getViewport({ scale: dpi / 72 });
  const canvas = document.createElement('canvas');
  canvas.width = Math.round(viewport.width);
  canvas.height = Math.round(viewport.height);
  const ctx = canvas.getContext('2d');
  if (!ctx) throw new Error('no canvas');
  if (format === 'jpeg') {
    // JPEG has no transparency; a page is white paper.
    ctx.fillStyle = '#fff';
    ctx.fillRect(0, 0, canvas.width, canvas.height);
  }
  try {
    await p.render({ canvas, viewport }).promise;
  } finally {
    letGo(p);
  }
  const blob = await toBlob(canvas, format === 'png' ? 'image/png' : 'image/jpeg', 0.92);
  canvas.width = canvas.height = 0;
  return new Uint8Array(await blob.arrayBuffer());
}

/**
 * A picture the WebView can show, redrawn as a JPEG on white. The browser
 * applies EXIF orientation as it decodes (`image-orientation: from-image`),
 * so the result is upright and carries no orientation of its own.
 */
export async function asJpeg(path: string): Promise<Uint8Array> {
  const img = await loadImage(path);
  const canvas = document.createElement('canvas');
  canvas.width = img.naturalWidth;
  canvas.height = img.naturalHeight;
  const ctx = canvas.getContext('2d');
  if (!ctx) throw new Error('no canvas');
  ctx.fillStyle = '#fff';
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.drawImage(img, 0, 0);
  const blob = await toBlob(canvas, 'image/jpeg', 0.92);
  canvas.width = canvas.height = 0;
  return new Uint8Array(await blob.arrayBuffer());
}

// ── Pictures ───────────────────────────────────────────────────────────────

const encodable = new Map<string, boolean>();

/**
 * Whether this WebView can write a format. Safari reads WebP but writes it
 * only from Safari 26 on (older ones quietly hand back a PNG), so WebP is
 * offered only where it works — and where it does not, the option is simply
 * absent, which is the old behaviour.
 */
export function canEncode(type: 'image/webp' | 'image/jpeg' | 'image/png'): boolean {
  let known = encodable.get(type);
  if (known === undefined) {
    const c = document.createElement('canvas');
    c.width = c.height = 1;
    known = c.toDataURL(type).startsWith(`data:${type}`);
    encodable.set(type, known);
  }
  return known;
}

/**
 * A picture from the workspace, decoded and upright, that a canvas may read.
 *
 * The `asset:` URL is another origin from the page, and a picture loaded from
 * another origin without CORS taints any canvas it is drawn on: `toBlob` and
 * `getImageData` then throw "The operation is insecure" (thumbnails hit this
 * first; see `shared/thumbnails.ts`). Tauri's asset protocol answers with the
 * window's origin in `Access-Control-Allow-Origin`, so asking in CORS mode is
 * all it takes.
 */
export async function loadImage(path: string): Promise<HTMLImageElement> {
  const img = new Image();
  img.decoding = 'async';
  img.crossOrigin = 'anonymous';
  img.src = fileUrl(path);
  await img.decode();
  return img;
}

export interface Region {
  x: number;
  y: number;
  width: number;
  height: number;
}

/**
 * Draw (part of) a picture at a size and encode it. JPEG gets a white ground,
 * because a transparent pixel has no JPEG colour and would come out black.
 */
export async function encodeImage(
  img: HTMLImageElement,
  width: number,
  height: number,
  format: 'jpeg' | 'png' | 'webp',
  quality: number,
  from?: Region,
): Promise<Uint8Array> {
  const canvas = document.createElement('canvas');
  canvas.width = Math.max(1, Math.round(width));
  canvas.height = Math.max(1, Math.round(height));
  const ctx = canvas.getContext('2d');
  if (!ctx) throw new Error('no canvas');
  if (format === 'jpeg') {
    ctx.fillStyle = '#fff';
    ctx.fillRect(0, 0, canvas.width, canvas.height);
  }
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = 'high';
  const r = from ?? { x: 0, y: 0, width: img.naturalWidth, height: img.naturalHeight };
  ctx.drawImage(img, r.x, r.y, r.width, r.height, 0, 0, canvas.width, canvas.height);
  const blob = await toBlob(canvas, `image/${format}`, quality);
  canvas.width = canvas.height = 0;
  return new Uint8Array(await blob.arrayBuffer());
}

// ── Text of a PDF ──────────────────────────────────────────────────────────

/**
 * Every page's text, each under a line naming its page. pdf.js marks where a
 * line ends, so the lines come out as they were set rather than as one run.
 * A page with no text — a scan — says so, so the page numbers after it stay
 * right.
 */
export async function pdfText(path: string, pageLabel: string, noText: string, onPage?: () => boolean): Promise<string> {
  const doc = await openPdf(path);
  const parts: string[] = [];
  for (let n = 1; n <= doc.numPages; n++) {
    if (onPage && !onPage()) throw new Error('cancelled');
    parts.push(`—— ${pageLabel} ${n} ——\n\n${(await pageWords(path, n)) || noText}`);
  }
  return parts.join('\n\n') + '\n';
}

/**
 * The text a page carries of its own, line by line as it was set (pdf.js
 * marks where a line ends), trimmed: empty for a scan.
 */
export async function pageWords(path: string, page: number): Promise<string> {
  const doc = await openPdf(path);
  const p = await doc.getPage(page);
  const content = await pageText(p);
  letGo(p);
  let text = '';
  for (const item of content.items) {
    if (!('str' in item)) continue;
    text += item.str + (item.hasEOL ? '\n' : '');
  }
  return text.trim();
}

/** A page's size as it is shown (its own rotation applied), in points. */
export async function shownPageSize(path: string, page = 1): Promise<{ width: number; height: number }> {
  const doc = await openPdf(path);
  const p = await doc.getPage(page);
  const v = p.getViewport({ scale: 1 });
  return { width: v.width, height: v.height };
}

// ── Watermarks ─────────────────────────────────────────────────────────────

export interface MarkPicture {
  width: number;
  height: number;
  rgba: Uint8Array;
  /** For the preview: the same pixels as an image URL. */
  url: string;
}

export const MARK_FONT = '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif';

function pictureOf(canvas: HTMLCanvasElement): MarkPicture {
  const ctx = canvas.getContext('2d')!;
  const data = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
  return { width: canvas.width, height: canvas.height, rgba: new Uint8Array(data.buffer.slice(0)), url: canvas.toDataURL('image/png') };
}

/**
 * Text drawn as the watermark picture. Large enough to stay sharp on a printed
 * page, since `stamp.rs` scales it to the page and not the other way round.
 */
export function textMark(text: string, color: string, opacity: number): MarkPicture | null {
  const line = text.trim();
  if (!line) return null;
  const px = 160;
  const canvas = document.createElement('canvas');
  const ctx = canvas.getContext('2d')!;
  ctx.font = `700 ${px}px ${MARK_FONT}`;
  const width = Math.min(4000, Math.ceil(ctx.measureText(line).width + px * 0.4));
  canvas.width = width;
  canvas.height = Math.ceil(px * 1.35);
  ctx.font = `700 ${px}px ${MARK_FONT}`;
  ctx.textBaseline = 'middle';
  ctx.textAlign = 'center';
  ctx.globalAlpha = opacity;
  ctx.fillStyle = color;
  ctx.fillText(line, width / 2, canvas.height / 2, width - px * 0.2);
  return pictureOf(canvas);
}

/** A logo drawn as the watermark picture, at most 1600 pixels across. */
export function imageMark(img: HTMLImageElement, opacity: number): MarkPicture {
  const scale = Math.min(1, 1600 / Math.max(img.naturalWidth, img.naturalHeight));
  const canvas = document.createElement('canvas');
  canvas.width = Math.max(1, Math.round(img.naturalWidth * scale));
  canvas.height = Math.max(1, Math.round(img.naturalHeight * scale));
  const ctx = canvas.getContext('2d')!;
  ctx.globalAlpha = opacity;
  ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
  return pictureOf(canvas);
}

// ── Pixels for Vision ──────────────────────────────────────────────────────

export interface Pixels {
  width: number;
  height: number;
  rgba: Uint8Array;
}

/**
 * A picture's pixels, upright, at most `maxSide` across: Vision reads a 4000px
 * photo no better than a 2000px one, and the bytes cross to Rust uncompressed.
 */
export function pixelsOf(img: HTMLImageElement, maxSide = 2048): Pixels {
  const scale = Math.min(1, maxSide / Math.max(img.naturalWidth, img.naturalHeight));
  const canvas = document.createElement('canvas');
  canvas.width = Math.max(1, Math.round(img.naturalWidth * scale));
  canvas.height = Math.max(1, Math.round(img.naturalHeight * scale));
  const ctx = canvas.getContext('2d', { willReadFrequently: true })!;
  ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
  const data = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
  const out = { width: canvas.width, height: canvas.height, rgba: new Uint8Array(data.buffer.slice(0)) };
  canvas.width = canvas.height = 0;
  return out;
}

/** One PDF page as pixels, as it is shown, at `dpi` but at most `maxSide` across. */
export async function pagePixels(path: string, page: number, dpi = 200, maxSide = 3000): Promise<Pixels> {
  const doc = await openPdf(path);
  const p = await doc.getPage(page);
  const base = p.getViewport({ scale: 1 });
  const scale = Math.min(dpi / 72, maxSide / Math.max(base.width, base.height));
  const viewport = p.getViewport({ scale });
  const canvas = document.createElement('canvas');
  canvas.width = Math.round(viewport.width);
  canvas.height = Math.round(viewport.height);
  const ctx = canvas.getContext('2d', { willReadFrequently: true })!;
  ctx.fillStyle = '#fff';
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  try {
    await p.render({ canvas, canvasContext: ctx, viewport }).promise;
  } finally {
    letGo(p);
  }
  const data = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
  const out = { width: canvas.width, height: canvas.height, rgba: new Uint8Array(data.buffer.slice(0)) };
  canvas.width = canvas.height = 0;
  return out;
}

/**
 * A page drawn as a picture to show, its longer side `longSide` pixels: what a
 * box is drawn over (cropping, signing, redacting). The URL is the caller's to
 * revoke. `points` is the page's shown size, for turning pixels into points.
 */
export async function pagePicture(
  path: string,
  page: number,
  longSide = 1200,
): Promise<{ url: string; width: number; height: number; points: { width: number; height: number } }> {
  const doc = await openPdf(path);
  const p = await doc.getPage(page);
  const base = p.getViewport({ scale: 1 });
  const viewport = p.getViewport({ scale: longSide / Math.max(base.width, base.height) });
  const canvas = document.createElement('canvas');
  canvas.width = Math.round(viewport.width);
  canvas.height = Math.round(viewport.height);
  const ctx = canvas.getContext('2d')!;
  ctx.fillStyle = '#fff';
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  try {
    await p.render({ canvas, canvasContext: ctx, viewport }).promise;
  } finally {
    letGo(p);
  }
  const blob = await toBlob(canvas, 'image/png');
  const out = { url: URL.createObjectURL(blob), width: canvas.width, height: canvas.height, points: { width: base.width, height: base.height } };
  canvas.width = canvas.height = 0;
  return out;
}

/**
 * Where a page's content ends, as a share of the page shown (left, top,
 * right, bottom, 0–1), with `padding` points around it; `null` for a blank
 * page. Anything not near white counts as content, so a scan's grey edge does
 * too — which is the edge a person would cut to.
 */
export async function contentBox(path: string, page: number, padding: number): Promise<[number, number, number, number] | null> {
  const doc = await openPdf(path);
  const p = await doc.getPage(page);
  const base = p.getViewport({ scale: 1 });
  const scale = Math.min(2, 1000 / Math.max(base.width, base.height));
  const viewport = p.getViewport({ scale });
  const canvas = document.createElement('canvas');
  const [w, h] = [Math.max(1, Math.round(viewport.width)), Math.max(1, Math.round(viewport.height))];
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext('2d', { willReadFrequently: true })!;
  ctx.fillStyle = '#fff';
  ctx.fillRect(0, 0, w, h);
  try {
    await p.render({ canvas, canvasContext: ctx, viewport }).promise;
  } finally {
    letGo(p);
  }
  const data = ctx.getImageData(0, 0, w, h).data;
  canvas.width = canvas.height = 0;
  const box = inkBounds(data, w, h);
  if (!box) return null;
  const pad = padding * scale;
  return [
    Math.max(0, box[0] - pad) / w,
    Math.max(0, box[1] - pad) / h,
    Math.min(w, box[2] + 1 + pad) / w,
    Math.min(h, box[3] + 1 + pad) / h,
  ];
}

/** The pixels that are not near white, as left, top, right, bottom; `null` if none. */
export function inkBounds(rgba: Uint8ClampedArray | Uint8Array, width: number, height: number, near = 235): [number, number, number, number] | null {
  let [left, top, right, bottom] = [width, height, -1, -1];
  for (let y = 0; y < height; y++) {
    const row = y * width * 4;
    for (let x = 0; x < width; x++) {
      const i = row + x * 4;
      if (rgba[i] < near || rgba[i + 1] < near || rgba[i + 2] < near) {
        if (x < left) left = x;
        if (x > right) right = x;
        if (y < top) top = y;
        if (y > bottom) bottom = y;
      }
    }
  }
  return right < 0 ? null : [left, top, right, bottom];
}

/** One box of a PDF form, as Fill form shows it (from pdf.js's `getFieldObjects`). */
export interface FormField {
  /** The widget, as pdf.js names it: `"12R"`. Rust finds it by this. */
  id: string;
  /** The field's full name; widgets of one name share a value. */
  name: string;
  type: 'text' | 'checkbox' | 'radiobutton' | 'combobox' | 'listbox';
  /** 1-based. */
  page: number;
  /** Where it is on the page shown, as shares from the top left. */
  box: { x: number; y: number; w: number; h: number };
  /** What it holds now: text, a list's value, or a button's state name ("Off"). */
  value: string;
  /** A checkbox's or radio button's "on" state name. */
  exportValue?: string;
  multiline?: boolean;
  maxLength?: number;
  choices?: { value: string; label: string }[];
  readOnly: boolean;
}

const FIELD_TYPES = new Set(['text', 'checkbox', 'radiobutton', 'combobox', 'listbox']);

/** Every box of a PDF's form that a person can fill in, page by page. */
export async function formFields(path: string): Promise<FormField[]> {
  const doc = await openPdf(path);
  const all = (await doc.getFieldObjects()) as Record<string, Record<string, unknown>[]> | null;
  if (!all) return [];
  const viewports = new Map<number, { width: number; height: number; toView: (r: number[]) => number[] }>();
  const out: FormField[] = [];
  for (const [name, widgets] of Object.entries(all)) {
    for (const w of widgets) {
      const type = w.type as FormField['type'];
      if (!FIELD_TYPES.has(type) || w.hidden || !Array.isArray(w.rect) || typeof w.page !== 'number' || typeof w.id !== 'string') continue;
      const page = (w.page as number) + 1;
      if (page < 1 || page > doc.numPages) continue;
      let view = viewports.get(page);
      if (!view) {
        const p = await doc.getPage(page);
        const v = p.getViewport({ scale: 1 });
        // Two corners through the page's turn and crop: pdf.js 6 has no rectangle call.
        view = {
          width: v.width,
          height: v.height,
          toView: (r) => [...v.convertToViewportPoint(r[0], r[1]), ...v.convertToViewportPoint(r[2], r[3])],
        };
        viewports.set(page, view);
        letGo(p);
      }
      const [x1, y1, x2, y2] = view.toView(w.rect as number[]);
      const items = Array.isArray(w.items) ? (w.items as { exportValue: string; displayValue: string }[]) : undefined;
      out.push({
        id: w.id as string,
        name,
        type,
        page,
        box: {
          x: Math.min(x1, x2) / view.width,
          y: Math.min(y1, y2) / view.height,
          w: Math.abs(x2 - x1) / view.width,
          h: Math.abs(y2 - y1) / view.height,
        },
        value: typeof w.value === 'string' ? w.value : '',
        exportValue: typeof w.exportValues === 'string' ? w.exportValues : undefined,
        multiline: w.multiline === true,
        maxLength: typeof w.charLimit === 'number' && w.charLimit > 0 ? w.charLimit : undefined,
        choices: items?.map((i) => ({ value: String(i.exportValue), label: String(i.displayValue) })),
        readOnly: w.editable === false,
      });
    }
  }
  return out.sort((a, b) => a.page - b.page || a.box.y - b.box.y || a.box.x - b.box.x);
}

/** Where on the shown page a match is, as shares from the top left. */
export interface Found {
  page: number;
  x: number;
  y: number;
  w: number;
  h: number;
}

/**
 * Every match of `find` on every page, as boxes on the page shown.
 *
 * The page's runs of text are read as one string, a space where a line ends,
 * because a PDF often splits a line into runs of a letter or two ("H", "ợ",
 * "p" — Word does): a name looked for run by run would never be found. Each
 * character remembers its run, so a match is boxed by the part of each run it
 * covers, cut by its share of the run's characters, and joined into one box
 * per line. `find` gets the page's folded text and answers with the start and
 * end of each match, in characters.
 */
export async function findOnPages(path: string, find: (folded: string) => [number, number][], onPage?: () => boolean): Promise<Found[]> {
  const doc = await openPdf(path);
  const out: Found[] = [];
  for (let n = 1; n <= doc.numPages; n++) {
    if (onPage && !onPage()) break;
    const p = await doc.getPage(n);
    const view = p.getViewport({ scale: 1 });
    const content = await pageText(p);
    letGo(p);
    const runs = content.items.filter((i) => 'str' in i) as unknown as Run[];
    // Per character of the page's text: its run, and where it is in it (-1 for a line break).
    const owner: [number, number][] = [];
    let folded = '';
    runs.forEach((run, r) => {
      const chars = Array.from(run.str);
      chars.forEach((c, k) => {
        folded += foldChar(c);
        owner.push([r, k]);
      });
      if (run.hasEOL) {
        folded += ' ';
        owner.push([r, -1]);
      }
    });
    for (const [start, end] of find(folded)) {
      // The part of each run the match covers.
      const parts = new Map<number, [number, number]>();
      for (let i = start; i < end; i++) {
        const [r, k] = owner[i] ?? [-1, -1];
        if (r < 0 || k < 0) continue;
        const was = parts.get(r);
        parts.set(r, was ? [Math.min(was[0], k), Math.max(was[1], k + 1)] : [k, k + 1]);
      }
      // One box per line: runs whose baselines are close are one line.
      const lines: { y: number; h: number; box: [number, number, number, number] }[] = [];
      for (const [r, [from, to]] of parts) {
        const box = runBox(runs[r], from, to, view);
        const baseline = runs[r].transform[5] as number;
        const h = runs[r].height || 1;
        const line = lines.find((l) => Math.abs(l.y - baseline) < Math.max(l.h, h) * 0.5);
        if (line) {
          line.box = [Math.min(line.box[0], box[0]), Math.min(line.box[1], box[1]), Math.max(line.box[2], box[2]), Math.max(line.box[3], box[3])];
        } else {
          lines.push({ y: baseline, h, box });
        }
      }
      for (const { box: [x0, y0, x1, y1] } of lines) {
        out.push({ page: n, x: x0 / view.width, y: y0 / view.height, w: (x1 - x0) / view.width, h: (y1 - y0) / view.height });
      }
    }
  }
  return out;
}

/**
 * The box of characters `from`–`to` of a run, on the page shown, in pixels at
 * scale 1: from a little under the baseline to the top of its letters, with a
 * point and a half of room.
 */
/** A run of text as pdf.js reads it: what is written, and where. */
interface Run {
  str: string;
  transform: number[];
  width: number;
  height: number;
  hasEOL: boolean;
}

function runBox(
  run: Run,
  from: number,
  to: number,
  view: { width: number; height: number; convertToViewportPoint: (x: number, y: number) => number[] },
): [number, number, number, number] {
  const chars = Math.max(1, Array.from(run.str).length);
  const [a, b, , , e, f] = run.transform;
  const length = Math.hypot(a, b) || 1;
  const [dx, dy] = [a / length, b / length];
  const [ux, uy] = [-dy, dx];
  const h = run.height || length;
  const [s, t] = [(from / chars) * run.width, (to / chars) * run.width];
  const corners = [
    [e + dx * s - ux * h * 0.25, f + dy * s - uy * h * 0.25],
    [e + dx * t - ux * h * 0.25, f + dy * t - uy * h * 0.25],
    [e + dx * s + ux * h * 0.95, f + dy * s + uy * h * 0.95],
    [e + dx * t + ux * h * 0.95, f + dy * t + uy * h * 0.95],
  ].map(([x, y]) => view.convertToViewportPoint(x, y));
  const xs = corners.map((c) => c[0]);
  const ys = corners.map((c) => c[1]);
  const pad = 1.5;
  return [
    Math.max(0, Math.min(...xs) - pad),
    Math.max(0, Math.min(...ys) - pad),
    Math.min(view.width, Math.max(...xs) + pad),
    Math.min(view.height, Math.max(...ys) + pad),
  ];
}

/**
 * A page drawn with its boxes painted solid black, as Redact puts it in the
 * page's place: a JPEG for the file, and the same pixels for reading the words
 * that are left. At 200 dots an inch, at most 3000 pixels across.
 */
export async function blackedPage(
  path: string,
  page: number,
  boxes: { x: number; y: number; w: number; h: number }[],
): Promise<{ jpeg: Uint8Array; pixels: Pixels }> {
  const doc = await openPdf(path);
  const p = await doc.getPage(page);
  const base = p.getViewport({ scale: 1 });
  const viewport = p.getViewport({ scale: Math.min(200 / 72, 3000 / Math.max(base.width, base.height)) });
  const canvas = document.createElement('canvas');
  canvas.width = Math.round(viewport.width);
  canvas.height = Math.round(viewport.height);
  const ctx = canvas.getContext('2d', { willReadFrequently: true })!;
  ctx.fillStyle = '#fff';
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  try {
    await p.render({ canvas, canvasContext: ctx, viewport }).promise;
  } finally {
    letGo(p);
  }
  ctx.fillStyle = '#000';
  for (const b of boxes) {
    // Whole pixels, rounded outwards: no sliver of a letter at an edge.
    const x0 = Math.floor(b.x * canvas.width);
    const y0 = Math.floor(b.y * canvas.height);
    ctx.fillRect(x0, y0, Math.ceil((b.x + b.w) * canvas.width) - x0, Math.ceil((b.y + b.h) * canvas.height) - y0);
  }
  const data = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
  const pixels = { width: canvas.width, height: canvas.height, rgba: new Uint8Array(data.buffer.slice(0)) };
  const jpeg = new Uint8Array(await (await toBlob(canvas, 'image/jpeg', 0.9)).arrayBuffer());
  canvas.width = canvas.height = 0;
  return { jpeg, pixels };
}

/**
 * A photo of a page made flat and upright from its four corners, as a JPEG
 * for a PDF; `scan` also makes the paper white and the ink dark.
 */
export async function straightenedJpeg(path: string, quad: Quad, scan: boolean): Promise<Uint8Array> {
  const img = await loadImage(path);
  // The photo at up to 4000 pixels across: enough for a 3000-pixel page out of it.
  const s = Math.min(1, 4000 / Math.max(img.naturalWidth, img.naturalHeight));
  const [sw, sh] = [Math.max(1, Math.round(img.naturalWidth * s)), Math.max(1, Math.round(img.naturalHeight * s))];
  const source = document.createElement('canvas');
  source.width = sw;
  source.height = sh;
  const sctx = source.getContext('2d', { willReadFrequently: true })!;
  sctx.drawImage(img, 0, 0, sw, sh);
  const pixels = sctx.getImageData(0, 0, sw, sh).data;
  source.width = source.height = 0;
  const [ow, oh] = flatSize(quad, sw, sh);
  const flat = warp(pixels, sw, sh, quad, ow, oh);
  if (scan) asScan(flat);
  const out = document.createElement('canvas');
  out.width = ow;
  out.height = oh;
  out.getContext('2d')!.putImageData(new ImageData(flat, ow, oh), 0, 0);
  const jpeg = new Uint8Array(await (await toBlob(out, 'image/jpeg', 0.9)).arrayBuffer());
  out.width = out.height = 0;
  return jpeg;
}

export type Backdrop = 'transparent' | string;

/**
 * A picture cut out along a mask: the mask (at any size) stretched over the
 * full picture as its alpha, then laid on `backdrop`, and optionally trimmed to
 * the subject. Returns the canvas, for a preview or for encoding.
 */
export function cutOut(img: HTMLImageElement, mask: { width: number; height: number; mask: Uint8Array }, backdrop: Backdrop, trim: boolean): HTMLCanvasElement {
  const w = img.naturalWidth;
  const h = img.naturalHeight;
  // The mask as an image whose alpha is the mask.
  const m = document.createElement('canvas');
  m.width = mask.width;
  m.height = mask.height;
  const mctx = m.getContext('2d')!;
  const md = mctx.createImageData(mask.width, mask.height);
  for (let i = 0; i < mask.mask.length; i++) md.data[i * 4 + 3] = mask.mask[i];
  mctx.putImageData(md, 0, 0);

  const cut = document.createElement('canvas');
  cut.width = w;
  cut.height = h;
  const ctx = cut.getContext('2d')!;
  ctx.drawImage(img, 0, 0);
  ctx.globalCompositeOperation = 'destination-in';
  ctx.imageSmoothingQuality = 'high';
  ctx.drawImage(m, 0, 0, w, h);
  ctx.globalCompositeOperation = 'source-over';

  let box = { x: 0, y: 0, w, h };
  if (trim) {
    // The subject's bounds, from the mask, scaled up to the picture.
    let [x0, y0, x1, y1] = [mask.width, mask.height, -1, -1];
    for (let y = 0; y < mask.height; y++) {
      for (let x = 0; x < mask.width; x++) {
        if (mask.mask[y * mask.width + x] > 24) {
          if (x < x0) x0 = x;
          if (x > x1) x1 = x;
          if (y < y0) y0 = y;
          if (y > y1) y1 = y;
        }
      }
    }
    if (x1 >= x0) {
      const sx = w / mask.width;
      const sy = h / mask.height;
      box = { x: Math.floor(x0 * sx), y: Math.floor(y0 * sy), w: Math.ceil((x1 - x0 + 1) * sx), h: Math.ceil((y1 - y0 + 1) * sy) };
    }
  }
  const out = document.createElement('canvas');
  out.width = box.w;
  out.height = box.h;
  const octx = out.getContext('2d')!;
  if (backdrop !== 'transparent') {
    octx.fillStyle = backdrop;
    octx.fillRect(0, 0, box.w, box.h);
  }
  octx.drawImage(cut, box.x, box.y, box.w, box.h, 0, 0, box.w, box.h);
  m.width = m.height = cut.width = cut.height = 0;
  return out;
}

export async function encodeCanvas(canvas: HTMLCanvasElement, format: 'png' | 'jpeg', quality = 0.92): Promise<Uint8Array> {
  const blob = await toBlob(canvas, `image/${format}`, quality);
  return new Uint8Array(await blob.arrayBuffer());
}
