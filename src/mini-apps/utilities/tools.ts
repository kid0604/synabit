/**
 * The tools Utilities offers, and the small decisions about them that do not
 * need a screen to test: which tools fit which files, what a search matches,
 * what a range of pages means, what a result is called.
 *
 * Design: `docs/utilities-2026-11-11.md` §5. Only tools that are built are
 * listed: the grid never shows a card for something that does not work yet.
 *
 * Names, descriptions and button words live in the locale files under
 * `utilities.tools.<id>`, so this file holds no English a person reads.
 */
import type { Component } from 'vue';
import {
  Crop, Eraser, EyeOff, FileDown, FileImage, FileInput, FileOutput, FileSearch, FileStack, FileType, Images, LayoutGrid, ListOrdered,
  Lock, LockOpen, Minimize2, Repeat, RotateCw, Scaling, ScanText, Scissors, Stamp,
} from 'lucide-vue-next';

export type ToolId =
  | 'merge' | 'split' | 'organize' | 'rotate' | 'compress' | 'watermark' | 'numbers' | 'protect' | 'unlock'
  | 'ocr' | 'office2pdf' | 'pdf2word' | 'img2pdf' | 'pdf2img' | 'pdf2text'
  | 'removebg' | 'imgtext' | 'imgcompress' | 'resize' | 'crop' | 'imgconvert' | 'exif';
export type CategoryId = 'pdf' | 'convert' | 'image';
export type InputKind = 'pdf' | 'image' | 'office';

export interface ToolDef {
  id: ToolId;
  category: CategoryId;
  icon: Component;
  /** The one kind of file it takes. */
  accepts: InputKind;
  /** Fewest files it can run with. */
  min: number;
  /** Most files it takes, when there is a limit. */
  max?: number;
  /**
   * What the middle of the screen shows once files are in: a list of files to
   * put in order, the pages of one file, a preview of the first page with what
   * will be drawn on it, or one picture to cut.
   */
  workspace: 'files' | 'pages' | 'preview' | 'crop' | 'cutout';
  /** What it needs of Apple's Vision, on macOS only (`utilities::vision`). */
  needs?: 'subject' | 'text' | 'libreoffice' | 'pdf2docx';
}

export const CATEGORIES: readonly CategoryId[] = ['pdf', 'convert', 'image'];

export const TOOLS: readonly ToolDef[] = [
  { id: 'merge',    category: 'pdf',     icon: FileStack,  accepts: 'pdf',   min: 2,         workspace: 'files' },
  { id: 'split',    category: 'pdf',     icon: Scissors,   accepts: 'pdf',   min: 1, max: 1, workspace: 'pages' },
  { id: 'organize', category: 'pdf',     icon: LayoutGrid, accepts: 'pdf',   min: 1, max: 1, workspace: 'pages' },
  { id: 'rotate',   category: 'pdf',     icon: RotateCw,   accepts: 'pdf',   min: 1,         workspace: 'files' },
  { id: 'compress', category: 'pdf',     icon: FileDown,   accepts: 'pdf',   min: 1,         workspace: 'files' },
  { id: 'watermark', category: 'pdf',    icon: Stamp,      accepts: 'pdf',   min: 1,         workspace: 'preview' },
  { id: 'numbers',  category: 'pdf',     icon: ListOrdered, accepts: 'pdf',  min: 1,         workspace: 'preview' },
  { id: 'protect',  category: 'pdf',     icon: Lock,       accepts: 'pdf',   min: 1,         workspace: 'files' },
  { id: 'unlock',   category: 'pdf',     icon: LockOpen,   accepts: 'pdf',   min: 1,         workspace: 'files' },
  { id: 'ocr',      category: 'pdf',     icon: FileSearch, accepts: 'pdf',   min: 1,         workspace: 'files', needs: 'text' },
  { id: 'office2pdf', category: 'convert', icon: FileInput, accepts: 'office', min: 1,       workspace: 'files', needs: 'libreoffice' },
  { id: 'pdf2word', category: 'convert', icon: FileOutput, accepts: 'pdf',   min: 1,         workspace: 'files', needs: 'pdf2docx' },
  { id: 'img2pdf',  category: 'convert', icon: FileImage,  accepts: 'image', min: 1,         workspace: 'files' },
  { id: 'pdf2img',  category: 'convert', icon: Images,     accepts: 'pdf',   min: 1,         workspace: 'files' },
  { id: 'pdf2text', category: 'convert', icon: FileType,   accepts: 'pdf',   min: 1,         workspace: 'files' },
  { id: 'removebg', category: 'image',   icon: Eraser,     accepts: 'image', min: 1,         workspace: 'cutout', needs: 'subject' },
  { id: 'imgtext',  category: 'image',   icon: ScanText,   accepts: 'image', min: 1,         workspace: 'files', needs: 'text' },
  { id: 'imgcompress', category: 'image', icon: Minimize2, accepts: 'image', min: 1,         workspace: 'files' },
  { id: 'resize',   category: 'image',   icon: Scaling,    accepts: 'image', min: 1,         workspace: 'files' },
  { id: 'crop',     category: 'image',   icon: Crop,       accepts: 'image', min: 1, max: 1, workspace: 'crop' },
  { id: 'imgconvert', category: 'image', icon: Repeat,     accepts: 'image', min: 1,         workspace: 'files' },
  { id: 'exif',     category: 'image',   icon: EyeOff,     accepts: 'image', min: 1,         workspace: 'files' },
];

