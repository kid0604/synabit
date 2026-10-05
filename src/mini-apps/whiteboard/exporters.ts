/**
 * A board as a file other tools open: draw.io and Excalidraw.
 *
 * The reverse of `importers.ts`, and written to come back through it: what
 * the importers read (boxes, their words and colours, lines with their ends
 * and bends, frames, freehand) goes out as the thing they read it from. What
 * a format cannot hold natively goes out as the nearest thing it can still
 * edit — a draw.io stencil drawn from the shape's own outline, an Excalidraw
 * polygon — and only as a picture when there is nothing editable to make.
 *
 * Pure: the only thing from outside is `imageData`, which hands over the
 * bytes of a picture in the vault. Ids, seeds and stamps are worked out from
 * the board, so the same board always gives the same file.
 */
import { parsePath, samplePath, type Seg } from './pathGeometry';
import type { WBEdge, WBNode } from './boardFile';
import { EDGE_GREY } from './composables/useNodeOperations';
import { cleanGlyph, GLYPH_SHAPE, GLYPH_SIZE, glyphSvg } from './glyph';
import { labelOn, paint } from './ink';
import { strokeSize, type Box } from './inkLayer';
import { estimateSize, hiddenByCollapse } from './mindmap';
import { anchor, sidesFacing, type Point, type Side } from './routing';
import { SHAPES_MAP, type ShapeDef } from './shapes';
import { STICKY_SIZE, stickyColor } from './sticky';
import { CARD_SIZE } from './vaultCards';

export interface ExportInput { nodes: WBNode[]; edges: WBEdge[]; title?: string }
export interface ExportOptions {
  /** The bytes of a picture in the vault as a `data:` URI, or null when it cannot be read. */
  imageData?: (assetPath: string) => Promise<string | null>;
}

// ─── What is on the board, and where ────────────────────────

const INK_HEX = '#1e1e1e';
const FRAME_HEX = '#94a3b8';

/**
 * A stored colour as hex another tool can use.
 *
 * Stored colours are hex, but what the board paints with can be a theme
 * variable (`var(--wb-ink, #1e1e1e)`): the light theme's value — the
 * fallback inside the `var()` — is the one written out, since both formats
 * open on white.
 */
function hexOf(c: string | null | undefined): { hex: string; alpha: number } | null {
  let v = paint(c)?.trim();
  if (!v || v === 'none' || v === 'transparent') return null;
  for (let i = 0; i < 4 && v.startsWith('var('); i++) {
    const comma = v.indexOf(',');
    v = comma < 0 ? '' : v.slice(comma + 1, v.lastIndexOf(')')).trim();
  }
  if (!v || v === 'currentColor') return { hex: INK_HEX, alpha: 1 };
  const h = v.replace(/^#/, '');
  if (!/^([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i.test(h)) return null;
  const full = h.length === 3 ? h.replace(/./g, (x) => x + x) : h;
  return { hex: `#${full.slice(0, 6).toLowerCase()}`, alpha: full.length === 8 ? parseInt(full.slice(6), 16) / 255 : 1 };
}
const hex = (c: string | null | undefined, fallback: string) => hexOf(c)?.hex ?? fallback;

/** `color` laid over white at `alpha`, as hex: a tint neither format needs an alpha channel for. */
function tint(c: string, alpha: number): string {
  const h = hex(c, FRAME_HEX).slice(1);
  const ch = (i: number) => Math.round(255 - (255 - parseInt(h.slice(i, i + 2), 16)) * alpha).toString(16).padStart(2, '0');
  return `#${ch(0)}${ch(2)}${ch(4)}`;
}

const r2 = (n: number) => Math.round(n * 100) / 100;

/** A text item's words are Markdown; the other tools get the words. */
function plainText(md: string): string {
  return md
    .replace(/```[^\n]*\n([\s\S]*?)```/g, '$1')
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g, (_m, a, b) => b ?? a)
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
    .replace(/^#{1,6}\s+/gm, '')
    .replace(/^\s*>\s?/gm, '')
    .replace(/^(\s*)[-*+]\s+\[([ xX])\]\s+/gm, (_m, s, x) => `${s}${x === ' ' ? '☐' : '☑'} `)
    .replace(/^(\s*)[-*+]\s+/gm, '$1• ')
    .replace(/(\*\*|__)(?=\S)([\s\S]*?\S)\1/g, '$2')
    .replace(/(^|[^\w*])\*(?=\S)([^*\n]*?\S)\*(?!\w)/g, '$1$2')
    .replace(/(^|[^\w_])_(?=\S)([^_\n]*?\S)_(?!\w)/g, '$1$2')
    .replace(/~~(.+?)~~/g, '$1')
    .replace(/`([^`]+)`/g, '$1')
    .replace(/\\([\\`*_{}[\]()#+\-.!>~|])/g, '$1')
    .replace(/[ \t]+$/gm, '')
    .trim();
}

/** About how tall `text` is at `fontSize` in a column `width` wide. */
function textHeight(text: string, fontSize: number, width: number, lineHeight = 1.4): number {
  const perLine = Math.max(1, Math.floor(width / (fontSize * 0.55)));
  const lines = text.split('\n').reduce((n, line) => n + Math.max(1, Math.ceil(line.length / perLine)), 0);
  return Math.ceil(lines * fontSize * lineHeight);
}

/** A sticky note picks the largest type its words fit in; this is a guess at the one it picked. */
function stickyFont(label: string, w: number, h: number): number {
  const area = Math.max(1, (w - 24) * (h - 24));
  return Math.max(10, Math.min(28, Math.floor(Math.sqrt(area / (Math.max(1, label.length) * 0.55 * 1.4)))));
}

/** The words an item shows, as plain text. */
function labelOf(n: WBNode): string {
  const d = n.data ?? {};
  switch (n.type) {
    case 'text': return plainText(String(d.label ?? ''));
    case 'note': return String(d.noteTitle || d.noteId || '');
    case 'card': return String(d.title || d.ref || '');
    case 'image': return String(d.alt || d.assetPath?.split('/').pop() || '');
    default: return String(d.label ?? '');
  }
}

function boxOf(n: WBNode): Box {
  const d = n.data ?? {};
  const at = { x: n.position.x, y: n.position.y };
  switch (n.type) {
    case 'shape': {
      const def = SHAPES_MAP[d.shapeType];
      const glyph = d.shapeType === GLYPH_SHAPE;
      return { ...at, width: d.width || (glyph ? GLYPH_SIZE : def?.defaultWidth) || 160, height: d.height || (glyph ? GLYPH_SIZE : def?.defaultHeight) || 80 };
    }
    case 'sticky': return { ...at, width: d.width || STICKY_SIZE, height: d.height || STICKY_SIZE };
    case 'text': {
      const width = d.width || 200;
      return { ...at, width, height: d.height || textHeight(labelOf(n), d.fontSize || 16, width - 8) + 8 };
    }
    case 'frame': return { ...at, width: d.width || 480, height: d.height || 320 };
    case 'mindmap': {
      const guess = estimateSize(n);
      return { ...at, width: d.width || guess.width, height: d.height || guess.height };
    }
    case 'note': return { ...at, width: d.width || 280, height: d.height || 180 };
    case 'card': return { ...at, width: d.width || CARD_SIZE.width, height: d.height || CARD_SIZE.height };
    case 'image': return { ...at, width: d.width || 320, height: d.height || 240 };
    case 'stroke': return { ...at, ...strokeSize(d) };
    default: return { ...at, width: d.width || 160, height: d.height || 80 };
  }
}

const inside = (inner: Box, outer: Box) =>
  inner.x >= outer.x && inner.y >= outer.y && inner.x + inner.width <= outer.x + outer.width && inner.y + inner.height <= outer.y + outer.height;

/**
 * What goes in the file: everything but comments (notes for whoever edits
 * the board, left out of every export) and what a folded mind-map branch
 * hides, and the lines between what is left.
 */
function visible(board: ExportInput) {
  const hidden = hiddenByCollapse(board.nodes, board.edges);
  const nodes = board.nodes.filter((n) => n.type !== 'comment' && !hidden.has(n.id));
  const ids = new Set(nodes.map((n) => n.id));
  const edges = board.edges.filter((e) => ids.has(e.source) && ids.has(e.target));
  const boxes = new Map(nodes.map((n) => [n.id, boxOf(n)]));
  // The smallest frame an item lies wholly inside, if any.
  const frames = nodes.filter((n) => n.type === 'frame').sort((a, b) => area(boxes.get(b.id)!) - area(boxes.get(a.id)!));
  const frameOf = new Map<string, string>();
  for (const n of nodes) {
    const box = boxes.get(n.id)!;
    let best: WBNode | undefined;
    for (const f of frames) {
      if (f.id === n.id) continue;
      const fb = boxes.get(f.id)!;
      if (n.type === 'frame' && area(fb) <= area(box)) continue;
      if (inside(box, fb)) best = f;
    }
    if (best) frameOf.set(n.id, best.id);
  }
  return { nodes, edges, boxes, frames, frameOf };
}
const area = (b: Box) => b.width * b.height;

/** The side of an item a line leaves from: the handle it was drawn from, or the side facing the other end. */
function sideOf(handle: string | undefined): Side | undefined {
  const s = handle?.split('-')[0];
  return s === 'top' || s === 'right' || s === 'bottom' || s === 'left' ? s : undefined;
}

// ─── Small things both formats need ─────────────────────────

/** A stable 31-bit number from a string: seeds, nonces, file ids. */
function hash(s: string, seed = 0x811c9dc5): number {
  let h = seed >>> 0;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 0x01000193) >>> 0;
  return h & 0x7fffffff;
}

function base64(bytes: Uint8Array): string {
  let binary = '';
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(binary);
}
const utf8Base64 = (s: string) => base64(new TextEncoder().encode(s));

/** A data URI split into its type and base64 body, whatever form it came in. */
function dataUri(uri: string): { mime: string; data: string } | null {
  const m = /^data:([^;,]+)((?:;[^;,]*)*?)(;base64)?,(.*)$/s.exec(uri);
  if (!m) return null;
  if (m[3]) return { mime: m[1], data: m[4] };
  try {
    return { mime: m[1], data: utf8Base64(decodeURIComponent(m[4])) };
  } catch {
    return null;
  }
}

const svgUri = (svg: string) => `data:image/svg+xml;base64,${utf8Base64(svg)}`;

function xml(s: string): string {
  return s
    .replace(/[\u0000-\u0008\u000B\u000C\u000E-\u001F￾￿]/g, '')
    .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&apos;')
    .replace(/\n/g, '&#10;').replace(/\r/g, '&#13;').replace(/\t/g, '&#9;');
}

/** A label as draw.io HTML: the words exactly, never markup. */
const html = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/\n/g, '<br>');

