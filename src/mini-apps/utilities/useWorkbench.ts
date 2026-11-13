/**
 * What the Utilities screen is doing: which tool is open, which files are in
 * it, what the options say, and where a run has got to.
 *
 * One tool is open at a time and it is one screen with four states (design
 * §5.3): `empty` (nothing to work on yet), `loaded`, `running`, `done`. Going
 * back from `done` to `loaded` keeps the files and the options, so changing
 * one thing and running again does not mean starting over.
 *
 * Module-level state, so the sidebar, the grid and the tool screen read the
 * same thing without passing it down three levels.
 */
import { computed, reactive, ref, shallowRef } from 'vue';
import * as api from './api';
import { errorText } from '../../shared/errorText';
import { showAppNotice } from '../../composables/useAppNotice';
import {
  asJpeg, canEncode, closePdf, cutOut, encodeCanvas, encodeImage, imageMark, loadImage, openPdf, pageAsImage, pageWords, pagePixels,
  pdfText, pixelsOf, textMark, type Region,
} from './render';
import {
  availability, cleansInPlace, everyN, extensionFor, fitWithin, fits, isJpegName, kindOf, linesToText, ocrLanguages, outputName,
  ownFormat, pageImageName, parseRanges, span, stem, toolById, type ImageFormat, type InputKind, type OfficeStatus, type TextLine,
  type ToolDef, type ToolId, type VisionAbilities,
} from './tools';
import { i18n } from '../../i18n';

export interface Item {
  input: api.Input;
  kind: InputKind | null;
  /** Pages, once pdf.js has opened it. */
  pages?: number;
  /** A picture's size in pixels, upright, once decoded. */
  width?: number;
  height?: number;
  /** Whether a PDF asks for a password; known for the unlock tool. */
  lock?: api.Lock;
  /** Why it cannot be used, when it cannot. */
  problem?: 'unreadable' | 'locked' | 'wrong-kind';
}

export interface PageSlot {
  /** Page number in the source, 1-based. */
  page: number;
  /** Degrees clockwise to add. */
  rotate: number;
  /** Tells two copies of one page apart in a list. */
  key: string;
}

export type Phase = 'empty' | 'loaded' | 'running' | 'done';

const tool = ref<ToolId | null>(null);
const items = ref<Item[]>([]);
const phase = ref<Phase>('empty');
const progress = reactive({ done: 0, total: 0 });
const job = shallowRef<api.Job | null>(null);
/**
 * Bytes in, for a tool that makes things smaller: only the files that made a
 * result, so one left out as already small is not counted as saved.
 */
const before = ref<number | undefined>(undefined);
/** A failure to show above the work, as a locale key and its values. */
const failure = shallowRef<{ key: string; values?: Record<string, unknown> } | null>(null);
/**
 * The run in progress, if any. A run belongs to the screen it was started
 * from: leaving that screen — another tool, the home grid, a result from the
 * sidebar — cancels it. (Letting it carry on off screen meant it read the
 * next tool's options and wrote its progress bar.)
 */
interface Run {
  job: string | null;
  cancelled: boolean;
}
let active: Run | null = null;

/** Stop the run in progress and forget it; its work is thrown away. */
function stopActive() {
  if (!active) return;
  active.cancelled = true;
  if (active.job) void api.cancel(active.job).catch(() => {});
  active = null;
}