export function toolById(id: string | null | undefined): ToolDef | undefined {
  return TOOLS.find((t) => t.id === id);
}

const IMAGE_EXTENSIONS = ['jpg', 'jpeg', 'jfif', 'png', 'heic', 'heif', 'webp', 'gif', 'bmp', 'tif', 'tiff'];
const OFFICE_EXTENSIONS = ['docx', 'doc', 'odt', 'rtf', 'xlsx', 'xls', 'ods', 'csv', 'pptx', 'ppt', 'odp'];

/** What kind of file a name is, or null for one no tool takes. */
export function kindOf(name: string): InputKind | null {
  const ext = name.toLowerCase().split('.').pop() ?? '';
  if (ext === 'pdf') return 'pdf';
  if (IMAGE_EXTENSIONS.includes(ext)) return 'image';
  if (OFFICE_EXTENSIONS.includes(ext)) return 'office';
  return null;
}

/** A JPEG goes into a PDF as it is; anything else is redrawn as one first. */
export function isJpegName(name: string): boolean {
  return /\.(jpe?g|jfif)$/i.test(name);
}

/** Whether a tool can run with this many files. */
export function fits(tool: ToolDef, count: number): boolean {
  return count >= tool.min && (tool.max === undefined || count <= tool.max);
}

/**
 * The tools to offer for files dropped on the home screen, best first: those
 * that take this kind and this many, in the order people most often want
 * them. One PDF does not offer merge; three do not offer split.
 */
const SUGGESTION_ORDER: Record<InputKind, ToolId[]> = {
  pdf: ['merge', 'compress', 'split', 'pdf2word', 'watermark', 'numbers', 'organize', 'rotate', 'ocr', 'protect', 'unlock', 'pdf2img', 'pdf2text'],
  office: ['office2pdf'],
  image: ['removebg', 'img2pdf', 'imgcompress', 'resize', 'exif', 'imgtext', 'imgconvert', 'crop'],
};

export function suggestionsFor(kind: InputKind, count: number): ToolDef[] {
  return SUGGESTION_ORDER[kind].map((id) => toolById(id)!).filter((t) => fits(t, count));
}

/**
 * The kind most of the dropped files are, and which ones are left out of it.
 * A mixed drop is offered tools for the larger group, and says what it set aside.
 */
export function sortDrop<T extends { name: string }>(files: T[]): { kind: InputKind | null; taken: T[]; left: T[] } {
  // Ties go to the earlier kind in this list.
  const groups = (['pdf', 'image', 'office'] as const).map((kind) => ({ kind, files: files.filter((f) => kindOf(f.name) === kind) }));
  const best = groups.reduce((a, b) => (b.files.length > a.files.length ? b : a));
  if (!best.files.length) return { kind: null, taken: [], left: files };
  return { kind: best.kind, taken: best.files, left: files.filter((f) => !best.files.includes(f)) };
}

/**
 * Text folded for search: lower case, no diacritics, đ as d. "nen", "Nén"
 * and "NÉN" are one query, which is how people type Vietnamese in a hurry.
 */
export function fold(text: string): string {
  return text.normalize('NFD').replace(/[̀-ͯ]/g, '').replace(/đ/gi, 'd').toLowerCase();
}

/** Whether a tool matches a search, given its name and description as shown. */
export function matches(query: string, tool: ToolDef, name: string, desc: string): boolean {
  const q = fold(query.trim());
  if (!q) return true;
  return fold(`${name} ${desc} ${tool.id}`).includes(q);
}

/**
 * Page ranges as people write them — "1-3, 5, 8-10" — each one a file.
 * Errors name the part that is wrong so the screen can say which.
 */