// ─── SVG paths ──────────────────────────────────────────────



/** The path as a draw.io stencil `<path>`, its numbers moved and scaled by `map`. */
function stencilPath(segs: Seg[], map: (x: number, y: number) => [number, number]): string {
  const pt = (a: number, b: number, k = '') => {
    const [px, py] = map(a, b);
    return `x${k}="${r2(px)}" y${k}="${r2(py)}"`;
  };
  const body = segs.map((s) => {
    switch (s.c) {
      case 'M': return `<move ${pt(s.p[0], s.p[1])}/>`;
      case 'L': return `<line ${pt(s.p[0], s.p[1])}/>`;
      case 'Q': return `<quad ${pt(s.p[0], s.p[1], '1')} ${pt(s.p[2], s.p[3], '2')}/>`;
      case 'C': return `<curve ${pt(s.p[0], s.p[1], '1')} ${pt(s.p[2], s.p[3], '2')} ${pt(s.p[4], s.p[5], '3')}/>`;
      case 'Z': return '<close/>';
    }
  }).join('');
  return `<path>${body}</path>`;
}


/**
 * Shapes are drawn in a 100×100 box, but only 2–98 of it is shown,
 * stretched to the item (`viewBox="2 2 96 96"`): this is that stretch.
 */
const shapeMap = (w: number, h: number) => (x: number, y: number): [number, number] => [((x - 2) / 96) * w, ((y - 2) / 96) * h];

const rotateAbout = (p: Point, c: Point, deg: number): Point => {
  if (!deg) return p;
  const a = (deg * Math.PI) / 180, cos = Math.cos(a), sin = Math.sin(a);
  return { x: c.x + (p.x - c.x) * cos - (p.y - c.y) * sin, y: c.y + (p.x - c.x) * sin + (p.y - c.y) * cos };
};

const RECT_PATH = SHAPES_MAP.rectangle.path;
const ELLIPSE_PATH = SHAPES_MAP.ellipse.path;
const DIAMOND_PATH = SHAPES_MAP.diamond.path;
/** A shape that is plainly one of the three every tool has. */
function plainKind(def: ShapeDef | undefined, shapeType: string): 'rectangle' | 'roundedRect' | 'ellipse' | 'diamond' | null {
  if (shapeType === 'roundedRect' || shapeType === 'pill') return 'roundedRect';
  if (!def || def.deco?.length || def.labelBelow) return def ? null : 'rectangle';
  if (def.path === RECT_PATH) return 'rectangle';
  if (def.path === ELLIPSE_PATH) return 'ellipse';
  if (def.path === DIAMOND_PATH) return 'diamond';
  return null;
}