const options = reactive({
  split: {
    mode: 'ranges' as 'ranges' | 'every' | 'pick',
    ranges: '',
    every: 1,
    picked: new Set<number>(),
    zip: true,
  },
  organize: { pages: [] as PageSlot[] },
  rotate: { angle: 90 as 90 | 180 | 270 },
  pdf2img: { format: 'png' as 'png' | 'jpeg', dpi: 150 },
  img2pdf: { size: 'a4' as api.Layout['size'], orientation: 'auto' as api.Layout['orientation'], margin: 18 },
  watermark: {
    mode: 'text' as 'text' | 'image',
    text: '',
    color: '#d93838',
    opacity: 0.25,
    size: 0.6,
    rotate: -35,
    position: 4,
    tile: false,
    under: false,
    exceptFirst: false,
    logo: null as api.Input | null,
  },
  numbers: { position: 4, format: '{n} / {total}', start: 1, skipFirst: false, size: 10 },
  protect: { password: '', confirm: '' },
  unlock: { password: '' },
  imgcompress: { quality: 0.75, format: 'keep' as 'keep' | ImageFormat },
  resize: { mode: 'percent' as 'percent' | 'fit', percent: 50, width: 1600, height: 1600, enlarge: false },
  crop: { aspect: 'free' as string, rect: null as Region | null },
  imgconvert: { format: 'jpeg' as ImageFormat },
  removebg: { backdrop: 'transparent' as string, trim: false },
  ocr: { pdf: true, text: false, skipText: true, language: 'vi' as 'vi' | 'auto' },
  imgtext: { language: 'vi' as 'vi' | 'auto' },
  compress: { level: 1 as 0 | 1 | 2 },
});

/**
 * The three compression levels: how many dots an inch a picture keeps at the
 * page's size, and the JPEG quality it is written at.
 */
export const COMPRESS_LEVELS = [
  { dpi: 200, quality: 0.85 },
  { dpi: 150, quality: 0.7 },
  { dpi: 100, quality: 0.5 },
] as const;

/** What the room-backed tools have here; asked again after an install. */
const office = ref<OfficeStatus | null | undefined>(undefined);
function askOffice(): Promise<void> {
  return api.officeStatus().then((o) => { office.value = o ?? null; }).catch(() => { office.value = null; });
}

/**
 * Installing pdf2docx: minutes, once, through the room that reaches only the
 * package registry. Kept here rather than on the tool's screen, which is made
 * anew each time the tool opens: leaving and coming back showed "Install"
 * again while the first install went on, and a second could start beside it.
 */
const installing = ref(false);
const installError = ref('');
async function installPdf2docx(): Promise<void> {
  if (installing.value) return;
  installing.value = true;
  installError.value = '';
  try {
    await api.installPdf2docx();
    await askOffice();
  } catch (e) {
    // Rust says `install-failed: <why>`; the sentence around it says the first part.
    installError.value = errorText(e).replace(/^install-failed:\s*/, '');
  } finally {
    installing.value = false;
  }
}

/** What Vision can do here: `undefined` until asked, `null` off macOS. */
const vision = ref<VisionAbilities | null | undefined>(undefined);
let visionAsked: Promise<void> | null = null;
function askVision(): Promise<void> {
  visionAsked ??= api.visionStatus().then((v) => { vision.value = v ?? null; }).catch(() => { vision.value = null; });
  return visionAsked;
}

/** Subject masks already found, by input id, so the preview's work is not done twice. */
export interface Mask { width: number; height: number; mask: Uint8Array }
const masks = new Map<string, Promise<Mask>>();

/** The subject of a picture, found once. */
function maskFor(item: Item): Promise<Mask> {
  let found = masks.get(item.input.id);
  if (!found) {
    found = loadImage(item.input.path).then(async (img) => {
      const px = pixelsOf(img);
      return { width: px.width, height: px.height, mask: await api.subjectMask(px.width, px.height, px.rgba) };
    });
    found.catch(() => masks.delete(item.input.id));
    masks.set(item.input.id, found);
  }
  return found;
}

const unsaved = ref<api.Job[]>([]);

const current = computed<ToolDef | undefined>(() => toolById(tool.value));
const usable = computed(() => items.value.filter((i) => !i.problem));
const firstPdf = computed(() => usable.value.find((i) => i.kind === 'pdf'));

/** The files each split part will hold, or why there are none. */
const splitPlan = computed<{ parts: number[][]; bad: string | null }>(() => {
  const pages = firstPdf.value?.pages ?? 0;
  if (!pages) return { parts: [], bad: null };
  const s = options.split;
  if (s.mode === 'pick') {
    const picked = [...s.picked].filter((p) => p <= pages).sort((a, b) => a - b);
    return { parts: picked.length ? [picked] : [], bad: null };
  }
  if (s.mode === 'every') {
    if (!positive(s.every)) return { parts: [], bad: null };
    return { parts: everyN(s.every, pages).map(span), bad: null };
  }
  const { ranges, bad } = parseRanges(s.ranges, pages);
  return { parts: ranges.map(span), bad };
});

