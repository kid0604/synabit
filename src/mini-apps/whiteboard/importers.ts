/**
 * Drawings from other tools, as board items.
 *
 * Excalidraw and draw.io files are what people already have; Mermaid source
 * is what people and assistants write. Each comes in as items and lines on
 * this board — boxes, words, arrows with their ends, freehand — placed where
 * the user dropped or pasted them, through the same path as a paste.
 *
 * What a format has and the board does not (Excalidraw's hand-drawn roughness,
 * draw.io's thousands of stencils) comes in as the nearest thing the board
 * has, never as nothing: an unknown draw.io shape is a rectangle with its
 * label, not a gap in the diagram.
 */
import type { WBEdge, WBNode } from './boardFile';
import { buildStroke } from './composables/useFreeDrawing';
import { SHAPES_MAP } from './shapes';
import { cleanGlyph, GLYPH_SHAPE } from './glyph';

export interface Imported { nodes: WBNode[]; edges: WBEdge[] }

/** A lookup table with no prototype: a key read from a file can be "constructor" or "__proto__". */
const table = (entries: Record<string, string>): Record<string, string> => Object.assign(Object.create(null) as Record<string, string>, entries);

const num = (v: unknown, fallback = 0) => (typeof v === 'number' && Number.isFinite(v) ? v : fallback);
const color = (v: unknown): string | undefined =>
  typeof v === 'string' && /^#[0-9a-f]{3,8}$/i.test(v.trim()) ? v.trim() : undefined;

// ─── Excalidraw ─────────────────────────────────────────────

const EXCALIDRAW_ENDS: Record<string, string> = table({
  arrow: 'arrow-open', triangle: 'arrow', triangle_outline: 'triangle-open',
  dot: 'circle', circle: 'circle', circle_outline: 'circle-open',
  diamond: 'diamond', diamond_outline: 'diamond-open',
  crowfoot_one: 'er-one', crowfoot_many: 'er-many', crowfoot_one_or_many: 'er-one-many',
});

