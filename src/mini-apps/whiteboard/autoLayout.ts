import { stampElement } from './boardFile';
import type { WBEdge, WBNode } from './boardFile';
import {
  applyRoute,
  drawnPath,
  labelSize,
  outlineOf,
  placeLabels,
  reachOf,
  routeOf,
  runsThrough,
  type LabelToPlace,
  type Outline,
  type Point,
  type Route,
} from './autoRoute';
import type { Side } from './routing';
import type { ElkNode } from 'elkjs/lib/elk-api';

/**
 * A board Syn drew, laid out again by ELK's layered layout when it is first
 * opened: where every box goes, and the lines between them.
 *
 * # Which boards
 *
 * Only those Syn marks (`left_to_elk`, board.rs): a drawing whose shape says
 * nothing — no sites, no lanes, no order in time, nobody saying which row. A
 * flowchart, a data flow, tables and what joins them. There ELK draws fewer
 * crossings and bends than Syn's own layers, and joins a table's line to the
 * row it names; a layout that means something (two data centres side by
 * side, lanes, a sequence) it did not keep, and is not asked to
 * (docs/whiteboard-layout-elk-2026-11-13.md).
 *
 * The mark is on every item, `data.layout = 'pending'`, and comes off them
 * all once the board is laid out — or found to be one this should not move:
 * an edit put a frame on it, or something that is not a box or a table.
 *
 * # What it writes
 *
 * The boxes' places, and each line's route in the same form the router
 * writes (`applyRoute`, autoRoute.ts), so a line ELK drew follows its boxes
 * when they are dragged like any routed line. A route of ELK's that runs
 * through a box (`runsThrough`) is not used: the line is left for the router.
 *
 * Loaded when first needed (about 470 KB compressed, its own chunk), and run
 * on the main thread: once for a board, in well under a tenth of a second.
 */

/** Whether an item is waiting to be laid out. */
export const waitsForLayout = (n: WBNode): boolean => n.data?.layout === 'pending';

/** Room between ranks, between boxes in a rank, and around lines. */
const BETWEEN_RANKS = 56;
/** Between tables, which have line ends on both sides to draw. */
const BETWEEN_TABLES = 96;
const BETWEEN_BOXES = 40;
const LINE_TO_BOX = 20;
const LINE_TO_LINE = 12;

/** What on the board ELK places: a box, or a table with its rows. */
interface Piece {
  node: WBNode;
  /** A table's rows, which go where it goes. */
  rows: WBNode[];
}

/** What a board is to ELK, or null for one it should leave as it is. */
interface Plan {
  pieces: Piece[];
  /** The table each row is in, by the row's id. */
  tableOf: Map<string, Piece>;
}

const sizeOf = (n: WBNode) => ({ width: Number(n.data?.width) || 0, height: Number(n.data?.height) || 0 });

const inside = (outer: WBNode, inner: WBNode) => {
  const [o, i] = [sizeOf(outer), sizeOf(inner)];
  return (
    inner.position.x >= outer.position.x - 0.5 &&
    inner.position.y >= outer.position.y - 0.5 &&
    inner.position.x + i.width <= outer.position.x + o.width + 0.5 &&
    inner.position.y + i.height <= outer.position.y + o.height + 0.5
  );
};

/**
 * The board as boxes and tables, in the order Syn ranked them — top to
 * bottom, then left to right — which ELK keeps where it can (model order):
 * a flowchart's loop back to an earlier step stays a loop back, rather than
 * the step it goes to being moved below it.
 */
function planOf(nodes: WBNode[]): Plan | null {
  const pieces: Piece[] = [];
  const tableOf = new Map<string, Piece>();
  const frames = nodes.filter((n) => n.type === 'frame');
  for (const n of nodes) {
    const { width, height } = sizeOf(n);
    if (!width || !height) return null;
    if (n.type !== 'shape' && n.type !== 'frame') return null;
  }
  for (const frame of frames) {
    const held = nodes.filter((n) => n !== frame && inside(frame, n));
    // A frame of anything but rows is a zone somebody put boxes in.
    if (!held.length || !held.every((n) => n.type === 'shape' && n.data?.field)) return null;
    const piece = { node: frame, rows: held };
    pieces.push(piece);
    for (const row of held) tableOf.set(row.id, piece);
  }
  for (const n of nodes) {
    if (n.type !== 'shape') continue;
    if (n.data?.field) {
      if (!tableOf.has(n.id)) return null;
      continue;
    }
    pieces.push({ node: n, rows: [] });
  }
  pieces.sort((a, b) => a.node.position.y - b.node.position.y || a.node.position.x - b.node.position.x);
  return { pieces, tableOf };
}