/** A number typed into a field, and above nought. (An emptied field gives ''.) */
const positive = (v: unknown) => typeof v === 'number' && Number.isFinite(v) && v > 0;

/** Whether the run button can be pressed. */
const ready = computed(() => {
  const t = current.value;
  if (!t || phase.value !== 'loaded') return false;
  if (availability(t, vision.value, office.value) !== 'ready') return false;
  if (!fits(t, usable.value.length)) return false;
  if (t.id !== 'unlock' && usable.value.some((i) => i.kind === 'pdf' && !i.pages)) return false;
  if (usable.value.some((i) => i.kind === 'image' && !i.width)) return false;
  if (t.id === 'split') return splitPlan.value.parts.length > 0 && !splitPlan.value.bad;
  if (t.id === 'organize') return options.organize.pages.length > 0;
  if (t.id === 'watermark') {
    const w = options.watermark;
    return w.mode === 'text' ? !!w.text.trim() : !!w.logo;
  }
  if (t.id === 'ocr') return options.ocr.pdf || options.ocr.text;
  if (t.id === 'protect') return !!options.protect.password && options.protect.password === options.protect.confirm;
  if (t.id === 'crop') return !!options.crop.rect && options.crop.rect.width >= 1 && options.crop.rect.height >= 1;
  if (t.id === 'resize') {
    const r = options.resize;
    return r.mode === 'percent' ? positive(r.percent) : positive(r.width) && positive(r.height);
  }
  if (t.id === 'numbers') {
    const n = options.numbers;
    return Number.isInteger(n.start) && n.start >= 0 && positive(n.size) && n.size >= 6 && n.size <= 36;
  }
  return true;
});

/** Whether the WebView can write WebP, which decides whether it is offered. */
const webp = computed(() => canEncode('image/webp'));

let slotSerial = 0;
function slots(pages: number): PageSlot[] {
  return Array.from({ length: pages }, (_, i) => ({ page: i + 1, rotate: 0, key: `p${++slotSerial}` }));
}

/**
 * Read what can be known about a file before running: a PDF's pages (from
 * pdf.js), a picture's size. For the unlock tool a password-protected PDF is
 * the point rather than a problem, so it is asked about instead of refused.
 */
async function inspect(item: Item) {
  if (item.kind === 'image') {
    try {
      const img = await loadImage(item.input.path);
      item.width = img.naturalWidth;
      item.height = img.naturalHeight;
    } catch {
      item.problem = 'unreadable';
    }
  } else if (item.kind === 'pdf') {
    if (tool.value === 'unlock') {
      try {
        [item.lock] = await api.lockStatus([item.input.id]);
      } catch {
        item.problem = 'unreadable';
      }
    }
    if (!item.problem) {
      try {
        const doc = await openPdf(item.input.path);
        item.pages = doc.numPages;
      } catch (e) {
        const locked = (e as { name?: string })?.name === 'PasswordException';
        if (!(locked && tool.value === 'unlock')) item.problem = locked ? 'locked' : 'unreadable';
      }
    }
  }
  // Removed, or the tool changed, while it was being read: nothing to update,
  // and the next tool's page options are not this file's to reset.
  if (!items.value.some((i) => i.input.id === item.input.id)) return;
  items.value = [...items.value];
  resetPageOptions();
}

/** The crop box for a picture and an aspect: the largest that fits, centred. */
function defaultCrop(width: number, height: number, aspect: string): Region {
  const [a, b] = aspect.split(':').map(Number);
  if (!a || !b) return { x: 0, y: 0, width, height };
  const ratio = a / b;
  const w = Math.min(width, height * ratio);
  const h = w / ratio;
  return { x: (width - w) / 2, y: (height - h) / 2, width: w, height: h };
}

function resetCrop() {
  const img = usable.value.find((i) => i.kind === 'image');
  options.crop.rect = img?.width ? defaultCrop(img.width, img.height!, options.crop.aspect) : null;
}