export function fromExcalidraw(text: string): Imported | null {
  let doc: any;
  try {
    doc = JSON.parse(text);
  } catch {
    return null;
  }
  const elements: any[] = Array.isArray(doc?.elements) ? doc.elements.filter((e: any) => e && !e.isDeleted) : [];
  if (doc?.type !== 'excalidraw' && !elements.length) return null;

  const nodes: WBNode[] = [];
  const edges: WBEdge[] = [];
  const ids = new Set<string>();
  // Words written inside a box or on an arrow belong to it.
  const boundText = new Map<string, any>();
  for (const e of elements) if (e.type === 'text' && e.containerId) boundText.set(e.containerId, e);
  // A file this app wrote: a shape Excalidraw has no box for went out as its
  // outline plus a clear box holding the words. The box comes back as the
  // shape it was, in the outline's colours, and the outline is not drawn twice.
  const outlines = new Map<string, any>();
  for (const e of elements) {
    const of = e.customData?.synabit?.outlineOf;
    if (typeof of === 'string' && !outlines.has(of)) outlines.set(of, e);
  }
  // The words this app wrote under an icon, which are the icon's own.
  const captions = new Map<string, any>();
  for (const e of elements) {
    const of = e.customData?.synabit?.captionOf;
    if (e.type === 'text' && typeof of === 'string') captions.set(of, e);
  }
  const words = (t: any) => String(t?.originalText ?? t?.text ?? '');
  const ownShape = (e: any): string | undefined => {
    const kind = e.customData?.synabit?.shapeType;
    return typeof kind === 'string' && SHAPES_MAP[kind] ? kind : undefined;
  };

  const style = (e: any) => ({
    color: color(e.strokeColor),
    fillColor: e.backgroundColor && e.backgroundColor !== 'transparent' ? color(e.backgroundColor) : undefined,
    borderWidth: Math.min(5, Math.max(1, Math.round(num(e.strokeWidth, 2)))),
    dashStyle: e.strokeStyle === 'dashed' || e.strokeStyle === 'dotted' ? e.strokeStyle : undefined,
    opacity: num(e.opacity, 100) === 100 ? undefined : num(e.opacity, 100),
    rotation: e.angle ? Math.round((num(e.angle) * 180) / Math.PI) : undefined,
  });

  for (const e of elements) {
    const base = { position: { x: num(e.x), y: num(e.y) }, updated: 0 };
    // The words as written, not as Excalidraw wrapped them to fit the box.
    const label = boundText.has(e.id) ? words(boundText.get(e.id)) : '';
    if (e.customData?.synabit?.outlineOf || e.customData?.synabit?.captionOf) continue;
    if (e.type === 'rectangle' || e.type === 'ellipse' || e.type === 'diamond') {
      const own = ownShape(e);
      const shapeType = own ?? (e.type === 'rectangle' ? (e.roundness ? 'roundedRect' : 'rectangle') : e.type);
      const look = own && outlines.has(e.id) ? { ...style(outlines.get(e.id)), rotation: style(e).rotation } : style(e);
      nodes.push({
        id: e.id, type: 'shape', ...base,
        data: { shapeType, label, width: Math.round(num(e.width, 160)), height: Math.round(num(e.height, 80)), fontSize: boundText.has(e.id) ? num(boundText.get(e.id).fontSize, 13) : undefined, ...look },
      });
      ids.add(e.id);
    } else if (e.type === 'image') {
      const glyph = e.customData?.synabit?.shapeType === GLYPH_SHAPE ? cleanGlyph(e.customData.synabit.glyph) : null;
      const size = { width: Math.round(num(e.width, 64)), height: Math.round(num(e.height, 64)) };
      if (glyph) {
        // An icon this app wrote: the icon itself, its words with it.
        nodes.push({ id: e.id, type: 'shape', ...base, data: { shapeType: GLYPH_SHAPE, glyph, label: words(captions.get(e.id)), ...size, color: color(e.customData?.synabit?.color) } });
        ids.add(e.id);
      } else {
        // A picture: its bytes come along, to be put in the vault when the
        // drawing is placed (see `pendingPicture`).
        const file = doc?.files?.[e.fileId];
        const dataUrl = typeof file?.dataURL === 'string' && /^data:image\/(png|jpe?g|gif|webp|svg\+xml)[;,]/.test(file.dataURL) ? file.dataURL : null;
        if (dataUrl) {
          nodes.push({ id: e.id, type: 'image', ...base, data: { pendingPicture: dataUrl, alt: '', ...size, rotation: style(e).rotation } });
          ids.add(e.id);
        }
      }
    } else if (e.type === 'text' && !e.containerId) {
      nodes.push({
        id: e.id, type: 'text', ...base,
        data: { label: words(e), fontSize: num(e.fontSize, 20), color: color(e.strokeColor), width: Math.max(80, Math.round(num(e.width, 200)) + 24), rotation: style(e).rotation },
      });
      ids.add(e.id);
    } else if (e.type === 'frame' || e.type === 'magicframe') {
      nodes.push({ id: e.id, type: 'frame', ...base, data: { label: String(e.name ?? ''), width: Math.round(num(e.width, 480)), height: Math.round(num(e.height, 320)) } });
      ids.add(e.id);
    }
  }

  for (const e of elements) {
    if (e.customData?.synabit?.outlineOf) continue;
    if (e.type === 'arrow' || e.type === 'line') {
      const from = e.startBinding?.elementId;
      const to = e.endBinding?.elementId;
      if (from && to && ids.has(from) && ids.has(to)) {
        edges.push({
          id: e.id, source: from, target: to, type: points(e).length > 2 ? 'straight' : 'default', updated: 0,
          data: {
            color: color(e.strokeColor),
            strokeWidth: Math.min(5, Math.max(1, Math.round(num(e.strokeWidth, 2)))),
            dashStyle: e.strokeStyle === 'dashed' || e.strokeStyle === 'dotted' ? e.strokeStyle : undefined,
            markerEnd: EXCALIDRAW_ENDS[e.endArrowhead] ?? 'none',
            markerStart: EXCALIDRAW_ENDS[e.startArrowhead] ?? 'none',
            label: boundText.has(e.id) ? words(boundText.get(e.id)) || undefined : undefined,
            waypoints: points(e).slice(1, -1).map(([px, py]) => ({ x: num(e.x) + px, y: num(e.y) + py })),
          },
        });
        continue;
      }
    }
    // Freehand, and lines not joined to anything, are ink.
    if (e.type === 'freedraw' || e.type === 'arrow' || e.type === 'line') {
      const pts = points(e).map(([px, py], i) => [num(e.x) + px, num(e.y) + py, num(e.pressures?.[i], 0.5)]);
      const stroke = buildStroke(pts, Math.max(2, num(e.strokeWidth, 1) * 2), e.type === 'freedraw' && !e.simulatePressure);
      if (!stroke) continue;
      nodes.push({
        id: e.id, type: 'stroke', position: { x: stroke.x, y: stroke.y }, updated: 0,
        data: { svgPath: stroke.svgPath, points: stroke.points, width: stroke.width, height: stroke.height, color: color(e.strokeColor) ?? '#1e1e1e', size: Math.max(2, num(e.strokeWidth, 1) * 2), opacity: 0.85 },
      });
    }
  }
  return nodes.length ? { nodes: nodes.map(clean), edges: edges.map(clean) } : null;
}