// ─── draw.io ────────────────────────────────────────────────

/** Board line ends as draw.io arrows: `DRAWIO_ENDS` read backwards. */
const DRAWIO_ARROWS: Record<string, [string, 0 | 1]> = {
  arrow: ['classic', 1], 'arrow-open': ['open', 0],
  // draw.io's hollow triangle; the importer reads `block` back as a filled arrow.
  'triangle-open': ['block', 0],
  diamond: ['diamond', 1], 'diamond-open': ['diamond', 0],
  circle: ['oval', 1], 'circle-open': ['oval', 0],
  'er-one': ['ERmandOne', 0], 'er-many': ['ERmany', 0], 'er-one-many': ['ERoneToMany', 0],
  'er-zero-one': ['ERzeroToOne', 0], 'er-zero-many': ['ERzeroToMany', 0],
};
// Looked up with names read from the board file: no prototype to find "constructor" on.
Object.setPrototypeOf(DRAWIO_ARROWS, null);

/**
 * Board shapes draw.io draws itself: `DRAWIO_SHAPES` read backwards, each
 * set to the board's proportions. Anything else goes out as a stencil of
 * its own outline (see `drawioStencil`).
 *
 * `chevron` is draw.io's `step`, which the importer reads back as the
 * board's (different) `step`; the board's `step` itself has no draw.io twin
 * and goes out as a stencil.
 */
const DRAWIO_NATIVE: Record<string, (w: number, h: number) => string> = {
  rectangle: () => 'rounded=0',
  roundedRect: () => 'rounded=1;absoluteArcSize=1;arcSize=24',
  pill: () => 'rounded=1;arcSize=50',
  process: () => 'shape=process;size=0',
  predefinedProcess: () => 'shape=process;size=0.14',
  ellipse: () => 'ellipse',
  diamond: () => 'rhombus',
  decision: () => 'rhombus',
  triangle: () => 'triangle;direction=north',
  invertedTriangle: () => 'triangle;direction=south',
  hexagon: () => 'shape=hexagon;perimeter=hexagonPerimeter2;fixedSize=0;size=0.24',
  preparation: () => 'shape=hexagon;perimeter=hexagonPerimeter2;fixedSize=0;size=0.17',
  cylinder: (_w, h) => `shape=cylinder3;boundedLbl=1;backgroundOutline=1;size=${Math.round(h * 0.125)}`,
  parallelogram: () => 'shape=parallelogram;perimeter=parallelogramPerimeter;fixedSize=0;size=0.24',
  dataIO: () => 'shape=parallelogram;perimeter=parallelogramPerimeter;fixedSize=0;size=0.24',
  trapezoid: () => 'shape=trapezoid;perimeter=trapezoidPerimeter;fixedSize=0;size=0.19',
  manualOperation: () => 'shape=trapezoid;perimeter=trapezoidPerimeter;fixedSize=0;size=0.17;flipV=1',
  document: () => 'shape=document;boundedLbl=1;size=0.2',
  cloud: () => 'shape=cloud',
  umlActor: () => 'shape=umlActor;verticalLabelPosition=bottom;verticalAlign=top;outlineConnect=0',
  note: (w, h) => `shape=note;size=${Math.round(Math.min(w, h) * 0.27)}`,
  umlNote: (w, h) => `shape=note;size=${Math.round(Math.min(w, h) * 0.27)}`,
  chevron: () => 'shape=step;perimeter=stepPerimeter;fixedSize=0;size=0.27',
  calloutRect: (w, h) => `shape=callout;perimeter=calloutPerimeter;size=${Math.round(h * 0.24)};position=0.4;position2=0.29;base=${Math.round(w * 0.21)}`,
};
// Looked up with names read from the board file: no prototype to find "constructor" on.
Object.setPrototypeOf(DRAWIO_NATIVE, null);

/**
 * `key=value;…;` — draw.io's style string.
 *
 * The plain strings are this file's own; the `[key, value]` pairs carry what
 * was read from the board, and a board may have come from a file anyone
 * wrote. A value ends at the first `;` — a font size of
 * `13;shape=image;image=…` is 13, not three keys — so no value can add
 * a key. An `=` is left alone: draw.io splits each entry at its first `=`,
 * so one inside a value stays the value's, and a picture's base64 needs it.
 * A number that is not finite is no number at all, and left out.
 */
export function style(parts: (string | [string, string | number | undefined | null | false])[]): string {
  const value = (v: string | number | undefined | null | false): string => {
    if (v === undefined || v === null || v === false) return '';
    if (typeof v === 'number') return Number.isFinite(v) ? String(v) : '';
    return String(v).split(';', 1)[0].replace(/[\r\n]/g, '').trim();
  };
  return parts
    .map((p) => {
      if (typeof p === 'string') return p;
      const v = value(p[1]);
      return v ? `${p[0]}=${v}` : '';
    })
    .filter(Boolean)
    .join(';') + ';';
}

async function deflateBase64(text: string): Promise<string> {
  const stream = new Response(new TextEncoder().encode(text)).body!.pipeThrough(new CompressionStream('deflate-raw'));
  return base64(new Uint8Array(await new Response(stream).arrayBuffer()));
}

/**
 * A shape as a draw.io stencil: its own outline, in draw.io's own drawing
 * language, packed into the style the way draw.io packs one (`stencil(…)`
 * is base64 of raw-deflated, URI-encoded XML). It stays a shape there —
 * recoloured, resized, joined to — rather than becoming a picture of one.
 * Null when the outline uses something a stencil cannot say (arcs).
 */
async function drawioStencil(main: string, deco: string[], w: number, h: number, map: (x: number, y: number) => [number, number], fill = true): Promise<string | null> {
  const segs = parsePath(main);
  const decoSegs = deco.map(parsePath);
  if (!segs || decoSegs.some((s) => !s)) return null;
  const decoXml = decoSegs.map((s) => `${stencilPath(s!, map)}<stroke/>`).join('');
  const shape = `<shape w="${r2(w)}" h="${r2(h)}" aspect="variable" strokewidth="inherit"><connections/>`
    + `<background>${stencilPath(segs, map)}</background>`
    + `<foreground>${fill ? '<fillstroke/>' : '<stroke/>'}${decoXml}</foreground></shape>`;
  return `stencil(${await deflateBase64(encodeURIComponent(shape))})`;
}

/** A data URI as draw.io writes one in a style: `data:image/png,<base64>`, since `;` ends a style value. */
function drawioImage(uri: string): string | null {
  const parsed = dataUri(uri);
  return parsed ? `data:${parsed.mime},${parsed.data}` : null;
}