/** Where a row's line leaves its table, or comes in: its right side, or its left. */
const portOf = (row: string, out: boolean) => `${row}:${out ? 'E' : 'W'}`;

const SIDES: Side[] = ['top', 'bottom', 'left', 'right'];
const ELK_SIDE: Record<Side, string> = { top: 'NORTH', bottom: 'SOUTH', left: 'WEST', right: 'EAST' };

/**
 * Whether a shape is met only at the middle of some side: a diamond, an
 * ellipse (`reachOf`). Left to itself ELK spreads a box's line ends along
 * the side; moved to the corner of a diamond after, the three lines of a
 * question came together on its bottom corner and ran down one another.
 */
const pointed = (n: WBNode) => {
  const reach = n.type === 'shape' ? reachOf(n.data?.shapeType) : undefined;
  return !!reach && SIDES.some((side) => !reach[side]);
};

const middle = (n: WBNode) => ({ x: n.position.x + sizeOf(n).width / 2, y: n.position.y + sizeOf(n).height / 2 });

/**
 * Which corner of a pointed shape each of its lines leaves from or comes in
 * at, by `${line id}>` going out and `${line id}<` coming in.
 *
 * From where the other ends were as Syn drew it, top to bottom, the way a
 * flowchart's question is drawn: in at the top; out at the bottom to what is
 * under it, and to the side to what is off to that side. One answer only
 * leaves from the bottom — two that did ran down one another — and the
 * others go out of the sides, the one further over first. A line coming back
 * up comes in at the side it is on, unless an answer leaves there; then at
 * the top, with the rest.
 */
function cornersOf(n: WBNode, lines: WBEdge[], byId: Map<string, WBNode>): Map<string, Side> {
  const corners = new Map<string, Side>();
  const at = middle(n);
  const toward = (e: WBEdge, out: boolean) => {
    const other = byId.get(out ? e.target : e.source);
    const b = other ? middle(other) : at;
    return { e, dx: b.x - at.x, dy: b.y - at.y };
  };
  const aside = (dx: number): Side => (dx < 0 ? 'left' : 'right');
  const outs = lines.filter((e) => e.source === n.id).map((e) => toward(e, true));
  const under = outs
    .filter(({ dx, dy }) => dy >= 1 && Math.abs(dx) < sizeOf(n).width / 2)
    .sort((a, b) => Math.abs(a.dx) - Math.abs(b.dx));
  const used = new Set<Side>();
  const take = (key: string, side: Side) => {
    corners.set(key, side);
    used.add(side);
  };
  if (under.length) take(`${under[0].e.id}>`, 'bottom');
  // The rest, the furthest over first, each to its own side while one is free.
  const rest = outs.filter((o) => !corners.has(`${o.e.id}>`)).sort((a, b) => Math.abs(b.dx) - Math.abs(a.dx));
  for (const { e, dx } of rest) {
    const side = aside(dx);
    const other: Side = side === 'left' ? 'right' : 'left';
    take(`${e.id}>`, !used.has(side) ? side : !used.has(other) ? other : side);
  }
  for (const { e, dx, dy } of lines.filter((l) => l.target === n.id).map((l) => toward(l, false))) {
    const side = aside(dx);
    corners.set(`${e.id}<`, dy <= -1 || used.has(side) ? 'top' : side);
  }
  return corners;
}

/** How far along a path a point is, as a share of its length: where on it is nearest. */
export function shareAt(path: Point[], p: Point): number {
  let total = 0;
  let best = { d: Infinity, at: 0 };
  for (let i = 1; i < path.length; i++) {
    const [a, b] = [path[i - 1], path[i]];
    const len = Math.hypot(b.x - a.x, b.y - a.y);
    const k = len ? Math.min(1, Math.max(0, ((p.x - a.x) * (b.x - a.x) + (p.y - a.y) * (b.y - a.y)) / (len * len))) : 0;
    const d = Math.hypot(a.x + (b.x - a.x) * k - p.x, a.y + (b.y - a.y) * k - p.y);
    if (d < best.d) best = { d, at: total + len * k };
    total += len;
  }
  return total ? Math.round((best.at / total) * 1000) / 1000 : 0.5;
}