export function parseRanges(text: string, pageCount: number): { ranges: [number, number][]; bad: string | null } {
  const ranges: [number, number][] = [];
  for (const raw of text.split(/[,;]/)) {
    const part = raw.trim();
    if (!part) continue;
    const m = part.match(/^(\d+)\s*(?:[-–—]\s*(\d+)?)?$/);
    if (!m) return { ranges, bad: part };
    const from = Number(m[1]);
    // "8-" means to the end.
    const to = m[2] !== undefined ? Number(m[2]) : part.includes('-') || part.includes('–') || part.includes('—') ? pageCount : from;
    if (from < 1 || to < from || to > pageCount) return { ranges, bad: part };
    ranges.push([from, to]);
  }
  return { ranges, bad: null };
}

/** Every `n` pages, as ranges: 10 pages by 4 is 1–4, 5–8, 9–10. */
export function everyN(n: number, pageCount: number): [number, number][] {
  const step = Math.max(1, Math.floor(n));
  const out: [number, number][] = [];
  for (let from = 1; from <= pageCount; from += step) out.push([from, Math.min(pageCount, from + step - 1)]);
  return out;
}

/** The pages from..to, inclusive. */
export function span([from, to]: [number, number]): number[] {
  return Array.from({ length: to - from + 1 }, (_, i) => from + i);
}

/** A file name without its extension. */
export function stem(name: string): string {
  const dot = name.lastIndexOf('.');
  return dot > 0 ? name.slice(0, dot) : name;
}

/**
 * What a result is called: the first input's name, then what was done, in
 * plain ASCII so it travels well by mail and chat — "hop-dong-gop.pdf" style
 * suffixes as in §5.7 of the design.
 */
const SUFFIX: Record<ToolId, string> = {
  merge: 'gop',
  split: 'phan',
  organize: 'sap-xep',
  rotate: 'xoay',
  watermark: 'watermark',
  numbers: 'danh-so',
  protect: 'khoa',
  unlock: 'mo-khoa',
  compress: 'nen',
  office2pdf: '',
  pdf2word: '',
  ocr: 'tim-duoc',
  removebg: 'tach-nen',
  imgtext: '',
  pdf2img: 'trang',
  img2pdf: '',
  pdf2text: '',
  imgcompress: 'nen',
  resize: 'doi-co',
  crop: 'cat',
  imgconvert: '',
  exif: 'sach',
};

/** A file name's extension, lower case, without the dot. */
export function extension(name: string): string {
  const dot = name.lastIndexOf('.');
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : '';
}

/**
 * What a result is called. `ext` is the extension of what is made, for tools
 * whose output is not a PDF; it defaults to the input's own.
 */
export function outputName(tool: ToolId, firstInput: string, part?: number, ext?: string): string {
  const base = stem(firstInput);
  if (tool === 'img2pdf') return `${base}.pdf`;
  if (tool === 'pdf2text' || tool === 'imgtext') return `${base}.txt`;
  if (tool === 'office2pdf') return `${base}.pdf`;
  if (tool === 'pdf2word') return `${base}.docx`;
  if (tool === 'split') return `${base}-${SUFFIX.split}-${part ?? 1}.pdf`;
  if (toolById(tool)?.category === 'image') {
    const out = ext ?? extension(firstInput);
    return SUFFIX[tool] ? `${base}-${SUFFIX[tool]}.${out}` : `${base}.${out}`;
  }
  return `${base}-${SUFFIX[tool]}.pdf`;
}

/** The name of one page drawn as a picture. */
export function pageImageName(firstInput: string, page: number, pages: number, format: 'png' | 'jpeg'): string {
  const width = String(pages).length;
  return `${stem(firstInput)}-${String(page).padStart(width, '0')}.${format === 'png' ? 'png' : 'jpg'}`;
}

/** A size for people: 2,1 MB, 640 KB. */
export function formatSize(bytes: number, locale: string): string {
  const units = ['B', 'KB', 'MB', 'GB'];
  let n = bytes;
  let i = 0;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i++;
  }
  const digits = i === 0 || n >= 100 ? 0 : 1;
  return `${n.toLocaleString(locale, { maximumFractionDigits: digits })} ${units[i]}`;
}

/**
 * The format a picture is written back in: its own when the WebView can
 * write it, JPEG otherwise. HEIC, GIF, BMP and TIFF can be read but not
 * written by a canvas.
 */
export type ImageFormat = 'jpeg' | 'png' | 'webp';

export function ownFormat(name: string, canWebp: boolean): ImageFormat {
  const ext = extension(name);
  if (ext === 'png') return 'png';
  if (ext === 'webp' && canWebp) return 'webp';
  return 'jpeg';
}

export function extensionFor(format: ImageFormat): string {
  return format === 'jpeg' ? 'jpg' : format;
}