interface Cell { id: string; value?: string; style: string; parent: string; box: Box; vertex?: boolean }

function vertexXml(c: Cell): string {
  return `<mxCell id="${xml(c.id)}" value="${xml(c.value ?? '')}" style="${xml(c.style)}" vertex="1" parent="${xml(c.parent)}">`
    + `<mxGeometry x="${r2(c.box.x)}" y="${r2(c.box.y)}" width="${r2(c.box.width)}" height="${r2(c.box.height)}" as="geometry"/></mxCell>`;
}

/**
 * The board as an uncompressed draw.io file.
 *
 * Items wholly inside a frame are put in it (draw.io containers move what
 * they hold); everything else, and every line, is on the page itself.
 */
export async function toDrawio(board: ExportInput, opts: ExportOptions = {}): Promise<string> {
  const { nodes, edges, boxes, frames, frameOf } = visible(board);
  const cellId = (id: string) => (id === '0' || id === '1' ? `item-${id}` : id);
  const parentOf = (id: string) => (frameOf.has(id) ? cellId(frameOf.get(id)!) : '1');
  const local = (id: string): Box => {
    const b = boxes.get(id)!;
    const f = frameOf.get(id);
    const fb = f ? boxes.get(f)! : null;
    return fb ? { ...b, x: b.x - fb.x, y: b.y - fb.y } : b;
  };
  const common = (d: Record<string, any>) => [
    ['rotation', d.rotation ? r2(d.rotation) : undefined] as [string, number | undefined],
    ['opacity', typeof d.opacity === 'number' && d.opacity < 100 ? Math.round(d.opacity) : undefined] as [string, number | undefined],
    ['locked', d.locked ? 1 : undefined] as [string, number | undefined],
  ];

  const cells: string[] = [];
  // Frames first, largest first, so a frame exists before what it holds.
  for (const f of frames) {
    const d = f.data ?? {};
    const color = hex(d.color, FRAME_HEX);
    cells.push(vertexXml({
      id: cellId(f.id), value: html(labelOf(f)), parent: parentOf(f.id), box: local(f.id),
      style: style(['rounded=0', 'whiteSpace=wrap', 'html=1', 'container=1', 'collapsible=0', 'recursiveResize=0',
        ['strokeColor', color], ['fillColor', tint(color, 0.07)], ['fontColor', color],
        'verticalAlign=top', 'align=left', 'spacingLeft=10', 'spacingTop=2', 'fontStyle=1', ...common(d)]),
    }));
  }

  for (const n of nodes) {
    if (n.type === 'frame') continue;
    const d = n.data ?? {};
    const box = local(n.id);
    const id = cellId(n.id);
    const parent = parentOf(n.id);
    const label = html(labelOf(n));
    switch (n.type) {
      case 'shape': {
        const stroke = hex(d.color, INK_HEX);
        const fill = hexOf(d.fillColor);
        const look: (string | [string, string | number | undefined])[] = [
          ['strokeColor', stroke], ['fillColor', fill?.hex ?? 'none'],
          ['fillOpacity', fill && fill.alpha < 1 ? Math.round(fill.alpha * 100) : undefined],
          ['strokeWidth', d.borderWidth || 2],
          ...(d.dashStyle === 'dashed' ? ['dashed=1'] : d.dashStyle === 'dotted' ? ['dashed=1', 'dashPattern=1 4'] : []),
          ['fontSize', d.fontSize || 13],
          ...common(d),
        ];
        if (d.shapeType === GLYPH_SHAPE) {
          const glyph = cleanGlyph(d.glyph);
          const svg = glyph ? glyphSvg(glyph, stroke, box.width, box.height, d.borderWidth || 2) : null;
          cells.push(vertexXml({
            id, parent, box, value: label,
            style: style(['shape=image', 'html=1', 'imageAspect=0', 'aspect=fixed', 'verticalLabelPosition=bottom', 'verticalAlign=top',
              ['image', svg ? drawioImage(svgUri(svg)) : undefined], ['imageBackground', fill?.hex],
              ['fontColor', hex(labelOn(null), INK_HEX)], ['fontSize', d.fontSize || 13], ...common(d)]),
          }));
          break;
        }
        const def = SHAPES_MAP[d.shapeType] ?? SHAPES_MAP.rectangle;
        // Words in a head (a class's name, a lifeline's) sit at its top, as on the board.
        const below = def.labelBelow ? ['verticalLabelPosition=bottom', 'verticalAlign=top']
          : def.labelBox && def.labelBox[2] > 50 ? ['verticalAlign=top', `spacingTop=${Math.round((def.labelBox[0] / 100) * box.height)}`]
            : [];
        const fontColor = ['fontColor', hex(def.labelBelow ? labelOn(null) : labelOn(d.fillColor), INK_HEX)] as [string, string];
        const native = DRAWIO_NATIVE[def.id];
        let kind = native ? native(box.width, box.height) : null;
        if (!kind) kind = await drawioStencil(def.path, def.deco ?? [], 96, 96, (x, y) => [x - 2, y - 2]).then((s) => (s ? `shape=${s}` : null));
        if (kind) {
          cells.push(vertexXml({ id, parent, box, value: label, style: style([kind, 'whiteSpace=wrap', 'html=1', ...below, fontColor, ...look]) }));
        } else {
          // An outline a stencil cannot say: a picture of it, with its words.
          const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${box.width}" height="${box.height}" viewBox="2 2 96 96" preserveAspectRatio="none">`
            + `<path d="${xml(def.path)}" fill="${fill?.hex ?? 'none'}" stroke="${stroke}" stroke-width="${d.borderWidth || 2}" vector-effect="non-scaling-stroke"/></svg>`;
          cells.push(vertexXml({ id, parent, box, value: label, style: style(['shape=image', 'html=1', 'imageAspect=0', ['image', drawioImage(svgUri(svg))], fontColor, ...common(d)]) }));
        }
        break;
      }
      case 'sticky': {
        const paper = stickyColor(d.color);
        cells.push(vertexXml({
          id, parent, box, value: label,
          style: style(['rounded=1', 'arcSize=3', 'whiteSpace=wrap', 'html=1', 'shadow=1', 'strokeColor=none', ['fillColor', paper.fill],
            ['fontColor', '#18181b'], ['fontSize', stickyFont(labelOf(n), box.width, box.height)], 'spacing=12', ...common(d)]),
        }));
        break;
      }
      case 'text': {
        const bg = hexOf(d.backgroundColor);
        cells.push(vertexXml({
          id, parent, box, value: label,
          style: style(['text', 'html=1', 'whiteSpace=wrap', ['align', d.textAlign || 'left'], 'verticalAlign=top', 'spacing=4',
            ['fontSize', d.fontSize || 16], ['fontColor', hex(d.color, INK_HEX)], ['fillColor', bg?.hex],
            ['fontStyle', (d.fontWeight === 'bold' || Number(d.fontWeight) >= 600 ? 1 : 0) + (d.fontStyle === 'italic' ? 2 : 0) || undefined],
            ...common(d)]),
        }));
        break;
      }
      case 'mindmap': {
        const color = hex(d.color, '#6366f1');
        cells.push(vertexXml({
          id, parent, box, value: label,
          style: style(['rounded=1', 'arcSize=30', 'whiteSpace=wrap', 'html=1', ['strokeColor', color], ['fillColor', tint(color, 0x12 / 255)],
            ['fontColor', INK_HEX], ['fontSize', d.level === 0 ? 15 : 13], ['fontStyle', d.level === 0 ? 1 : undefined], 'strokeWidth=2']),
        }));
        break;
      }
      case 'note':
      case 'card': {
        const color = hex(d.color, '#d4d4d8');
        cells.push(vertexXml({
          id, parent, box, value: label,
          style: style(['rounded=1', 'absoluteArcSize=1', 'arcSize=16', 'whiteSpace=wrap', 'html=1', ['strokeColor', color], 'fillColor=#ffffff',
            ['fontColor', INK_HEX], 'fontSize=14', 'fontStyle=1', 'verticalAlign=top', 'align=left', 'spacing=12', ...common(d)]),
        }));
        break;
      }
      case 'image': {
        const uri = d.assetPath && opts.imageData ? await opts.imageData(d.assetPath) : null;
        const image = uri ? drawioImage(uri) : null;
        cells.push(vertexXml(image
          ? { id, parent, box, value: '', style: style(['shape=image', 'html=1', 'imageAspect=0', 'verticalLabelPosition=bottom', 'verticalAlign=top', ['image', image], ...common(d)]) }
          : { id, parent, box, value: label, style: style(['rounded=0', 'whiteSpace=wrap', 'html=1', 'dashed=1', 'strokeColor=#a1a1aa', 'fillColor=#f4f4f5', 'fontColor=#71717a', ...common(d)]) }));
        break;
      }
      case 'stroke': {
        const color = hex(d.color, INK_HEX);
        const opacity = typeof d.opacity === 'number' && d.opacity < 1 ? Math.round(d.opacity * 100) : undefined;
        // The outline the board fills, as a stencil filled with the ink's colour.
        let stencil = d.svgPath ? await drawioStencil(String(d.svgPath), [], box.width, box.height, (x, y) => [x, y]) : null;
        let ink = ['strokeColor=none', ['fillColor', color]] as (string | [string, string | number])[];
        if (!stencil && Array.isArray(d.points) && d.points.length > 1) {
          // No outline saved: the line through its points, at its width.
          const line = (d.points as number[][]).map(([x, y], i) => `${i ? 'L' : 'M'}${x},${y}`).join(' ');
          stencil = await drawioStencil(line, [], box.width, box.height, (x, y) => [x, y], false);
          ink = [['strokeColor', color], 'fillColor=none', ['strokeWidth', d.size || 3]];
        }
        if (!stencil) break;
        cells.push(vertexXml({ id, parent, box, value: '', style: style([`shape=${stencil}`, ...ink, ['opacity', opacity], 'connectable=0', ['locked', d.locked ? 1 : undefined]]) }));
        break;
      }
      default:
        cells.push(vertexXml({ id, parent, box, value: label, style: style(['rounded=1', 'whiteSpace=wrap', 'html=1', 'strokeColor=#a1a1aa', 'fillColor=#ffffff']) }));
    }
  }

  for (const e of edges) {
    const d = e.data ?? {};
    const end = DRAWIO_ARROWS[d.markerEnd] ?? ['none', 0];
    const start = DRAWIO_ARROWS[d.markerStart] ?? ['none', 0];
    const step = e.type === 'step' || e.type === 'smoothstep';
    const exit = sideOf(e.sourceHandle);
    const entry = sideOf(e.targetHandle);
    const at = (s: Side): [number, number] => (s === 'top' ? [0.5, 0] : s === 'bottom' ? [0.5, 1] : s === 'left' ? [0, 0.5] : [1, 0.5]);
    const waypoints: Point[] = Array.isArray(d.waypoints) ? d.waypoints.filter((p: any) => Number.isFinite(p?.x) && Number.isFinite(p?.y)) : [];
    const s = style([
      ...(step ? ['edgeStyle=orthogonalEdgeStyle', `rounded=${e.type === 'smoothstep' ? 1 : 0}`, 'orthogonalLoop=1', 'jettySize=auto'] : e.type === 'default' && waypoints.length ? ['curved=1'] : []),
      'html=1',
      ['strokeColor', hex(d.color || EDGE_GREY, '#8b8b8b')],
      ['strokeWidth', d.strokeWidth || 2],
      ...(d.dashStyle === 'dashed' ? ['dashed=1'] : d.dashStyle === 'dotted' ? ['dashed=1', 'dashPattern=1 4'] : []),
      ['endArrow', end[0]], ['endFill', end[1]], ['startArrow', start[0]], ['startFill', start[1]],
      ...(exit ? [['exitX', at(exit)[0]], ['exitY', at(exit)[1]], 'exitDx=0', 'exitDy=0'] as [string, number][] : []),
      ...(entry ? [['entryX', at(entry)[0]], ['entryY', at(entry)[1]], 'entryDx=0', 'entryDy=0'] as [string, number][] : []),
      'labelBackgroundColor=#ffffff', 'fontSize=12',
    ]);
    const points = waypoints.length
      ? `<Array as="points">${waypoints.map((p) => `<mxPoint x="${r2(p.x)}" y="${r2(p.y)}"/>`).join('')}</Array>`
      : '';
    cells.push(`<mxCell id="${xml(cellId(e.id))}" value="${xml(html(String(d.label ?? '')))}" style="${xml(s)}" edge="1" parent="1" source="${xml(cellId(e.source))}" target="${xml(cellId(e.target))}">`
      + `<mxGeometry relative="1" as="geometry">${points}</mxGeometry></mxCell>`);
  }

  const name = board.title?.trim() || 'Page-1';
  return '<mxfile host="Synabit" type="device">'
    + `<diagram id="synabit-board" name="${xml(name)}">`
    + '<mxGraphModel grid="1" gridSize="10" guides="1" tooltips="1" connect="1" arrows="1" fold="1" page="0" pageScale="1" math="0" shadow="0">'
    + '<root><mxCell id="0"/><mxCell id="1" parent="0"/>'
    + cells.join('')
    + '</root></mxGraphModel></diagram></mxfile>';
}