function resetPageOptions() {
  if (tool.value === 'crop') resetCrop();
  const pages = firstPdf.value?.pages ?? 0;
  if (tool.value === 'organize') options.organize.pages = slots(pages);
  if (tool.value === 'split') {
    options.split.picked = new Set();
    // Split in two by default, so there is something to see at once.
    const half = Math.ceil(pages / 2);
    options.split.ranges = pages > 1 ? `1-${half}, ${half + 1}-${pages}` : '1';
  }
}

/** What belongs to one tool's screen and not the next. */
function forgetScreen() {
  stopActive();
  for (const i of items.value) if (i.kind === 'pdf') void closePdf(i.input.path);
  masks.clear();
  options.protect.password = options.protect.confirm = '';
  options.unlock.password = '';
  before.value = undefined;
}

function openTool(id: ToolId, inputs: api.Input[] = []) {
  forgetScreen();
  tool.value = id;
  items.value = [];
  job.value = null;
  failure.value = null;
  phase.value = 'empty';
  if (inputs.length) addInputs(inputs);
}

function closeTool() {
  forgetScreen();
  tool.value = null;
  items.value = [];
  job.value = null;
  failure.value = null;
  phase.value = 'empty';
}

/**
 * Add files to the open tool. Files of a kind the tool does not take are
 * kept out, and the count of them comes back so the screen can say so.
 */
function addInputs(inputs: api.Input[]): number {
  const t = current.value;
  if (!t) return inputs.length;
  let room = t.max === undefined ? Infinity : t.max - usable.value.length;
  // A one-file tool given a new file swaps it in — only a file it takes: one
  // of the wrong kind is refused and the file already open stays.
  if (t.max === 1 && inputs.some((i) => kindOf(i.name) === t.accepts)) {
    for (const i of items.value) if (i.kind === 'pdf') void closePdf(i.input.path);
    items.value = [];
    room = 1;
  }
  let refused = 0;
  const added: Item[] = [];
  for (const input of inputs) {
    const kind = kindOf(input.name);
    if (kind !== t.accepts || room <= 0) {
      refused++;
      continue;
    }
    room--;
    added.push({ input, kind });
  }
  if (added.length) {
    items.value = [...items.value, ...added];
    phase.value = 'loaded';
    failure.value = null;
    for (const item of added) void inspect(items.value.find((i) => i.input.id === item.input.id)!);
  }
  return refused;
}

function removeItem(index: number) {
  const [gone] = items.value.splice(index, 1);
  if (gone?.kind === 'pdf') void closePdf(gone.input.path);
  items.value = [...items.value];
  if (!items.value.length) phase.value = 'empty';
  resetPageOptions();
}

function moveItem(from: number, to: number) {
  if (from === to) return;
  const list = [...items.value];
  const [moved] = list.splice(from, 1);
  list.splice(to, 0, moved);
  items.value = list;
}

function sortByName() {
  items.value = [...items.value].sort((a, b) => a.input.name.localeCompare(b.input.name, undefined, { numeric: true }));
}

function clearItems() {
  for (const i of items.value) if (i.kind === 'pdf') void closePdf(i.input.path);
  items.value = [];
  phase.value = 'empty';
}

/** What the Rust error says, as a locale key the screen can show. */
function explain(e: unknown): { key: string; values?: Record<string, unknown> } | null {
  const text = errorText(e);
  if (/cancelled/.test(text)) return null;
  if (/no-subject/.test(text)) return { key: 'utilities.errors.no_subject' };
  if (/not-smaller/.test(text)) return { key: 'utilities.errors.not_smaller' };
  if (/missing-(libreoffice|pdf2docx)/.test(text)) return { key: 'utilities.errors.missing_program' };
  if (/no-room/.test(text)) return { key: 'utilities.errors.no_room' };
  if (/install-failed/.test(text)) return { key: 'utilities.errors.install_failed', values: { reason: text } };
  const named = text.match(/(locked|unreadable): “(.+?)”/);
  if (named) {
    // For the unlock tool, "locked" can only mean the password given was not it.
    const key = named[1] === 'locked' && tool.value === 'unlock' ? 'wrong_password' : named[1];
    return { key: `utilities.errors.${key}`, values: { name: named[2] } };
  }
  return { key: 'utilities.errors.other', values: { reason: text } };
}