const points = (e: any): [number, number][] =>
  Array.isArray(e.points) ? e.points.filter((p: any) => Array.isArray(p)).map((p: any) => [num(p[0]), num(p[1])]) : [];

/** Drop the keys left undefined, so the board file stays tidy. */
function clean<T extends { data?: Record<string, any> }>(item: T): T {
  if (item.data) for (const k of Object.keys(item.data)) if (item.data[k] === undefined) delete item.data[k];
  return item;
}

// ─── draw.io ────────────────────────────────────────────────

const DRAWIO_SHAPES: [RegExp, string][] = [
  [/^ellipse|shape=ellipse|^doubleEllipse/, 'ellipse'],
  [/^rhombus|shape=rhombus/, 'diamond'],
  [/^triangle|shape=triangle/, 'triangle'],
  [/shape=hexagon|^hexagon/, 'hexagon'],
  [/shape=cylinder|^cylinder/, 'cylinder'],
  [/shape=parallelogram|^parallelogram/, 'parallelogram'],
  [/shape=trapezoid/, 'trapezoid'],
  [/shape=process/, 'process'],
  [/shape=document/, 'document'],
  [/shape=cloud|^cloud/, 'cloud'],
  [/shape=umlActor/, 'umlActor'],
  [/shape=note/, 'note'],
  [/shape=step/, 'step'],
  [/shape=card|shape=callout/, 'calloutRect'],
  [/shape=dataStorage|shape=cylinder3/, 'cylinder'],
];

const DRAWIO_ENDS: Record<string, string> = table({
  classic: 'arrow', block: 'arrow', classicThin: 'arrow', blockThin: 'arrow',
  open: 'arrow-open', openThin: 'arrow-open',
  diamond: 'diamond', diamondThin: 'diamond', oval: 'circle',
  ERone: 'er-one', ERmandOne: 'er-one', ERmany: 'er-many', ERoneToMany: 'er-one-many',
  ERzeroToOne: 'er-zero-one', ERzeroToMany: 'er-zero-many',
});

/** `key=value;key=value;bare` → a map; bare words map to `''`. */
function parseStyle(style: string): Map<string, string> {
  const map = new Map<string, string>();
  for (const part of style.split(';')) {
    if (!part) continue;
    const at = part.indexOf('=');
    if (at < 0) map.set(part, '');
    else map.set(part.slice(0, at), part.slice(at + 1));
  }
  return map;
}

/**
 * The picture in a draw.io `image=` style value, as a `data:` URL, or null.
 * draw.io writes `data:image/png,<base64>` — no `;base64`, since a style
 * value cannot hold a `;`. Pictures from the web are left out: an import
 * fetches nothing.
 */