// ─── Excalidraw ─────────────────────────────────────────────

/** Board line ends as Excalidraw arrowheads: `EXCALIDRAW_ENDS` read backwards. */
const EXCALIDRAW_HEADS: Record<string, string> = {
  arrow: 'triangle', 'arrow-open': 'arrow', 'triangle-open': 'triangle_outline',
  circle: 'circle', 'circle-open': 'circle_outline', diamond: 'diamond', 'diamond-open': 'diamond_outline',
  'er-one': 'crowfoot_one', 'er-many': 'crowfoot_many', 'er-one-many': 'crowfoot_one_or_many',
  // Excalidraw has no "zero or …": the nearest it has.
  'er-zero-one': 'crowfoot_one', 'er-zero-many': 'crowfoot_many',
};
// Looked up with names read from the board file: no prototype to find "constructor" on.
Object.setPrototypeOf(EXCALIDRAW_HEADS, null);

type El = Record<string, any>;

const FONT_HELVETICA = 2;
const LINE_HEIGHT = 1.15;

const radians = (deg: number | undefined) => {
  const d = ((Number(deg) || 0) % 360 + 360) % 360;
  return d ? (d * Math.PI) / 180 : 0;
};

/**
 * One element with every field Excalidraw expects. `roughness: 0`: crisp
 * lines, as the board draws them, not Excalidraw's hand-drawn wobble.
 */