/**
 * Lay out the board, in place, if its items are waiting for it. Returns
 * whether anything changed.
 *
 * Throws when ELK cannot be loaded or fails, the marks left on: the board is
 * as Syn drew it, which is a fair drawing, and is laid out when it is next
 * opened somewhere ELK runs.
 */
export async function layOutPending(board: { nodes: WBNode[]; edges: WBEdge[] }): Promise<boolean> {
  if (!board.nodes.some(waitsForLayout)) return false;
  const plan = planOf(board.nodes);
  if (plan) await layOut(board, plan);
  for (const n of board.nodes) {
    if (!waitsForLayout(n)) continue;
    const data = { ...n.data };
    delete data.layout;
    n.data = data;
    stampElement(n);
  }
  return true;
}

async function layOut(board: { nodes: WBNode[]; edges: WBEdge[] }, plan: Plan) {
  const { default: ELK } = await import('elkjs/lib/elk.bundled.js');
  const { pieces, tableOf } = plan;
  const byId = new Map(board.nodes.map((n) => [n.id, n]));
  const placed = new Set(pieces.map((p) => p.node.id));
  const tables = pieces.some((p) => p.rows.length);

  const children = pieces.map(({ node, rows }) => {
    const { width, height } = sizeOf(node);
    if (pointed(node)) {
      const at: Record<Side, [number, number]> = { top: [width / 2, 0], bottom: [width / 2, height], left: [0, height / 2], right: [width, height / 2] };
      return {
        id: node.id,
        width,
        height,
        layoutOptions: { 'elk.portConstraints': 'FIXED_POS' },
        ports: SIDES.map((side) => ({
          id: `${node.id}:${side}`,
          width: 0,
          height: 0,
          x: at[side][0],
          y: at[side][1],
          layoutOptions: { 'elk.port.side': ELK_SIDE[side] },
        })),
      };
    }
    if (!rows.length) return { id: node.id, width, height };
    // A port on each side of each row, at its middle, where the app draws
    // a row's line end.
    return {
      id: node.id,
      width,
      height,
      layoutOptions: { 'elk.portConstraints': 'FIXED_POS' },
      ports: rows.flatMap((row) => {
        const y = row.position.y - node.position.y + sizeOf(row).height / 2;
        return [
          { id: portOf(row.id, false), width: 0, height: 0, x: 0, y, layoutOptions: { 'elk.port.side': 'WEST' } },
          { id: portOf(row.id, true), width: 0, height: 0, x: width, y, layoutOptions: { 'elk.port.side': 'EAST' } },
        ];
      }),
    };
  });

  // A line between two things ELK places: from a row's port if it is a
  // row's, a corner if it is a pointed shape's.
  const known = (id: string) => tableOf.has(id) || placed.has(id);
  const lines = board.edges.filter((e) => e.source !== e.target && known(e.source) && known(e.target));
  const corners = new Map<string, Side>();
  for (const { node } of pieces) {
    if (!pointed(node)) continue;
    const own = lines.filter((e) => e.source === node.id || e.target === node.id);
    for (const [key, side] of cornersOf(node, own, byId)) corners.set(key, side);
  }
  const end = (e: WBEdge, out: boolean) => {
    const id = out ? e.source : e.target;
    if (tableOf.has(id)) return portOf(id, out);
    const corner = corners.get(`${e.id}${out ? '>' : '<'}`);
    return corner ? `${id}:${corner}` : id;
  };
  const edges = lines.map((e) => {
    const label = typeof e.data?.label === 'string' ? e.data.label.trim() : '';
    return {
      id: e.id,
      sources: [end(e, true)],
      targets: [end(e, false)],
      labels: label ? [{ text: label, ...labelSize(label) }] : [],
    };
  });

  const elk = new ELK();
  const graph: ElkNode = {
    id: 'board',
    layoutOptions: {
      'elk.algorithm': 'layered',
      // Tables read across, as a schema is drawn, lines leaving a row on its
      // right and coming in on the left; everything else down the page.
      'elk.direction': tables ? 'RIGHT' : 'DOWN',
      'elk.edgeRouting': 'ORTHOGONAL',
      'elk.layered.spacing.nodeNodeBetweenLayers': String(tables ? BETWEEN_TABLES : BETWEEN_RANKS),
      'elk.spacing.nodeNode': String(BETWEEN_BOXES),
      'elk.spacing.edgeNode': String(LINE_TO_BOX),
      'elk.spacing.edgeEdge': String(LINE_TO_LINE),
      'elk.layered.spacing.edgeNodeBetweenLayers': String(LINE_TO_BOX),
      'elk.layered.considerModelOrder.strategy': 'NODES_AND_EDGES',
      'elk.layered.cycleBreaking.strategy': 'MODEL_ORDER',
      'elk.layered.nodePlacement.strategy': 'BRANDES_KOEPF',
      'elk.layered.nodePlacement.bk.fixedAlignment': 'BALANCED',
      // Beside the line, which keeps room between lines for their labels;
      // on it, ELK put a jog of four bends in a line to reach its label. The
      // app draws a label on its line, and `placeLabels` slides it clear.
      'elk.edgeLabels.placement': 'CENTER',
    },
    children,
    edges,
  };
  const out = await elk.layout(graph);

  // Where the board started stays where it starts: the same corner, so the
  // view it opens at still finds it.
  const left = Math.min(...pieces.map((p) => p.node.position.x));
  const top = Math.min(...pieces.map((p) => p.node.position.y));
  const laid = out.children ?? [];
  const dx = left - Math.min(...laid.map((c) => c.x ?? 0));
  const dy = top - Math.min(...laid.map((c) => c.y ?? 0));
  const at = (p: { x: number; y: number }): Point => ({ x: p.x + dx, y: p.y + dy });

  for (const c of laid) {
    const piece = pieces.find((p) => p.node.id === c.id);
    if (!piece) continue;
    const to = at({ x: c.x ?? 0, y: c.y ?? 0 });
    const [mx, my] = [to.x - piece.node.position.x, to.y - piece.node.position.y];
    for (const n of [piece.node, ...piece.rows]) {
      n.position = { x: tidy(n.position.x + mx), y: tidy(n.position.y + my) };
      stampElement(n);
    }
  }

  // The lines, as routes from the boxes where they are now.
  const boxes = new Map<string, Outline>();
  for (const n of board.nodes) {
    if (n.type === 'frame') continue;
    const box = outlineOf(n);
    if (box) boxes.set(n.id, box);
  }
  const drawn = new Map((out.edges ?? []).map((e) => [e.id, e]));
  const routed: [WBEdge, Route][] = [];
  const labels: LabelToPlace[] = [];
  for (const line of lines) {
    const e = drawn.get(line.id);
    const section = e?.sections?.[0];
    const [from, to] = [boxes.get(line.source), boxes.get(line.target)];
    if (!section || !from || !to || !byId.has(line.source)) continue;
    const path = [section.startPoint, ...(section.bendPoints ?? []), section.endPoint].map(at);
    // A row's port is on its table's side, out past the row's own: the
    // line ends on the row.
    if (tableOf.has(line.source)) path[0] = { x: from.x + from.width, y: path[0].y };
    if (tableOf.has(line.target)) path[path.length - 1] = { x: to.x, y: path[path.length - 1].y };
    const route = routeOf(path, from, to);
    if (!route || runsThrough(drawnPath(route, from, to), boxes, line)) {
      // Left for the router, from the boxes where they are now.
      line.data = { ...(line.data ?? {}), routing: 'pending' };
      for (const key of ['waypoints', 'sourceAt', 'targetAt', 'labelAt', 'routed']) delete line.data[key];
      line.type = 'step';
      stampElement(line);
      continue;
    }
    const label = e?.labels?.[0];
    if (label?.x !== undefined && label.y !== undefined) {
      const middle = at({ x: label.x + (label.width ?? 0) / 2, y: label.y + (label.height ?? 0) / 2 });
      route.labelAt = shareAt(drawnPath(route, from, to), middle);
      labels.push({ id: line.id, label: label.text ?? '', path: drawnPath(route, from, to), share: route.labelAt });
    }
    routed.push([line, route]);
  }
  // ELK leaves a label room beside its line; the app draws it on the line.
  const shares = placeLabels(labels, boxes.values());
  for (const [line, route] of routed) {
    applyRoute(line, { ...route, labelAt: shares.get(line.id) ?? route.labelAt });
  }
}

/** Coordinates as written to the file: to the tenth of a pixel. */
const tidy = (n: number) => Math.round(n * 10) / 10;