/**
 * One picture from a PDF, smaller: scaled so its longer side is at most
 * `most` pixels and written as a JPEG at `quality`. The WebView decodes it —
 * a JPEG as a file, raw pixels as they are — so Rust needs no codec.
 */
async function shrinkPicture(bytes: Uint8Array, p: api.PdfPicture, most: number, quality: number): Promise<Uint8Array | null> {
  let source: CanvasImageSource;
  let url: string | null = null;
  if (p.stored === 'jpeg') {
    url = URL.createObjectURL(new Blob([bytes as BlobPart], { type: 'image/jpeg' }));
    const img = new Image();
    img.src = url;
    try {
      await img.decode();
    } catch {
      URL.revokeObjectURL(url);
      return null;
    }
    source = img;
  } else {
    const raw = document.createElement('canvas');
    raw.width = p.width;
    raw.height = p.height;
    const ctx = raw.getContext('2d')!;
    const data = ctx.createImageData(p.width, p.height);
    const per = p.stored === 'rgb' ? 3 : 1;
    for (let i = 0, j = 0; i < p.width * p.height; i++, j += per) {
      data.data[i * 4] = bytes[j];
      data.data[i * 4 + 1] = bytes[j + (per === 3 ? 1 : 0)];
      data.data[i * 4 + 2] = bytes[j + (per === 3 ? 2 : 0)];
      data.data[i * 4 + 3] = 255;
    }
    ctx.putImageData(data, 0, 0);
    source = raw;
  }
  const [w, h] = fitWithin(p.width, p.height, most, most);
  const canvas = document.createElement('canvas');
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext('2d')!;
  ctx.fillStyle = '#fff';
  ctx.fillRect(0, 0, w, h);
  ctx.imageSmoothingQuality = 'high';
  ctx.drawImage(source, 0, 0, w, h);
  if (url) URL.revokeObjectURL(url);
  const out = await encodeCanvas(canvas, 'jpeg', quality);
  canvas.width = canvas.height = 0;
  if (source instanceof HTMLCanvasElement) source.width = source.height = 0;
  return out;
}

/** Each file and the name its result gets. */
function perFile(t: ToolId, files: Item[]): api.PerFile[] {
  return files.map((f) => ({ input: f.input.id, name: outputName(t, f.input.name) }));
}

/** Redraw a picture and add it to the run. */
async function putPicture(job: string, f: Item, w: number, h: number, format: ImageFormat, quality: number, tool: ToolId, region?: Region) {
  const img = await loadImage(f.input.path);
  const bytes = await encodeImage(img, w, h, format, quality, region);
  await api.putOutput(job, outputName(tool, f.input.name, undefined, extensionFor(format)), bytes);
}

async function runPdfJob(id: string, inputs: string[], outputs: api.PdfOutput[]) {
  return api.assemble(id, inputs, outputs);
}