function element(id: string, type: string, box: Box, extra: El, stamp = 1): El {
  return {
    id, type, x: r2(box.x), y: r2(box.y), width: r2(box.width), height: r2(box.height), angle: 0,
    strokeColor: INK_HEX, backgroundColor: 'transparent', fillStyle: 'solid', strokeWidth: 2, strokeStyle: 'solid',
    roughness: 0, opacity: 100, groupIds: [], frameId: null, roundness: null,
    seed: hash(id) || 1, version: 1, versionNonce: hash(id, 0x9e3779b9) || 1, isDeleted: false,
    boundElements: null, updated: stamp, link: null, locked: false,
    ...extra,
  };
}

function textElement(id: string, box: Box, text: string, extra: El, stamp: number): El {
  return element(id, 'text', box, {
    text, originalText: text, fontSize: 16, fontFamily: FONT_HELVETICA, textAlign: 'left', verticalAlign: 'top',
    containerId: null, lineHeight: LINE_HEIGHT, autoResize: true, ...extra,
  }, stamp);
}

/** Words written inside a box: centred in it, and moved with it. */
function boundText(container: El, text: string, fontSize: number, color: string, align: { textAlign?: string; verticalAlign?: string } = {}): El {
  const width = Math.max(10, container.width - 10);
  const height = textHeight(text, fontSize, width, LINE_HEIGHT);
  return textElement(`${container.id}-label`, {
    x: container.x + 5, y: container.y + Math.max(0, (container.height - height) / 2), width, height,
  }, text, {
    fontSize, strokeColor: color, textAlign: align.textAlign ?? 'center', verticalAlign: align.verticalAlign ?? 'middle',
    containerId: container.id, angle: container.angle, groupIds: container.groupIds, frameId: container.frameId,
    opacity: container.opacity, locked: container.locked, autoResize: true,
  }, container.updated);
}

function bind(host: El, child: El, type: 'text' | 'arrow') {
  host.boundElements = [...(host.boundElements ?? []), { type, id: child.id }];
}

/** A polyline (or polygon) element through absolute points. */
function lineElement(id: string, pts: Point[], closed: boolean, extra: El, stamp: number): El {
  const x0 = pts[0].x, y0 = pts[0].y;
  const rel = pts.map((p) => [r2(p.x - x0), r2(p.y - y0)]);
  if (closed) rel.push([0, 0]);
  const xs = rel.map((p) => p[0]), ys = rel.map((p) => p[1]);
  return element(id, 'line', { x: x0, y: y0, width: Math.max(...xs) - Math.min(...xs), height: Math.max(...ys) - Math.min(...ys) }, {
    points: rel, lastCommittedPoint: null, startBinding: null, endBinding: null, startArrowhead: null, endArrowhead: null,
    polygon: closed, ...extra,
  }, stamp);
}

/**
 * The board as an Excalidraw file.
 *
 * Excalidraw draws three shapes itself — rectangle, ellipse, diamond — and
 * those go out as themselves. Every other shape goes out as two things,
 * grouped so they move as one:
 *
 *  - its outline, as a `line` polygon (curves sampled). That keeps it
 *    editable — recolour it, drag its corners — where a picture would be a
 *    flat image; and it looks like the shape, where the nearest native box
 *    would not.
 *  - an invisible rectangle over it, which holds its words and is what lines
 *    join to. Excalidraw writes words only inside its three boxes and joins
 *    arrows only to boxes, pictures and words, never to a polygon.
 *
 * Icons and pictures are images, kept in `files`; ink is freehand.
 */
