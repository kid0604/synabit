/**
 * A drawn Mermaid diagram, as a board somebody can move things on.
 *
 * # Why this exists
 *
 * Mermaid decides where everything goes, and there is no way to tell it
 * otherwise — no coordinates, no "put the two cores side by side", nothing.
 * For a system drawing that is the whole job: two data centres that ought to
 * mirror each other come out lopsided, and the person who knows what the
 * picture means cannot fix it.
 *
 * The Whiteboard already stores a position per item, which is exactly the
 * thing Mermaid will not give. And the SVG Mermaid just produced *has* the
 * positions in it — every node carries a `transform`, every cluster a `rect`,
 * every edge its own waypoints. So the layout engine does the first draft and
 * the person does the rest.
 *
 * Nothing here reads the diagram's source text. The picture on screen is what
 * gets converted, because that is what the person is looking at and wants to
 * rearrange.
 */
import type { WBNode, WBEdge } from '../mini-apps/whiteboard/boardFile';

/** What a converted diagram is, before it becomes a file. */
export interface BoardDraft {
  nodes: WBNode[];
  edges: WBEdge[];
}

/** Where the board sits relative to the diagram's own origin. */
const MARGIN = 60;

/** The colour a converted box is drawn in, matching the app's own violet. */
const NODE_COLOUR = '#7c3aed';
/** And the quieter one for a group's frame, which is background, not content. */
const GROUP_COLOUR = '#94a3b8';