function pictureIn(value: string | undefined): string | null {
  const m = /^data:(image\/(?:png|jpe?g|gif|webp|svg\+xml))(;base64)?,(.+)$/.exec(value ?? '');
  if (!m) return null;
  return m[2] || /^[A-Za-z0-9+/=\s]+$/.test(m[3]) ? `data:${m[1]};base64,${m[3].replace(/\s/g, '')}` : `data:${m[1]},${m[3]}`;
}

/** A draw.io label is HTML; the board wants the words. */
function plain(value: string | null): string {
  if (!value) return '';
  const doc = new DOMParser().parseFromString(value.replace(/<br\s*\/?>/gi, '\n').replace(/<\/(div|p)>/gi, '\n'), 'text/html');
  return (doc.body.textContent ?? '').replace(/\n{2,}/g, '\n').trim();
}

/**
 * The diagram inside a draw.io file. Older files and the desktop app store it
 * compressed: base64 of raw-deflated, URI-encoded XML.
 */
async function drawioModel(text: string): Promise<Element | null> {
  const doc = new DOMParser().parseFromString(text, 'application/xml');
  const direct = doc.querySelector('mxGraphModel');
  if (direct) return direct;
  const packed = doc.querySelector('diagram')?.textContent?.trim();
  if (!packed) return null;
  try {
    const bytes = Uint8Array.from(atob(packed), (c) => c.charCodeAt(0));
    const stream = new Response(bytes).body!.pipeThrough(new DecompressionStream('deflate-raw'));
    const xml = decodeURIComponent(await new Response(stream).text());
    return new DOMParser().parseFromString(xml, 'application/xml').querySelector('mxGraphModel');
  } catch {
    return null;
  }
}