export async function toExcalidraw(board: ExportInput, opts: ExportOptions = {}): Promise<string> {
  const { nodes, edges, boxes, frameOf } = visible(board);
  const out: El[] = [];
  const frameEls: El[] = [];
  const files: Record<string, { mimeType: string; id: string; dataURL: string; created: number; lastRetrieved: number }> = {};
  /** What a line may join to, by item: the element that stands for it. */
  const bindable = new Map<string, El>();

  const addFile = (uri: string): string | null => {
    const parsed = dataUri(uri);
    if (!parsed) return null;
    const dataURL = `data:${parsed.mime};base64,${parsed.data}`;
    const id = `${hash(dataURL).toString(16)}${hash(dataURL, 0x1234567).toString(16)}`;
    files[id] ??= { mimeType: parsed.mime, id, dataURL, created: 1, lastRetrieved: 1 };
    return id;
  };

  for (const n of nodes) {
    const d = n.data ?? {};
    const box = boxes.get(n.id)!;
    const stamp = n.updated || 1;
    const frameId = n.type !== 'frame' ? frameOf.get(n.id) ?? null : null;
    const groups: string[] = d.groupId ? [String(d.groupId)] : [];
    const base: El = {
      angle: radians(d.rotation), groupIds: groups, frameId, locked: !!d.locked,
      opacity: n.type === 'stroke'
        ? Math.round((typeof d.opacity === 'number' ? d.opacity : 1) * 100)
        : typeof d.opacity === 'number' ? Math.round(d.opacity) : 100,
      customData: { synabit: { type: n.type, ...(n.type === 'shape' ? { shapeType: d.shapeType } : {}) } },
    };
    const label = labelOf(n);
    const centre = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
    /** Words under an item (an icon's name, an actor's): free text, grouped with it, turned with it. */
    const caption = (groupIds: string[], fontSize: number) => {
      if (!label) return;
      const width = Math.max(box.width, label.length * fontSize * 0.55);
      const height = textHeight(label, fontSize, width, LINE_HEIGHT);
      const c = rotateAbout({ x: centre.x, y: box.y + box.height + 4 + height / 2 }, centre, Number(d.rotation) || 0);
      out.push(textElement(`${n.id}-label`, { x: c.x - width / 2, y: c.y - height / 2, width, height }, label, {
        fontSize, strokeColor: hex(labelOn(null), INK_HEX), textAlign: 'center', groupIds, frameId, angle: base.angle, locked: base.locked,
        // Read back as the words of the item it is under, not as words of its own.
        customData: { synabit: { captionOf: n.id } },
      }, stamp));
    };

    switch (n.type) {
      case 'shape': {
        const stroke = hex(d.color, INK_HEX);
        const fill = hexOf(d.fillColor);
        const look: El = {
          ...base, strokeColor: stroke, backgroundColor: fill?.hex ?? 'transparent', strokeWidth: d.borderWidth || 2,
          strokeStyle: d.dashStyle === 'dashed' || d.dashStyle === 'dotted' ? d.dashStyle : 'solid',
        };
        const fontSize = d.fontSize || 13;
        if (d.shapeType === GLYPH_SHAPE) {
          const glyph = cleanGlyph(d.glyph);
          const fileId = glyph ? addFile(svgUri(glyphSvg(glyph, stroke, box.width, box.height, d.borderWidth || 2))) : null;
          const own = [`${n.id}-icon`, ...groups];
          // The drawing goes along, so a board read back from this file has the
          // icon itself, not a picture of it.
          const mark = { synabit: { type: 'shape', shapeType: GLYPH_SHAPE, color: stroke, ...(glyph ? { glyph } : {}) } };
          const el = fileId
            ? element(n.id, 'image', box, { ...base, customData: mark, groupIds: label ? own : groups, strokeColor: 'transparent', fileId, status: 'saved', scale: [1, 1], crop: null }, stamp)
            : element(n.id, 'rectangle', box, { ...look, groupIds: label ? own : groups }, stamp);
          out.push(el);
          bindable.set(n.id, el);
          caption(label ? own : groups, fontSize);
          break;
        }
        const def = SHAPES_MAP[d.shapeType] ?? SHAPES_MAP.rectangle;
        const kind = plainKind(SHAPES_MAP[d.shapeType], d.shapeType);
        if (kind) {
          const el = element(n.id, kind === 'roundedRect' ? 'rectangle' : kind, box, {
            ...look, roundness: kind === 'roundedRect' ? { type: 3 } : kind === 'rectangle' ? null : { type: 2 },
          }, stamp);
          out.push(el);
          bindable.set(n.id, el);
          if (label) {
            const t = boundText(el, label, fontSize, hex(labelOn(d.fillColor), INK_HEX));
            bind(el, t, 'text');
            out.push(t);
          }
          break;
        }
        // The outline as polygons, and the invisible box that holds the words.
        const own = [`${n.id}-shape`, ...groups];
        const holder = element(n.id, 'rectangle', box, {
          ...base, groupIds: own, strokeColor: 'transparent', backgroundColor: 'transparent', strokeWidth: 1,
        }, stamp);
        out.push(holder);
        bindable.set(n.id, holder);
        const map = shapeMap(box.width, box.height);
        const toBoard = (x: number, y: number) => {
          const [px, py] = map(x, y);
          return rotateAbout({ x: box.x + px, y: box.y + py }, centre, Number(d.rotation) || 0);
        };
        const outlines = [
          ...samplePath(parsePath(def.path) ?? []).map((sp, i) => ({ ...sp, fill: i === 0 })),
          ...(def.deco ?? []).flatMap((p) => samplePath(parsePath(p) ?? []).map((sp) => ({ ...sp, fill: false }))),
        ];
        outlines.forEach((sp, i) => {
          out.push(lineElement(`${n.id}-outline-${i}`, sp.points.map(([x, y]) => toBoard(x, y)), sp.closed, {
            ...look, angle: 0, groupIds: own, backgroundColor: sp.fill && sp.closed ? look.backgroundColor : 'transparent',
            roundness: null, customData: { synabit: { outlineOf: n.id } },
          }, stamp));
        });
        if (label && !def.labelBelow) {
          const t = boundText(holder, label, fontSize, hex(labelOn(d.fillColor), INK_HEX));
          bind(holder, t, 'text');
          out.push(t);
        } else caption(own, fontSize);
        break;
      }
      case 'sticky': {
        const paper = stickyColor(d.color);
        const el = element(n.id, 'rectangle', box, { ...base, strokeColor: 'transparent', backgroundColor: paper.fill, roundness: { type: 3 } }, stamp);
        out.push(el);
        bindable.set(n.id, el);
        if (label) {
          const t = boundText(el, label, stickyFont(label, box.width, box.height), '#18181b');
          bind(el, t, 'text');
          out.push(t);
        }
        break;
      }
      case 'text': {
        const fontSize = d.fontSize || 16;
        // The board's text box has a little padding inside it; Excalidraw's
        // has none, and the importer adds the padding back.
        const width = Math.max(10, box.width - 24);
        const el = textElement(n.id, { x: box.x, y: box.y, width, height: textHeight(label, fontSize, width, LINE_HEIGHT) }, label, {
          ...base, fontSize, strokeColor: hex(d.color, INK_HEX), textAlign: d.textAlign === 'center' || d.textAlign === 'right' ? d.textAlign : 'left',
          backgroundColor: hexOf(d.backgroundColor)?.hex ?? 'transparent', autoResize: false,
        }, stamp);
        out.push(el);
        bindable.set(n.id, el);
        break;
      }
      case 'frame': {
        const el = element(n.id, 'frame', box, {
          ...base, angle: 0, strokeColor: hex(d.color, '#bbb'), strokeWidth: 2, roundness: null, name: label || null,
        }, stamp);
        frameEls.push(el);
        bindable.set(n.id, el);
        break;
      }
      case 'mindmap':
      case 'note':
      case 'card': {
        const mind = n.type === 'mindmap';
        const color = hex(d.color, mind ? '#6366f1' : '#d4d4d8');
        const el = element(n.id, 'rectangle', box, {
          ...base, strokeColor: color, backgroundColor: mind ? tint(color, 0x12 / 255) : '#ffffff', roundness: { type: 3 },
        }, stamp);
        out.push(el);
        bindable.set(n.id, el);
        if (label) {
          const t = boundText(el, label, mind ? (d.level === 0 ? 15 : 13) : 14, INK_HEX, mind ? {} : { textAlign: 'left', verticalAlign: 'top' });
          bind(el, t, 'text');
          out.push(t);
        }
        break;
      }
      case 'image': {
        const uri = d.assetPath && opts.imageData ? await opts.imageData(d.assetPath) : null;
        const fileId = uri ? addFile(uri) : null;
        if (fileId) {
          const el = element(n.id, 'image', box, { ...base, strokeColor: 'transparent', fileId, status: 'saved', scale: [1, 1], crop: null }, stamp);
          out.push(el);
          bindable.set(n.id, el);
        } else {
          // The picture could not be read: a box where it was, with its name.
          const el = element(n.id, 'rectangle', box, { ...base, strokeColor: '#a1a1aa', backgroundColor: '#f4f4f5', strokeStyle: 'dashed', strokeWidth: 1 }, stamp);
          out.push(el);
          bindable.set(n.id, el);
          if (label) {
            const t = boundText(el, label, 14, '#71717a');
            bind(el, t, 'text');
            out.push(t);
          }
        }
        break;
      }
      case 'stroke': {
        const pts = Array.isArray(d.points) ? (d.points as number[][]).filter((p) => Array.isArray(p) && Number.isFinite(p[0]) && Number.isFinite(p[1])) : [];
        if (pts.length < 2) break;
        // Excalidraw's freehand is points from its first one; the board's are from the stroke's corner.
        const [fx, fy] = pts[0];
        const rel = pts.map(([x, y]) => [r2(x - fx), r2(y - fy)]);
        const xs = rel.map((p) => p[0]), ys = rel.map((p) => p[1]);
        out.push(element(n.id, 'freedraw', {
          x: box.x + fx, y: box.y + fy, width: Math.max(...xs) - Math.min(...xs), height: Math.max(...ys) - Math.min(...ys),
        }, {
          ...base, angle: 0, strokeColor: hex(d.color, INK_HEX), strokeWidth: Math.max(1, (d.size || 3) / 2),
          points: rel, pressures: pts.map((p) => r2(typeof p[2] === 'number' ? p[2] : 0.5)), simulatePressure: false, lastCommittedPoint: null,
        }, stamp));
        break;
      }
    }
  }

  for (const e of edges) {
    const d = e.data ?? {};
    const a = boxes.get(e.source)!;
    const b = boxes.get(e.target)!;
    const waypoints: Point[] = Array.isArray(d.waypoints) ? d.waypoints.filter((p: any) => Number.isFinite(p?.x) && Number.isFinite(p?.y)) : [];
    const firstToward: Box = waypoints.length ? { ...waypoints[0], width: 0, height: 0 } : b;
    const lastFrom: Box = waypoints.length ? { ...waypoints[waypoints.length - 1], width: 0, height: 0 } : a;
    const fromSide = sideOf(e.sourceHandle) ?? sidesFacing(a, firstToward).from;
    const toSide = sideOf(e.targetHandle) ?? sidesFacing(lastFrom, b).to;
    const start = anchor(a, fromSide);
    const end = anchor(b, toSide);
    let path: Point[] = [start, ...waypoints, end];
    if ((e.type === 'step' || e.type === 'smoothstep') && !waypoints.length) {
      // Excalidraw's own elbow arrows need fixed binding points; the corners are written out instead.
      const across = (s: Side) => s === 'left' || s === 'right';
      if (across(fromSide) && across(toSide)) {
        const mx = (start.x + end.x) / 2;
        path = [start, { x: mx, y: start.y }, { x: mx, y: end.y }, end];
      } else if (!across(fromSide) && !across(toSide)) {
        const my = (start.y + end.y) / 2;
        path = [start, { x: start.x, y: my }, { x: end.x, y: my }, end];
      } else {
        path = [start, across(fromSide) ? { x: end.x, y: start.y } : { x: start.x, y: end.y }, end];
      }
      path = path.filter((p, i) => i === 0 || p.x !== path[i - 1].x || p.y !== path[i - 1].y);
      if (path.length < 2) path = [start, end];
    }
    const rel = path.map((p) => [r2(p.x - start.x), r2(p.y - start.y)]);
    const xs = rel.map((p) => p[0]), ys = rel.map((p) => p[1]);
    const from = bindable.get(e.source);
    const to = bindable.get(e.target);
    const arrow = element(e.id, 'arrow', { x: start.x, y: start.y, width: Math.max(...xs) - Math.min(...xs), height: Math.max(...ys) - Math.min(...ys) }, {
      strokeColor: hex(d.color || EDGE_GREY, '#8b8b8b'), strokeWidth: d.strokeWidth || 2,
      strokeStyle: d.dashStyle === 'dashed' || d.dashStyle === 'dotted' ? d.dashStyle : 'solid',
      roundness: (e.type ?? 'default') === 'default' ? { type: 2 } : null,
      points: rel, lastCommittedPoint: null,
      startBinding: from ? { elementId: from.id, focus: 0, gap: 1 } : null,
      endBinding: to ? { elementId: to.id, focus: 0, gap: 1 } : null,
      startArrowhead: EXCALIDRAW_HEADS[d.markerStart] ?? null,
      endArrowhead: EXCALIDRAW_HEADS[d.markerEnd] ?? null,
      elbowed: false,
    }, e.updated || 1);
    if (from) bind(from, arrow, 'arrow');
    if (to && to !== from) bind(to, arrow, 'arrow');
    out.push(arrow);
    const label = String(d.label ?? '').trim();
    if (label) {
      const mid = path.length % 2 ? path[(path.length - 1) / 2] : {
        x: (path[path.length / 2 - 1].x + path[path.length / 2].x) / 2,
        y: (path[path.length / 2 - 1].y + path[path.length / 2].y) / 2,
      };
      const width = Math.min(240, label.length * 12 * 0.6 + 8);
      const height = textHeight(label, 12, width, LINE_HEIGHT);
      const t = textElement(`${e.id}-label`, { x: mid.x - width / 2, y: mid.y - height / 2, width, height }, label, {
        fontSize: 12, strokeColor: arrow.strokeColor, textAlign: 'center', verticalAlign: 'middle', containerId: e.id,
      }, arrow.updated);
      bind(arrow, t, 'text');
      out.push(t);
    }
  }

  return JSON.stringify({
    type: 'excalidraw',
    version: 2,
    source: 'synabit',
    // Frames last: Excalidraw keeps what a frame holds before the frame.
    elements: [...out, ...frameEls],
    appState: { viewBackgroundColor: '#ffffff', ...(board.title?.trim() ? { name: board.title.trim() } : {}) },
    files,
  }, null, 2);
}