/** `translate(12, 34)` → `[12, 34]`, and `null` for anything else. */
const translationOf = (el: Element | null): [number, number] | null => {
  const m = /translate\(\s*([-\d.]+)[ ,]+([-\d.]+)/.exec(el?.getAttribute('transform') ?? '');
  return m ? [parseFloat(m[1]), parseFloat(m[2])] : null;
};

const numberAttr = (el: Element | null, name: string): number =>
  parseFloat(el?.getAttribute(name) ?? '0') || 0;

/**
 * The words in a label, as one line.
 *
 * Mermaid puts labels in a `foreignObject` as HTML, so the text is in `<p>`
 * elements — one per line of a multi-line label, which is why they are joined
 * rather than picked from.
 */
const labelOf = (el: Element | null): string => {
  if (!el) return '';
  const lines = Array.from(el.querySelectorAll('p')).map(p => p.textContent?.trim() ?? '');
  const text = lines.filter(Boolean).join(' ');
  return text || (el.textContent ?? '').trim();
};

/**
 * The diagram's own name for a node, out of the id Mermaid gave the element.
 *
 * `probe-svg-flowchart-K1-0` → `K1`. The prefix is the SVG's id and the suffix
 * is a counter, and what is left in the middle is the name written in the
 * diagram — which is what the edges refer to.
 */
const keyOfNode = (id: string, svgId: string): string =>
  id
    .replace(new RegExp(`^${svgId}-`), '')
    .replace(/^flowchart-/, '')
    .replace(/-\d+$/, '');

/**
 * Which two nodes an edge joins, from `L_K1_F1_0`.
 *
 * Split on the longest name that is actually a node, rather than on the first
 * underscore: `SW_Core` is a perfectly good name in a diagram and would
 * otherwise be read as a node called `SW`.
 */
const endsOf = (edgeId: string, keys: Set<string>): [string, string] | null => {
  const body = edgeId.replace(/^L_/, '').replace(/_\d+$/, '');
  const candidates = Array.from(keys).sort((a, b) => b.length - a.length);
  for (const source of candidates) {
    if (!body.startsWith(`${source}_`)) continue;
    const target = body.slice(source.length + 1);
    if (keys.has(target)) return [source, target];
  }
  return null;
};

/**
 * Which side of each box the line should leave from and arrive at.
 *
 * Decided from where the boxes ended up, because that is what the reader is
 * looking at: a line to something below leaves the bottom. Mermaid's own
 * waypoints are not used — a hand-arranged board draws its own routes, and
 * keeping the old ones would mean lines that ignore where their boxes went.
 */
const sidesBetween = (
  from: { x: number; y: number; w: number; h: number },
  to: { x: number; y: number; w: number; h: number },
): [string, string] => {
  const dx = to.x + to.w / 2 - (from.x + from.w / 2);
  const dy = to.y + to.h / 2 - (from.y + from.h / 2);
  if (Math.abs(dx) > Math.abs(dy)) {
    return dx >= 0 ? ['right', 'left'] : ['left', 'right'];
  }
  return dy >= 0 ? ['bottom', 'top'] : ['top', 'bottom'];
};

/**
 * Convert a rendered diagram into board items.
 *
 * Groups come first in the list so they sit behind the boxes they contain —
 * a subgraph is a frame around its contents, and a frame drawn last would
 * hide them.
 */
export function boardFromDiagram(svg: string, idPrefix = 'd'): BoardDraft {
  const doc = new DOMParser().parseFromString(svg, 'image/svg+xml');
  const root = doc.querySelector('svg');
  if (!root) return { nodes: [], edges: [] };
  const svgId = root.getAttribute('id') ?? '';

  const nodes: WBNode[] = [];
  const edges: WBEdge[] = [];
  /** Where each diagram node ended up, so the edges can pick a side. */
  const boxes = new Map<string, { id: string; x: number; y: number; w: number; h: number }>();
  const now = Date.now();
  let n = 0;
  const nextId = (kind: string) => `${idPrefix}-${kind}-${now.toString(36)}-${n++}`;

  // ── The subgraphs, as frames ──────────────────────────────
  for (const cluster of Array.from(root.querySelectorAll('g.cluster'))) {
    const rect = cluster.querySelector('rect');
    if (!rect) continue;
    nodes.push({
      id: nextId('group'),
      type: 'shape',
      position: {
        x: numberAttr(rect, 'x') + MARGIN,
        y: numberAttr(rect, 'y') + MARGIN,
      },
      data: {
        shapeType: 'rectangle',
        label: labelOf(cluster.querySelector('.cluster-label')),
        color: GROUP_COLOUR,
        width: numberAttr(rect, 'width'),
        height: numberAttr(rect, 'height'),
      },
      updated: now,
    });
  }

  // ── The boxes ─────────────────────────────────────────────
  for (const node of Array.from(root.querySelectorAll('g.node'))) {
    const at = translationOf(node);
    const rect = node.querySelector('rect');
    if (!at || !rect) continue;

    // Mermaid puts a node's centre in the transform and its box around that
    // centre; a board places the top-left corner.
    const w = numberAttr(rect, 'width');
    const h = numberAttr(rect, 'height');
    const x = at[0] - w / 2 + MARGIN;
    const y = at[1] - h / 2 + MARGIN;

    const id = nextId('shape');
    boxes.set(keyOfNode(node.getAttribute('id') ?? '', svgId), { id, x, y, w, h });
    nodes.push({
      id,
      type: 'shape',
      position: { x, y },
      data: {
        shapeType: 'rectangle',
        label: labelOf(node.querySelector('.label')),
        color: NODE_COLOUR,
        width: w,
        height: h,
      },
      updated: now,
    });
  }

  // ── The lines between them ────────────────────────────────
  const labels = new Map<string, string>();
  for (const label of Array.from(root.querySelectorAll('g.edgeLabel .label'))) {
    const forEdge = label.getAttribute('data-id');
    const words = labelOf(label);
    if (forEdge && words) labels.set(forEdge, words);
  }

  for (const path of Array.from(root.querySelectorAll('path[data-id]'))) {
    const edgeId = path.getAttribute('data-id') ?? '';
    const ends = endsOf(edgeId, new Set(boxes.keys()));
    if (!ends) continue;
    const from = boxes.get(ends[0]);
    const to = boxes.get(ends[1]);
    if (!from || !to) continue;

    const [sourceHandle, targetHandle] = sidesBetween(from, to);
    edges.push({
      id: nextId('edge'),
      source: from.id,
      sourceHandle,
      target: to.id,
      targetHandle,
      type: 'default',
      data: labels.has(edgeId) ? { label: labels.get(edgeId) } : {},
      updated: now,
    });
  }

  return { nodes, edges };
}
