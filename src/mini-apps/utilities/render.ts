/**
 * Drawing in the WebView: pages of a PDF as pictures, and pictures as JPEGs.
 *
 * pdf.js draws page thumbnails and does PDF → images; a canvas turns a HEIC,
 * PNG or WebP into a JPEG for images → PDF. Both use what the WebView already
 * has, so Rust needs no renderer and no image decoder (see
 * `commands::thumbnails` for why it has none).
 */
import { convertFileSrc } from '@tauri-apps/api/core';
import { pageText, pdfjsLib, type PDFDocumentProxy } from '../../shared/pdfjs';

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
          cMapUrl: '/pdfjs/cmaps/',
          cMapPacked: true,
          standardFontDataUrl: '/pdfjs/standard_fonts/',
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
  await p.render({ canvas, viewport }).promise;
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
  await p.render({ canvas, viewport }).promise;
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
  const img = new Image();
  img.decoding = 'async';
  img.src = fileUrl(path);
  await img.decode();
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

/** A picture from the workspace, decoded and upright. */
export async function loadImage(path: string): Promise<HTMLImageElement> {
  const img = new Image();
  img.decoding = 'async';
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
    const page = await doc.getPage(n);
    const content = await pageText(page);
    let text = '';
    for (const item of content.items) {
      if (!('str' in item)) continue;
      text += item.str + (item.hasEOL ? '\n' : '');
    }
    parts.push(`—— ${pageLabel} ${n} ——\n\n${text.trim() || noText}`);
  }
  return parts.join('\n\n') + '\n';
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

const MARK_FONT = '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif';

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
  await p.render({ canvas, canvasContext: ctx, viewport }).promise;
  const data = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
  const out = { width: canvas.width, height: canvas.height, rgba: new Uint8Array(data.buffer.slice(0)) };
  canvas.width = canvas.height = 0;
  return out;
}

/** Whether a page already has text of its own, so it needs no reading. */
export async function pageHasText(path: string, page: number): Promise<boolean> {
  const doc = await openPdf(path);
  const content = await pageText(await doc.getPage(page));
  return content.items.some((i) => 'str' in i && i.str.trim().length > 0);
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