async function run() {
  const t = current.value;
  if (!t || !ready.value) return;
  const files = usable.value;
  const first = files[0].input.name;
  phase.value = 'running';
  failure.value = null;
  progress.done = 0;
  progress.total = 0;
  stopActive();
  const run: Run = { job: null, cancelled: false };
  active = run;
  /** Whether the screen still shows this run: not cancelled, not since left. */
  const shown = () => active === run;
  /** Stop here if the run was cancelled: checked before each step that costs. */
  const check = () => {
    if (run.cancelled) throw new Error('cancelled');
  };
  /** Progress, written only while this run is the one on screen. */
  const tick = () => {
    if (shown()) progress.done++;
  };
  const expect = (n: number) => {
    if (shown()) progress.total += n;
  };
  let started: api.Job | null = null;
  let sizeIn: number | undefined = files.reduce((n, f) => n + f.input.size, 0);
  try {
    started = await api.newJob(t.id);
    run.job = started.id;
    check();
    const ids = files.map((f) => f.input.id);
    let result: api.Job;
    switch (t.id) {
      case 'merge':
        result = await runPdfJob(started.id, ids, [
          { name: outputName('merge', first), picks: files.map((_, source) => ({ source, pages: null, rotate: 0 })) },
        ]);
        break;
      case 'split': {
        const parts = splitPlan.value.parts;
        const name = (k: number) =>
          options.split.mode === 'pick' ? `${stem(first)}-trich.pdf` : outputName('split', first, k);
        result = await runPdfJob(
          started.id,
          [ids[0]],
          parts.map((pages, k) => ({ name: name(k + 1), picks: [{ source: 0, pages, rotate: 0 }] })),
        );
        break;
      }
      case 'organize':
        result = await runPdfJob(started.id, [ids[0]], [
          {
            name: outputName('organize', first),
            picks: options.organize.pages.map((s) => ({ source: 0, pages: [s.page], rotate: s.rotate })),
          },
        ]);
        break;
      case 'rotate':
        result = await runPdfJob(
          started.id,
          ids,
          files.map((f, source) => ({ name: outputName('rotate', f.input.name), picks: [{ source, pages: null, rotate: options.rotate.angle }] })),
        );
        break;
      case 'img2pdf': {
        // A JPEG goes in as it is; anything else is redrawn as one here.
        const jpegs: string[] = [];
        expect(files.length);
        for (const f of files) {
          check();
          if (isJpegName(f.input.name)) jpegs.push(f.input.id);
          else jpegs.push((await api.stage(`${stem(f.input.name)}.jpg`, await asJpeg(f.input.path))).id);
          tick();
        }
        const { size, orientation, margin } = options.img2pdf;
        result = await api.imagesToPdf(started.id, jpegs, { size, orientation, margin }, outputName('img2pdf', first));
        break;
      }
      case 'pdf2img': {
        const { format, dpi } = options.pdf2img;
        expect(files.reduce((n, f) => n + (f.pages ?? 0), 0));
        for (const f of files) {
          const pages = f.pages ?? 0;
          for (let p = 1; p <= pages; p++) {
            check();
            const bytes = await pageAsImage(f.input.path, p, dpi, format);
            await api.putOutput(started.id, pageImageName(f.input.name, p, pages, format), bytes);
            tick();
          }
        }
        result = await api.getJob(started.id);
        break;
      }
      case 'watermark': {
        const w = options.watermark;
        const picture =
          w.mode === 'text' ? textMark(w.text, w.color, w.opacity) : imageMark(await loadImage(w.logo!.path), w.opacity);
        if (!picture) throw new Error('no watermark');
        result = await api.watermark(
          started.id,
          perFile(t.id, files),
          picture,
          { position: w.position, tile: w.tile, size: w.size, rotate: w.rotate, under: w.under },
          w.exceptFirst,
        );
        break;
      }
      case 'numbers': {
        const n = options.numbers;
        result = await api.numberPages(started.id, perFile(t.id, files), {
          position: n.position, format: n.format, start: n.start, skipFirst: n.skipFirst, size: n.size,
        });
        break;
      }
      case 'protect':
        result = await api.protect(started.id, perFile(t.id, files), options.protect.password);
        break;
      case 'unlock':
        result = await api.unlock(started.id, perFile(t.id, files), options.unlock.password);
        break;
      case 'pdf2text': {
        const tr = i18n.global.t;
        expect(files.length);
        for (const f of files) {
          const text = await pdfText(f.input.path, tr('utilities.options.text_page'), tr('utilities.options.text_none'), () => !run.cancelled);
          check();
          await api.putOutput(started.id, outputName('pdf2text', f.input.name), new TextEncoder().encode(text));
          tick();
        }
        result = await api.getJob(started.id);
        break;
      }
      case 'compress': {
        const { dpi, quality } = COMPRESS_LEVELS[options.compress.level];
        for (const f of files) {
          const { pictures, pageLongSide } = await api.pdfPictures(f.input.id);
          const most = Math.round((pageLongSide / 72) * dpi);
          expect(pictures.length);
          for (const p of pictures) {
            check();
            const smaller = await shrinkPicture(await api.pdfPicture(f.input.id, p.id), p, most, quality);
            if (smaller && smaller.length < p.bytes * 0.9) await api.compressPut(started.id, f.input.id, p.id, smaller);
            tick();
          }
        }
        result = await api.compressPdf(started.id, perFile(t.id, files));
        const kept = files.length - result.outputs.length;
        const made = new Set(result.outputs.map((o) => o.name));
        const shrunk = files.filter((f) => made.has(outputName('compress', f.input.name)));
        // Matched by name; two inputs of one name make that guesswork, so say nothing.
        sizeIn = shrunk.length === result.outputs.length ? shrunk.reduce((n, f) => n + f.input.size, 0) : undefined;
        if (kept > 0) showAppNotice(i18n.global.t('utilities.errors.some_not_smaller', { n: kept }, kept));
        break;
      }
      case 'office2pdf':
        result = await api.officeToPdf(started.id, perFile(t.id, files));
        break;
      case 'pdf2word':
        result = await api.pdfToWord(started.id, perFile(t.id, files));
        break;
      case 'removebg': {
        const { backdrop, trim } = options.removebg;
        const format = backdrop === 'transparent' ? 'png' : 'jpeg';
        expect(files.length);
        const skipped: string[] = [];
        for (const f of files) {
          check();
          let mask: Mask;
          try {
            mask = await maskFor(f);
          } catch (e) {
            // A picture with no subject is left out, not the reason the rest fail.
            if (!/no-subject/.test(errorText(e))) throw e;
            skipped.push(f.input.name);
            tick();
            continue;
          }
          const img = await loadImage(f.input.path);
          const canvas = cutOut(img, mask, backdrop, trim);
          const bytes = await encodeCanvas(canvas, format);
          canvas.width = canvas.height = 0;
          await api.putOutput(started.id, outputName('removebg', f.input.name, undefined, extensionFor(format)), bytes);
          tick();
        }
        if (skipped.length === files.length) throw new Error('no-subject');
        if (skipped.length) {
          showAppNotice(i18n.global.t('utilities.errors.some_without_subject', { names: skipped.join(', ') }, skipped.length));
        }
        result = await api.getJob(started.id);
        break;
      }
      case 'imgtext': {
        const languages = ocrLanguages(vision.value, options.imgtext.language);
        expect(files.length);
        for (const f of files) {
          check();
          const px = pixelsOf(await loadImage(f.input.path), 3000);
          const lines = await api.recognize(px.width, px.height, px.rgba, languages);
          await api.putOutput(started.id, outputName('imgtext', f.input.name), new TextEncoder().encode(linesToText(lines) + '\n'));
          tick();
        }
        result = await api.getJob(started.id);
        break;
      }
      case 'ocr': {
        const o = options.ocr;
        const languages = ocrLanguages(vision.value, o.language);
        const tr = i18n.global.t;
        expect(files.reduce((n, f) => n + (f.pages ?? 0), 0));
        const found: api.SearchableFile[] = [];
        for (const f of files) {
          const pages: { page: number; lines: TextLine[] }[] = [];
          const text: string[] = [];
          for (let p = 1; p <= (f.pages ?? 0); p++) {
            check();
            let lines: TextLine[] = [];
            // A page that has text already is not read again; the .txt takes
            // the text it has, not a "no text here".
            const own = o.skipText ? await pageWords(f.input.path, p) : '';
            if (!own) {
              check();
              const px = await pagePixels(f.input.path, p);
              lines = await api.recognize(px.width, px.height, px.rgba, languages);
            }
            pages.push({ page: p, lines });
            text.push(`—— ${tr('utilities.options.text_page')} ${p} ——\n\n${own || linesToText(lines) || tr('utilities.options.text_none')}`);
            tick();
          }
          found.push({ input: f.input.id, name: outputName('ocr', f.input.name), pages });
          if (o.text) await api.putOutput(started.id, outputName('pdf2text', f.input.name), new TextEncoder().encode(text.join('\n\n') + '\n'));
        }
        result = o.pdf ? await api.searchable(started.id, found) : await api.getJob(started.id);
        break;
      }
      case 'exif': {
        // JPEG and PNG are cleaned in Rust without re-encoding; anything else
        // is redrawn, which leaves nothing of what it carried.
        const inPlace = files.filter((f) => cleansInPlace(f.input.name));
        const redraw = files.filter((f) => !cleansInPlace(f.input.name));
        expect(redraw.length);
        for (const f of redraw) {
          check();
          await putPicture(started.id, f, f.width!, f.height!, ownFormat(f.input.name, webp.value), 0.92, 'exif');
          tick();
        }
        result = inPlace.length ? await api.stripMetadata(started.id, perFile(t.id, inPlace)) : await api.getJob(started.id);
        break;
      }
      case 'imgcompress':
      case 'resize':
      case 'crop':
      case 'imgconvert': {
        expect(files.length);
        for (const f of files) {
          check();
          const own = ownFormat(f.input.name, webp.value);
          let [w, h] = [f.width!, f.height!];
          let format = own;
          let quality = 0.92;
          let region: Region | undefined;
          if (t.id === 'imgcompress') {
            format = options.imgcompress.format === 'keep' ? own : options.imgcompress.format;
            quality = options.imgcompress.quality;
          } else if (t.id === 'imgconvert') {
            format = options.imgconvert.format;
          } else if (t.id === 'resize') {
            const r = options.resize;
            [w, h] = r.mode === 'percent'
              ? [Math.max(1, Math.round((w * r.percent) / 100)), Math.max(1, Math.round((h * r.percent) / 100))]
              : fitWithin(w, h, r.width, r.height, r.enlarge);
          } else {
            region = { ...options.crop.rect! };
            [w, h] = [region.width, region.height];
          }
          await putPicture(started.id, f, w, h, format, quality, t.id, region);
          tick();
        }
        result = await api.getJob(started.id);
        break;
      }
    }
    // Cancelled while the last step finished anyway: its result is not wanted.
    check();
    job.value = result;
    before.value = sizeIn;
    phase.value = 'done';
    void refreshUnsaved();
  } catch (e) {
    if (started) void api.discard(started.id).catch(() => {}).then(refreshUnsaved);
    if (shown()) {
      failure.value = run.cancelled ? null : explain(e);
      phase.value = 'loaded';
    }
  } finally {
    if (active === run) active = null;
  }
}

