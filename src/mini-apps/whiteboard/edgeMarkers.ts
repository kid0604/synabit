/**
 * The ends a line can have.
 *
 * An arrow says which way something goes. The rest are the notations people
 * draw by hand on a whiteboard and expect a diagram tool to know: UML's open
 * triangle (is a kind of) and diamonds (is made of), and the crow's feet of
 * an entity–relationship diagram (one, many, none or one, none or many). The
 * shape library has had UML and ER boxes for a while; without these ends the
 * lines between them could not say anything.
 *
 * Each is drawn in a 20×20 box with its tip at (20, 10), where the line meets
 * the item. `hollow` parts are filled with the board's background, so they
 * read as outlines over a line passing behind them.
 *
 * Kept as data and drawn by the template, never as markup: the colour comes
 * from the board file, which may have come from anywhere.
 */
/** One part of a marker: a path or a dot, and how it is painted. */
export interface MarkerPart {
  /** `solid`: filled with the line's colour. `hollow`: background inside, line colour around. `line`: strokes only. */
  paint: 'solid' | 'hollow' | 'line';
  d?: string;
  /** A dot's centre along the line, in the 20×20 box. */
  cx?: number;
}

export interface MarkerKind {
  id: string;
  labelKey: string;
  parts: MarkerPart[];
}

/** Crow's foot: three prongs meeting the item, joined a little way back. */
const CROW = 'M20,3 L10,10 L20,17 M10,10 L20,10';

export const MARKER_KINDS: MarkerKind[] = [
  { id: 'arrow', labelKey: 'whiteboard.marker.arrow', parts: [{ paint: 'solid', d: 'M4,4 L20,10 L4,16 Z' }] },
  { id: 'arrow-open', labelKey: 'whiteboard.marker.arrow_open', parts: [{ paint: 'line', d: 'M7,4 L20,10 L7,16' }] },
  { id: 'triangle-open', labelKey: 'whiteboard.marker.triangle_open', parts: [{ paint: 'hollow', d: 'M3,3 L20,10 L3,17 Z' }] },
  { id: 'diamond', labelKey: 'whiteboard.marker.diamond', parts: [{ paint: 'solid', d: 'M0,10 L10,4 L20,10 L10,16 Z' }] },
  { id: 'diamond-open', labelKey: 'whiteboard.marker.diamond_open', parts: [{ paint: 'hollow', d: 'M1,10 L10,4.5 L19,10 L10,15.5 Z' }] },
  { id: 'circle', labelKey: 'whiteboard.marker.circle', parts: [{ paint: 'solid', cx: 15 }] },
  { id: 'circle-open', labelKey: 'whiteboard.marker.circle_open', parts: [{ paint: 'hollow', cx: 15 }] },
  { id: 'er-one', labelKey: 'whiteboard.marker.er_one', parts: [{ paint: 'line', d: 'M12,3 L12,17 M16,3 L16,17' }] },
  { id: 'er-many', labelKey: 'whiteboard.marker.er_many', parts: [{ paint: 'line', d: CROW }] },
  { id: 'er-one-many', labelKey: 'whiteboard.marker.er_one_many', parts: [{ paint: 'line', d: `${CROW} M7,3 L7,17` }] },
  { id: 'er-zero-one', labelKey: 'whiteboard.marker.er_zero_one', parts: [{ paint: 'line', d: 'M16,3 L16,17' }, { paint: 'hollow', cx: 7 }] },
  { id: 'er-zero-many', labelKey: 'whiteboard.marker.er_zero_many', parts: [{ paint: 'line', d: CROW }, { paint: 'hollow', cx: 5 }] },
];

const KINDS = new Map(MARKER_KINDS.map((k) => [k.id, k]));

/** Whether a stored end is one this build draws. `none` and unknown ends draw nothing. */
export function isMarker(kind: string | undefined): kind is string {
  return !!kind && KINDS.has(kind);
}

export function markerKind(kind: string): MarkerKind | undefined {
  return KINDS.get(kind);
}

/**
 * The element id of a marker for a kind in a colour. Colours may be CSS
 * (`var(--wb-ink, #1e1e1e)`), so they are reduced to a short hash.
 */
export function markerId(kind: string, color: string): string {
  return markerIdFor(kind, color);
}

/** A line end as the `marker-start`/`marker-end` of the line it ends, or none. */
export function markerUrl(kind: string | undefined, color: string): string | undefined {
  return isMarker(kind) ? `url('#${markerIdFor(kind, color)}')` : undefined;
}

function markerIdFor(kind: string, color: string): string {
  let h = 0;
  for (let i = 0; i < color.length; i++) h = (h * 31 + color.charCodeAt(i)) | 0;
  return `wb-marker-${kind}-${(h >>> 0).toString(36)}`;
}
