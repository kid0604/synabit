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

/** `translate(12, 34)` → `[12, 34]`; `translate(12)` → `[12, 0]`. */
const translationOf = (el: Element | null): [number, number] => {
  const m = /translate\(\s*([-\d.]+)(?:[ ,]+([-\d.]+))?/.exec(el?.getAttribute('transform') ?? '');
  return m ? [parseFloat(m[1]), parseFloat(m[2] ?? '0') || 0] : [0, 0];
};

/**
 * Where an element really sits, transforms of its parents included.
 *
 * # Why this is not just the element's own transform
 *
 * Because a subgraph is a `<g>` with a transform of its own, and everything
 * inside it is placed **relative to that**. Reading only the node's own
 * transform put every box of a subgraph within a few pixels of the subgraph's
 * corner: on a real drawing of two data centres, forty-four boxes came out in
 * three overlapping heaps.
 */
const placeOf = (el: Element): [number, number] => {
  let x = 0;
  let y = 0;
  for (let at: Element | null = el; at && at.tagName !== 'svg'; at = at.parentElement) {
    const [dx, dy] = translationOf(at);
    x += dx;
    y += dy;
  }
  return [x, y];
};

const numberAttr = (el: Element | null, name: string): number =>
  parseFloat(el?.getAttribute(name) ?? '0') || 0;

/** A box in the diagram's own coordinates. */
interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
  shape: string;
}

/** The numbers in `points="9.75,0 144,0 …"`, as a box. */
const polygonBox = (points: string): { x: number; y: number; w: number; h: number } | null => {
  const n = points.trim().split(/[\s,]+/).map(Number).filter(v => !Number.isNaN(v));
  if (n.length < 6) return null;
  const xs = n.filter((_, i) => i % 2 === 0);
  const ys = n.filter((_, i) => i % 2 === 1);
  const x = Math.min(...xs);
  const y = Math.min(...ys);
  return { x, y, w: Math.max(...xs) - x, h: Math.max(...ys) - y };
};

/**
 * The shape a node is drawn as, and how big it is.
 *
 * Mermaid draws its shapes five different ways and only one of them is a
 * `<rect>`: a decision is a `<polygon>`, a database is a `<path>` of arcs, a
 * round node is a `<circle>`. Reading rectangles alone meant a diagram came
 * across with its databases and its junctions missing — which on a network
 * drawing is most of the interesting boxes.
 *
 * Every one of them is drawn centred on the node, which is what makes the
 * arc-covered `<path>` measurable without parsing arcs: its own `translate`
 * is the corner it starts from, so twice that is its size.
 */
const shapeOf = (node: Element): Box | null => {
  const kid = node.querySelector('rect, polygon, circle, ellipse, path');
  if (!kid) return null;
  const [ox, oy] = translationOf(kid);
  const num = (name: string) => parseFloat(kid.getAttribute(name) ?? '0') || 0;

  switch (kid.tagName.toLowerCase()) {
    case 'rect': {
      const w = num('width');
      const h = num('height');
      if (!w || !h) return null;
      // A corner radius is Mermaid's rounded node, and the board has one too.
      const shape = num('rx') > 2 ? 'roundedRect' : 'rectangle';
      return { x: num('x') + ox, y: num('y') + oy, w, h, shape };
    }
    case 'circle': {
      const r = num('r');
      return r ? { x: num('cx') - r + ox, y: num('cy') - r + oy, w: r * 2, h: r * 2, shape: 'ellipse' } : null;
    }
    case 'ellipse': {
      const rx = num('rx');
      const ry = num('ry');
      return rx && ry
        ? { x: num('cx') - rx + ox, y: num('cy') - ry + oy, w: rx * 2, h: ry * 2, shape: 'ellipse' }
        : null;
    }
    case 'polygon': {
      const box = polygonBox(kid.getAttribute('points') ?? '');
      if (!box) return null;
      const corners = (kid.getAttribute('points') ?? '').trim().split(/\s+/).length;
      return {
        x: box.x + ox,
        y: box.y + oy,
        w: box.w,
        h: box.h,
        shape: corners === 4 ? 'diamond' : 'hexagon',
      };
    }
    default: {
      // The arc-drawn shapes — a database is the one that matters. Centred on
      // the node, so the corner it is translated to says how big it is.
      if (ox >= 0 || oy >= 0) return null;
      return { x: ox, y: oy, w: -ox * 2, h: -oy * 2, shape: 'cylinder' };
    }
  }
};

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
  // Parsed as HTML, not as XML.
  //
  // The picture is full of `foreignObject` labels holding ordinary HTML —
  // `<br>`, `&nbsp;`, whatever somebody wrote in a node — and one of those is
  // enough for a strict XML parse to stop where it stands. It did: a real
  // drawing of forty-four boxes parsed as eleven, silently, because the parser
  // gave back the part it had managed before the first unescaped thing. HTML
  // parsing is what the browser does with this markup anyway — the conversation
  // puts the same string into the page with `innerHTML`.
  const doc = new DOMParser().parseFromString(svg, 'text/html');
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
    // A subgraph inside a subgraph is placed relative to the one that holds
    // it, the same as everything else. See `placeOf`.
    const [gx, gy] = placeOf(cluster);
    const id = nextId('group');
    // A subgraph is a thing a line can be drawn to, so it goes in the lookup
    // beside the boxes. Fourteen of the real drawing's forty-six lines ended
    // on a subgraph rather than on a box, and without this they were dropped.
    boxes.set((cluster.getAttribute('id') ?? '').replace(new RegExp(`^${svgId}-`), ''), {
      id,
      x: gx + numberAttr(rect, 'x') + MARGIN,
      y: gy + numberAttr(rect, 'y') + MARGIN,
      w: numberAttr(rect, 'width'),
      h: numberAttr(rect, 'height'),
    });
    nodes.push({
      id,
      type: 'shape',
      position: {
        x: gx + numberAttr(rect, 'x') + MARGIN,
        y: gy + numberAttr(rect, 'y') + MARGIN,
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
    const shape = shapeOf(node);
    if (!shape) continue;

    // Mermaid places a node by its centre and draws the shape around that;
    // a board places the top-left corner.
    const [cx, cy] = placeOf(node);
    const x = cx + shape.x + MARGIN;
    const y = cy + shape.y + MARGIN;

    const id = nextId('shape');
    boxes.set(keyOfNode(node.getAttribute('id') ?? '', svgId), { id, x, y, w: shape.w, h: shape.h });
    nodes.push({
      id,
      type: 'shape',
      position: { x, y },
      data: {
        shapeType: shape.shape,
        label: labelOf(node.querySelector('.label')),
        color: NODE_COLOUR,
        width: shape.w,
        height: shape.h,
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
