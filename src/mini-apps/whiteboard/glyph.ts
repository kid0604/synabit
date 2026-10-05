/**
 * An icon on the board: a shape whose outline is a small drawing — a server,
 * a user, a cloud — rather than a box to write in.
 *
 * The drawing is kept in the board file itself (`data.glyph` on a `shape`
 * item whose `shapeType` is `glyph`), not referred to by name. A board then
 * shows the same picture wherever it is opened — in a note's preview, on a
 * shared page, in an export, on a machine with another version of the app —
 * and an icon set dropping or redrawing an icon cannot change a board that
 * used it. An icon is a few hundred bytes.
 *
 * Only plain drawing elements and plain attributes are kept, and they are
 * drawn as elements, never as markup, so a board from somewhere else cannot
 * carry anything that runs.
 */

export type GlyphTag = 'path' | 'circle' | 'ellipse' | 'rect' | 'line' | 'polyline' | 'polygon';
export type GlyphPart = [GlyphTag, Record<string, string | number>];

export interface Glyph {
  /** Where it came from, and its name there: `lucide`, `server`. */
  set: string;
  name: string;
  /** `minX minY width height` of the drawing. */
  viewBox: [number, number, number, number];
  parts: GlyphPart[];
}

export const GLYPH_SHAPE = 'glyph';

const TAGS = new Set<GlyphTag>(['path', 'circle', 'ellipse', 'rect', 'line', 'polyline', 'polygon']);
const ATTRS = new Set(['d', 'cx', 'cy', 'r', 'rx', 'ry', 'x', 'y', 'width', 'height', 'x1', 'y1', 'x2', 'y2', 'points']);
/** What a geometry attribute may hold: numbers, path commands, separators. */
const GEOMETRY = /^[\d\s.,+\-eEMmLlHhVvCcSsQqTtAaZz]*$/;

/** A glyph as found in a file, made safe to draw, or null if it is not one. */
export function cleanGlyph(raw: unknown): Glyph | null {
  const g = raw as Partial<Glyph> | null;
  if (!g || typeof g !== 'object' || !Array.isArray(g.parts)) return null;
  const box = Array.isArray(g.viewBox) && g.viewBox.length === 4 && g.viewBox.every((n) => Number.isFinite(Number(n)))
    ? (g.viewBox.map(Number) as Glyph['viewBox'])
    : [0, 0, 24, 24] as Glyph['viewBox'];
  if (box[2] <= 0 || box[3] <= 0) return null;
  const parts: GlyphPart[] = [];
  for (const part of g.parts.slice(0, 200)) {
    if (!Array.isArray(part) || !TAGS.has(part[0] as GlyphTag) || !part[1] || typeof part[1] !== 'object') continue;
    const attrs: Record<string, string | number> = {};
    for (const [k, v] of Object.entries(part[1] as Record<string, unknown>)) {
      if (!ATTRS.has(k)) continue;
      if (typeof v === 'number' && Number.isFinite(v)) attrs[k] = v;
      else if (typeof v === 'string' && v.length <= 8000 && GEOMETRY.test(v)) attrs[k] = v;
    }
    if (Object.keys(attrs).length) parts.push([part[0] as GlyphTag, attrs]);
  }
  if (!parts.length) return null;
  return {
    set: typeof g.set === 'string' ? g.set.slice(0, 40) : 'unknown',
    name: typeof g.name === 'string' ? g.name.slice(0, 80) : '',
    viewBox: box,
    parts,
  };
}

const escapeAttr = (v: string | number) => String(v).replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;');

/**
 * The glyph as an SVG document, `width`×`height`, drawn in `color` — for the
 * places that need a file rather than elements: exports to other tools, and
 * pictures of the board.
 */
export function glyphSvg(glyph: Glyph, color: string, width: number, height: number, strokeWidth = 2): string {
  const inner = glyph.parts
    .map(([tag, attrs]) => `<${tag} ${Object.entries(attrs).map(([k, v]) => `${k}="${escapeAttr(v)}"`).join(' ')}/>`)
    .join('');
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${Math.round(width)}" height="${Math.round(height)}" viewBox="${glyph.viewBox.join(' ')}" fill="none" stroke="${escapeAttr(color)}" stroke-width="${strokeWidth}" stroke-linecap="round" stroke-linejoin="round">${inner}</svg>`;
}

/** The square an icon is put down as. */
export const GLYPH_SIZE = 64;