function cancel() {
  if (!active) return;
  active.cancelled = true;
  if (active.job) void api.cancel(active.job);
}

/** Back from a result to the files and options that made it. */
function editAgain() {
  phase.value = items.value.length ? 'loaded' : 'empty';
  job.value = null;
  before.value = undefined;
}

/** Open a result from the sidebar. */
function showJob(saved: api.Job) {
  const id = toolById(saved.tool)?.id;
  if (!id) return;
  openTool(id);
  job.value = saved;
  phase.value = 'done';
}

/** Hand a result to another tool, as its input. */
async function chain(next: ToolId) {
  const from = job.value;
  if (!from) return;
  const inputs = await Promise.all(from.outputs.map((o) => api.inputFromOutput(from.id, o.name)));
  openTool(next, inputs);
}

async function refreshUnsaved() {
  try {
    const found = (await api.unsavedResults()) ?? [];
    // A run still going writes its files as it goes, so it has outputs, but it
    // is not a result yet: opening it from the list would cancel it and
    // throw those files away under the result on screen.
    unsaved.value = found.filter((j) => j.id !== active?.job);
  } catch {
    unsaved.value = [];
  }
}

function noteSaved(updated: api.Job) {
  if (job.value?.id === updated.id) job.value = updated;
  void refreshUnsaved();
}

let unlisten: (() => void) | null = null;
async function listenForProgress() {
  if (unlisten) return;
  unlisten = await api.onProgress((p) => {
    if (!active || p.job !== active.job) return;
    progress.done = p.done;
    progress.total = p.total;
  });
}

function stopListening() {
  unlisten?.();
  unlisten = null;
}

export function useWorkbench() {
  return {
    tool, current, items, usable, firstPdf, phase, progress, job, before, failure, options, unsaved, splitPlan, ready, webp,
    resetCrop, defaultCrop, vision, askVision, maskFor, office, askOffice, installing, installError, installPdf2docx,
    openTool, closeTool, addInputs, removeItem, moveItem, sortByName, clearItems, run, cancel, editAgain,
    showJob, chain, refreshUnsaved, noteSaved, listenForProgress, stopListening,
  };
}

/** For tests: back to a fresh screen. */
export function resetWorkbench() {
  closeTool();
  unsaved.value = [];
}