/** Whether `exif` can clean a file without redrawing it. */
export function cleansInPlace(name: string): boolean {
  return ['jpg', 'jpeg', 'jfif', 'png'].includes(extension(name));
}

/** Width and height that fit inside a box, never larger than the original. */
export function fitWithin(width: number, height: number, maxW: number, maxH: number, enlarge = false): [number, number] {
  let scale = Math.min(maxW / width, maxH / height);
  if (!enlarge) scale = Math.min(scale, 1);
  return [Math.max(1, Math.round(width * scale)), Math.max(1, Math.round(height * scale))];
}

/** Page numbers as `stamp::Numbering` writes them. */
export const NUMBER_FORMATS = ['{n}', '{n} / {total}', 'Trang {n}', 'Trang {n} / {total}', 'Page {n} of {total}'] as const;

export function numberText(format: string, n: number, total: number): string {
  return format.replace('{n}', String(n)).replace('{total}', String(total));
}

/**
 * Whether a password has characters Preview on the Mac cannot take: it opens
 * an AES-256 PDF only when the password is plain ASCII, though the standard
 * (and pdf.js, and Chrome) allow any.
 */
export function previewMayRefuse(password: string): boolean {
  return /[^\x20-\x7e]/.test(password);
}

/**
 * Where a watermark's centres fall on a page, in points with y up: the same
 * sums as `centres` in `stamp.rs`, so the preview shows what the file gets.
 */
export function markCentres(
  layout: { position: number; tile: boolean },
  page: [number, number],
  box: [number, number],
): [number, number][] {
  const [pw, ph] = page;
  const [bw, bh] = box;
  if (layout.tile) {
    const [sx, sy] = [bw * 1.35 + 12, bh * 1.8 + 12];
    const out: [number, number][] = [];
    let row = 0;
    for (let y = ph - bh / 2; y > -bh / 2; y -= sy, row++) {
      for (let x = row % 2 === 0 ? bw / 2 : bw / 2 + sx / 2; x < pw + bw / 2; x += sx) out.push([x, y]);
    }
    return out;
  }
  const margin = 0.05 * Math.min(pw, ph);
  const col = layout.position % 3;
  const row = Math.floor(Math.min(layout.position, 8) / 3);
  return [[[margin + bw / 2, pw / 2, pw - margin - bw / 2][col], [ph - margin - bh / 2, ph / 2, margin + bh / 2][row]]];
}

/** The box a w × h rectangle takes once turned by `degrees`. */
export function turnedBox(w: number, h: number, degrees: number): [number, number] {
  const r = (degrees * Math.PI) / 180;
  const [s, c] = [Math.abs(Math.sin(r)), Math.abs(Math.cos(r))];
  return [w * c + h * s, w * s + h * c];
}

/** What Vision on this machine can do; `null` off macOS (`utility_vision_status`). */
export interface VisionAbilities {
  subject: boolean;
  text: boolean;
  languages: string[];
}

/** What the room-backed tools need (`utility_office_status`). */
export interface OfficeStatus {
  room: boolean;
  libreoffice: boolean;
  pdf2docx: boolean;
}

/**
 * Whether a tool can run here, and if not, why: not on this operating system,
 * not on this version of it, or waiting for something to be installed.
 */
export type Availability = 'ready' | 'not-mac' | 'old-mac' | 'needs-install';

export function availability(
  tool: ToolDef,
  vision: VisionAbilities | null | undefined,
  office?: OfficeStatus | null,
): Availability {
  if (!tool.needs) return 'ready';
  if (!vision) return 'not-mac';
  if (tool.needs === 'subject' || tool.needs === 'text') return vision[tool.needs] ? 'ready' : 'old-mac';
  if (!office?.room) return 'old-mac';
  return office[tool.needs] ? 'ready' : 'needs-install';
}

/** The languages to ask Vision for: Vietnamese and English where offered. */
export function ocrLanguages(vision: VisionAbilities | null | undefined, choice: 'vi' | 'auto'): string[] {
  if (choice === 'auto' || !vision) return [];
  return ['vi-VT', 'en-US'].filter((l) => vision.languages.includes(l));
}

export interface TextLine {
  text: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Lines (already in reading order) as text, a row to a line: `plain_text` in `searchable.rs`. */
export function linesToText(lines: TextLine[]): string {
  let out = '';
  let previous: TextLine | null = null;
  for (const line of lines) {
    if (previous) {
      const sameRow = Math.abs(previous.y + previous.h / 2 - (line.y + line.h / 2)) < Math.min(previous.h, line.h) / 2;
      out += sameRow ? ' ' : '\n';
    }
    out += line.text;
    previous = line;
  }
  return out;
}