export async function fromDrawio(text: string): Promise<Imported | null> {
  if (!/<mxfile|<mxGraphModel/.test(text)) return null;
  const model = await drawioModel(text);
  if (!model) return null;

  const cells = Array.from(model.querySelectorAll('mxCell'));
  const byId = new Map(cells.map((c) => [c.getAttribute('id') ?? '', c]));
  // A cell inside a group or container is placed relative to it.
  const origin = (cell: Element | undefined, seen = new Set<string>()): { x: number; y: number } => {
    const parentId = cell?.getAttribute('parent');
    const parent = parentId ? byId.get(parentId) : undefined;
    if (!parent || seen.has(parentId!) || parent.getAttribute('vertex') !== '1') return { x: 0, y: 0 };
    seen.add(parentId!);
    const g = parent.querySelector(':scope > mxGeometry');
    const up = origin(parent, seen);
    return { x: up.x + Number(g?.getAttribute('x') ?? 0), y: up.y + Number(g?.getAttribute('y') ?? 0) };
  };

  const nodes: WBNode[] = [];
  const edges: WBEdge[] = [];
  const ids = new Set<string>();

  for (const cell of cells) {
    if (cell.getAttribute('vertex') !== '1') continue;
    const id = cell.getAttribute('id') ?? '';
    const g = cell.querySelector(':scope > mxGeometry');
    const style = cell.getAttribute('style') ?? '';
    const s = parseStyle(style);
    const at = origin(cell);
    // Numbers, whatever the file says: one "abc" used to turn the whole paste
    // into items at NaN.
    const attr = (name: string, fallback: number) => num(Number(g?.getAttribute(name) ?? fallback), fallback);
    const x = at.x + attr('x', 0);
    const y = at.y + attr('y', 0);
    const width = Math.round(attr('width', 160));
    const height = Math.round(attr('height', 80));
    const label = plain(cell.getAttribute('value'));
    const fill = s.get('fillColor');
    const common = {
      color: color(s.get('strokeColor')),
      fillColor: fill && fill !== 'none' ? color(fill) : undefined,
      fontSize: s.get('fontSize') ? num(Number(s.get('fontSize')), 13) : undefined,
      dashStyle: s.get('dashed') === '1' ? 'dashed' : undefined,
      rotation: s.get('rotation') ? num(Number(s.get('rotation'))) || undefined : undefined,
    };
    ids.add(id);
    if (s.has('text') || s.get('shape') === 'text') {
      nodes.push({ id, type: 'text', position: { x, y }, updated: 0, data: { label, width, color: color(s.get('fontColor')), fontSize: common.fontSize } });
    } else if ((s.get('shape') === 'image' || s.has('image')) && pictureIn(s.get('image'))) {
      // A picture: its bytes come along, to be put in the vault when the
      // drawing is placed (see `pendingPicture`).
      nodes.push({ id, type: 'image', position: { x, y }, updated: 0, data: { pendingPicture: pictureIn(s.get('image')), alt: label, width, height, rotation: common.rotation } });
    } else if (s.has('swimlane') || s.get('container') === '1' || s.has('group')) {
      nodes.push({ id, type: 'frame', position: { x, y }, updated: 0, data: { label, width, height, color: common.color } });
    } else {
      const known = DRAWIO_SHAPES.find(([re]) => re.test(style))?.[1];
      const shapeType = known ?? (s.get('rounded') === '1' ? 'roundedRect' : 'rectangle');
      nodes.push({ id, type: 'shape', position: { x, y }, updated: 0, data: { shapeType: SHAPES_MAP[shapeType] ? shapeType : 'rectangle', label, width, height, ...common } });
    }
  }

  for (const cell of cells) {
    if (cell.getAttribute('edge') !== '1') continue;
    const source = cell.getAttribute('source') ?? '';
    const target = cell.getAttribute('target') ?? '';
    if (!ids.has(source) || !ids.has(target)) continue;
    const s = parseStyle(cell.getAttribute('style') ?? '');
    const end = s.get('endArrow') ?? 'classic';
    const start = s.get('startArrow') ?? 'none';
    const filled = (key: string, kind: string | undefined) =>
      kind && s.get(key) === '0' && (kind === 'diamond' || kind === 'circle') ? `${kind}-open` : kind;
    const at = origin(cell);
    const waypoints = Array.from(cell.querySelectorAll(':scope > mxGeometry > Array[as="points"] > mxPoint')).map((p) => ({
      x: at.x + Number(p.getAttribute('x') ?? 0),
      y: at.y + Number(p.getAttribute('y') ?? 0),
    }));
    // A label on a line is either the cell's own value or a child cell.
    const childLabel = cells.find((c) => c.getAttribute('parent') === cell.getAttribute('id') && c.getAttribute('vertex') === '1');
    edges.push({
      id: cell.getAttribute('id') ?? '', source, target, updated: 0,
      type: s.get('edgeStyle')?.includes('orthogonal') || s.get('edgeStyle') === 'elbowEdgeStyle' ? 'step' : waypoints.length ? 'straight' : 'default',
      data: {
        color: color(s.get('strokeColor')),
        strokeWidth: s.get('strokeWidth') ? Math.min(5, Math.max(1, Math.round(Number(s.get('strokeWidth'))))) : undefined,
        dashStyle: s.get('dashed') === '1' ? 'dashed' : undefined,
        markerEnd: filled('endFill', DRAWIO_ENDS[end]) ?? 'none',
        markerStart: filled('startFill', DRAWIO_ENDS[start]) ?? 'none',
        label: plain(cell.getAttribute('value')) || plain(childLabel?.getAttribute('value') ?? null) || undefined,
        waypoints: waypoints.length ? waypoints : undefined,
      },
    });
    // The label cell, if any, is part of the line now, not a box of its own.
    if (childLabel) {
      const i = nodes.findIndex((n) => n.id === childLabel.getAttribute('id'));
      if (i >= 0) nodes.splice(i, 1);
    }
  }
  return nodes.length ? { nodes: nodes.map(clean), edges: edges.map(clean) } : null;
}

// ─── Mermaid ────────────────────────────────────────────────

/** Whether a piece of text is Mermaid source: its first word names a diagram type. */
export function looksLikeMermaid(text: string): boolean {
  const first = text.replace(/^\s*%%.*$/gm, '').replace(/^\s*---[\s\S]*?---\s*/, '').trim().split(/\s/)[0] ?? '';
  return /^(graph|flowchart|sequenceDiagram|classDiagram|classDiagram-v2|stateDiagram|stateDiagram-v2|erDiagram|journey|gantt|pie|mindmap|timeline|block-beta|architecture-beta|C4Context)$/.test(first);
}
