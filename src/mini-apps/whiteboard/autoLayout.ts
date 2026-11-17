import { stampElement } from './boardFile';
import type { WBEdge, WBNode } from './boardFile';
import {
  applyRoute,
  captionOf,
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
import { drawingCost, untangle } from './untangle';

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
 * bottom, then left to right — which ELK keeps where it can (model order) in
 * its first try: a flowchart's loop back to an earlier step stays a loop
 * back, rather than the step it goes to being moved below it. The other
 * tries let go of it, and one of them is kept only for fewer crossings
 * (`TRIES`).
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

/**
 * Which side of a figure's drawing each of its lines ends on, keyed as in
 * `cornersOf`: never its bottom, where its words are. A line to something
 * above comes in at the top; the rest at the side the other end is on, and
 * those straight under it shared between the two sides, so a customer's
 * order going down and the answer coming back up are not one line.
 */
function sidesOf(n: WBNode, lines: WBEdge[], byId: Map<string, WBNode>): Map<string, Side> {
  const sides = new Map<string, Side>();
  const at = middle(n);
  const count = { left: 0, right: 0 };
  const ends = lines.flatMap((e) =>
    [true, false]
      .filter((out) => (out ? e.source : e.target) === n.id)
      .map((out) => {
        const other = byId.get(out ? e.target : e.source);
        const b = other ? middle(other) : at;
        return { key: `${e.id}${out ? '>' : '<'}`, dx: b.x - at.x, dy: b.y - at.y };
      }),
  );
  for (const { key, dx, dy } of ends) {
    if (dy <= -sizeOf(n).height / 2) {
      sides.set(key, 'top');
      continue;
    }
    const side: 'left' | 'right' =
      Math.abs(dx) >= sizeOf(n).width / 2 ? (dx < 0 ? 'left' : 'right') : count.left <= count.right ? 'left' : 'right';
    count[side]++;
    sides.set(key, side);
  }
  return sides;
}

/** A field's name: "order_id: uuid FK" → "order_id". */
const fieldName = (row: WBNode) => {
  const label = String(row.data?.label ?? '').trim().toLowerCase();
  const colon = label.indexOf(':');
  return (colon < 0 ? label.split(/\s+/)[0] : label.slice(0, colon)).trim();
};

/**
 * Which row of a table a line to the table as a whole ends on: the one named
 * as a row of the table at the other end — a key and the key that points at
 * it, "order_id" in orders and in order items — or else the first, its key.
 * Two tables joined to a third by their keys came into its first row, one on
 * top of the other.
 */
function joiningRow(rows: WBNode[], other: WBNode[]): WBNode {
  const there = new Set(other.map(fieldName));
  return rows.find((row) => there.has(fieldName(row))) ?? rows[0];
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
  if (plan) {
    // Each way of laying it out, on a copy, and the one with the fewest
    // crossings kept (see `TRIES`); the first of equals, which is Syn's order.
    let best: { cost: number; tune: Tune } | null = null;
    for (const tune of TRIES) {
      const copy = JSON.parse(JSON.stringify({ nodes: board.nodes, edges: board.edges })) as typeof board;
      await layOut(copy, planOf(copy.nodes)!, tune, false);
      const cost = costOf(copy);
      if (!best || cost < best.cost) best = { cost, tune };
    }
    await layOut(board, plan, best!.tune, true);
  }
  for (const n of board.nodes) {
    if (!waitsForLayout(n)) continue;
    const data = { ...n.data };
    delete data.layout;
    n.data = data;
    stampElement(n);
  }
  return true;
}

/** Options for ELK on top of the board's own: one way of laying it out. */
type Tune = Record<string, string>;

/**
 * The ways a board is laid out, of which the one with the fewest crossings
 * is kept.
 *
 * First as Syn ordered it (model order), then free of that order, then with
 * the cycles broken greedily from a few starting points. ELK's crossing
 * minimisation is a heuristic that starts from a random order, and on the
 * CMDB board Syn drew the same graph came out with anything from 10 to 23
 * crossings depending on where it started. Six layouts of a board of twenty
 * boxes take a few tenths of a second, once, when the board is first opened.
 */
const TRIES: Tune[] = [
  {},
  { 'elk.layered.considerModelOrder.strategy': 'NONE' },
  ...[1, 2, 3, 4].map((seed) => ({
    'elk.layered.considerModelOrder.strategy': 'NONE',
    'elk.layered.cycleBreaking.strategy': 'GREEDY',
    'elk.randomSeed': String(seed),
  })),
];

/** A line ELK could not route, left for the router: about what a crossing costs. */
const LEFT_FOR_THE_ROUTER = 800;

/** What a laid-out board costs to read: its lines' crossings first (`drawingCost`). */
function costOf(board: { nodes: WBNode[]; edges: WBEdge[] }): number {
  const boxes = new Map<string, Outline>();
  for (const n of board.nodes) {
    const box = outlineOf(n);
    if (box) boxes.set(n.id, box);
  }
  const paths: Point[][] = [];
  let left = 0;
  for (const e of board.edges) {
    const [a, b] = [boxes.get(e.source), boxes.get(e.target)];
    if (!a || !b || e.source === e.target) continue;
    const at = e.data as { waypoints?: Point[]; sourceAt?: Route['sourceAt']; targetAt?: Route['targetAt'] } | undefined;
    if (at?.sourceAt && at.targetAt) {
      paths.push(drawnPath({ waypoints: at.waypoints ?? [], sourceAt: at.sourceAt, targetAt: at.targetAt, labelAt: 0.5 }, a, b));
    } else {
      left++;
      paths.push([{ x: a.x + a.width / 2, y: a.y + a.height / 2 }, { x: b.x + b.width / 2, y: b.y + b.height / 2 }]);
    }
  }
  return drawingCost(paths) + LEFT_FOR_THE_ROUTER * left;
}

async function layOut(board: { nodes: WBNode[]; edges: WBEdge[] }, plan: Plan, tune: Tune, finish: boolean) {
  const { default: ELK } = await import('elkjs/lib/elk.bundled.js');
  const { pieces, tableOf } = plan;
  const byId = new Map(board.nodes.map((n) => [n.id, n]));
  const placed = new Set(pieces.map((p) => p.node.id));
  const tables = pieces.some((p) => p.rows.length);

  // A line between two things ELK places: from a row's port if it is a
  // row's, a corner if it is a pointed shape's, a side of the drawing if it
  // is a figure's.
  const known = (id: string) => tableOf.has(id) || placed.has(id);
  const lines = board.edges.filter((e) => e.source !== e.target && known(e.source) && known(e.target));
  const captions = new Map<string, Outline>();
  for (const { node, rows } of pieces) {
    const caption = rows.length ? null : captionOf(node);
    if (caption) captions.set(node.id, caption);
  }
  const corners = new Map<string, Side>();
  for (const { node } of pieces) {
    const own = lines.filter((e) => e.source === node.id || e.target === node.id);
    if (captions.has(node.id)) for (const [key, side] of sidesOf(node, own, byId)) corners.set(key, side);
    else if (pointed(node)) for (const [key, side] of cornersOf(node, own, byId)) corners.set(key, side);
  }

  /** How far into its piece a figure's drawing is: its words can be wider. */
  const inset = (id: string) => {
    const caption = captions.get(id);
    const node = byId.get(id);
    return caption && node ? Math.max(0, (caption.width - sizeOf(node).width) / 2) : 0;
  };

  const children = pieces.map(({ node, rows }) => {
    const { width, height } = sizeOf(node);
    const caption = captions.get(node.id);
    if (caption) {
      // The drawing and its words, as one: lines end on the drawing's top
      // or its sides, one place each, and none under it, where the words are.
      const ox = inset(node.id);
      const whole = Math.max(width, caption.width);
      const ends = lines.flatMap((e) =>
        [true, false]
          .filter((out) => (out ? e.source : e.target) === node.id)
          .map((out) => ({ key: `${e.id}${out ? '>' : '<'}`, side: corners.get(`${e.id}${out ? '>' : '<'}`)!, toward: middle(byId.get(out ? e.target : e.source) ?? node) })),
      );
      const ports = SIDES.flatMap((side) => {
        const along = ends
          .filter((end) => end.side === side)
          .sort((a, b) => (side === 'top' ? a.toward.x - b.toward.x : a.toward.y - b.toward.y));
        return along.map((end, i) => {
          const k = (i + 1) / (along.length + 1);
          const [x, y] = side === 'top' ? [ox + width * k, 0] : [side === 'left' ? 0 : whole, height * k];
          return { id: `${node.id}:${end.key}`, width: 0, height: 0, x, y, layoutOptions: { 'elk.port.side': ELK_SIDE[side] } };
        });
      });
      return { id: node.id, width: whole, height: height + caption.height, layoutOptions: { 'elk.portConstraints': 'FIXED_POS' }, ports };
    }
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

  // A line to a table as a whole ends on a row of it, on the side a row's
  // line would: left to itself ELK put both ends on the tables' tops.
  const rowsOf = new Map(
    pieces.filter((p) => p.rows.length).map((p) => [p.node.id, [...p.rows].sort((a, b) => a.position.y - b.position.y)]),
  );
  const end = (e: WBEdge, out: boolean) => {
    const id = out ? e.source : e.target;
    if (tableOf.has(id)) return portOf(id, out);
    const rows = rowsOf.get(id);
    if (rows) return portOf(joiningRow(rows, rowsOf.get(out ? e.target : e.source) ?? []).id, out);
    const key = `${e.id}${out ? '>' : '<'}`;
    if (captions.has(id)) return `${id}:${key}`;
    const corner = corners.get(key);
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
      // Harder at crossings than ELK is by default; a board of Syn's is a few
      // dozen boxes, and this costs milliseconds.
      'elk.layered.thoroughness': '50',
      'elk.layered.crossingMinimization.greedySwitch.type': 'TWO_SIDED',
      ...tune,
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
  const dx = left - Math.min(...laid.map((c) => (c.x ?? 0) + inset(c.id)));
  const dy = top - Math.min(...laid.map((c) => c.y ?? 0));
  const at = (p: { x: number; y: number }): Point => ({ x: p.x + dx, y: p.y + dy });

  for (const c of laid) {
    const piece = pieces.find((p) => p.node.id === c.id);
    if (!piece) continue;
    const to = at({ x: (c.x ?? 0) + inset(c.id), y: c.y ?? 0 });
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
  // A figure's words are in the way too, of its own lines as much as any:
  // a route through them is the router's to do, and a label slides off them.
  for (const id of captions.keys()) {
    const caption = captionOf(byId.get(id)!);
    if (caption) boxes.set(`${id}:words`, caption);
  }
  // A line can end on a table as a whole — "orders → users", not a row of
  // each — and was left out, drawn as a plain step from the frame's middle.
  const wholeTables = new Map<string, Outline>();
  for (const { node, rows } of pieces) {
    const box = rows.length ? outlineOf(node) : null;
    if (box) wholeTables.set(node.id, box);
  }
  const endOf = (id: string) => boxes.get(id) ?? wholeTables.get(id);
  const drawn = new Map((out.edges ?? []).map((e) => [e.id, e]));
  const routed: [WBEdge, Route][] = [];
  const labels: LabelToPlace[] = [];
  for (const line of lines) {
    const e = drawn.get(line.id);
    const section = e?.sections?.[0];
    const [from, to] = [endOf(line.source), endOf(line.target)];
    if (!section || !from || !to || !byId.has(line.source)) continue;
    const path = [section.startPoint, ...(section.bendPoints ?? []), section.endPoint].map(at);
    // A row's port is on its table's side, out past the row's own: the
    // line ends on the row.
    if (tableOf.has(line.source)) path[0] = { x: from.x + from.width, y: path[0].y };
    if (tableOf.has(line.target)) path[path.length - 1] = { x: to.x, y: path[path.length - 1].y };
    // A figure's side port is on its piece's side, out past the drawing when
    // the words are wider: the line ends on the drawing.
    const onDrawing = (p: Point, box: Outline, side: Side | undefined): Point =>
      side === 'left' ? { x: box.x, y: p.y } : side === 'right' ? { x: box.x + box.width, y: p.y } : p;
    if (captions.has(line.source)) path[0] = onDrawing(path[0], from, corners.get(`${line.id}>`));
    if (captions.has(line.target)) path[path.length - 1] = onDrawing(path[path.length - 1], to, corners.get(`${line.id}<`));
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
    }
    routed.push([line, route]);
  }
  straightenPairs(routed, endOf, boxes);
  // A try is judged on ELK's own lines; the one kept is untangled too, as
  // the router's lines are (untangle.ts). Lines to a table as a whole are
  // left as ELK drew them: the router knows the table only as its rows.
  if (finish) {
    const free = routed.filter(([line]) => boxes.has(line.source) && boxes.has(line.target));
    const better = untangle(free.map(([line, route]) => ({ id: line.id, source: line.source, target: line.target, route })), boxes);
    for (const pair of routed) {
      const route = better.get(pair[0].id);
      if (route) pair[1] = route;
    }
  }
  for (const [line, route] of routed) {
    const label = typeof line.data?.label === 'string' ? line.data.label.trim() : '';
    const [from, to] = [endOf(line.source)!, endOf(line.target)!];
    if (label) labels.push({ id: line.id, label, path: drawnPath(route, from, to), share: route.labelAt });
  }
  // ELK leaves a label room beside its line; the app draws it on the line.
  const shares = placeLabels(labels, boxes.values());
  for (const [line, route] of routed) {
    applyRoute(line, { ...route, labelAt: shares.get(line.id) ?? route.labelAt });
  }
}

/** Room between lines drawn side by side, and between them and a box's corner. */
const SIDE_BY_SIDE = 20;

/**
 * Two lines or more between the same two boxes — a data flow's request and
 * its answer — drawn straight and side by side, when the boxes stand one over
 * the other (or one beside the other) with room for them across.
 *
 * ELK reverses one of a pair to break the cycle, and ends them where its
 * ports fall: a process and the payment gateway under it were joined by two
 * lines that crossed, with a label on the crossing. Two straight runs cannot
 * cross. Kept as ELK drew them when there is not the room, when a run would
 * go through something, or when a shape's outline pulls the ends together.
 */
function straightenPairs(routed: [WBEdge, Route][], endOf: (id: string) => Outline | undefined, boxes: Map<string, Outline>) {
  const pairs = new Map<string, [WBEdge, Route][]>();
  for (const entry of routed) {
    const key = [entry[0].source, entry[0].target].sort().join('\n');
    pairs.set(key, [...(pairs.get(key) ?? []), entry]);
  }
  for (const group of pairs.values()) {
    if (group.length < 2) continue;
    const first = group[0][0];
    const [p, q] = [endOf(first.source), endOf(first.target)];
    if (!p || !q) continue;
    const n = group.length;
    const lo = { x: Math.max(p.x, q.x), y: Math.max(p.y, q.y) };
    const hi = { x: Math.min(p.x + p.width, q.x + q.width), y: Math.min(p.y + p.height, q.y + q.height) };
    const stacked = (p.y + p.height <= q.y || q.y + q.height <= p.y) && hi.x - lo.x >= SIDE_BY_SIDE * (n + 1);
    const abreast = (p.x + p.width <= q.x || q.x + q.width <= p.x) && hi.y - lo.y >= SIDE_BY_SIDE * (n + 1);
    if (!stacked && !abreast) continue;
    const next = group.map(([line], k) => {
      const [from, to] = [endOf(line.source)!, endOf(line.target)!];
      const share = (k + 1) / (n + 1);
      if (stacked) {
        const x = lo.x + (hi.x - lo.x) * share;
        const down = from.y < to.y;
        return routeOf([{ x, y: down ? from.y + from.height : from.y }, { x, y: down ? to.y : to.y + to.height }], from, to);
      }
      const y = lo.y + (hi.y - lo.y) * share;
      const right = from.x < to.x;
      return routeOf([{ x: right ? from.x + from.width : from.x, y }, { x: right ? to.x : to.x + to.width, y }], from, to);
    });
    const ok = next.every((route, k) => {
      if (!route || route.waypoints.length) return false;
      const line = group[k][0];
      return !runsThrough(drawnPath(route, endOf(line.source)!, endOf(line.target)!), boxes, line);
    });
    // Still apart where they meet their boxes: an outline that lets a line
    // touch it only at one place would put them on top of each other.
    const starts = next.map((route, k) => (route ? drawnPath(route, endOf(group[k][0].source)!, endOf(group[k][0].target)!) : []));
    const apart = starts.every((a, i) => starts.slice(i + 1).every((b) => a.length && b.length && Math.hypot(a[0].x - b[b.length - 1].x, a[0].y - b[b.length - 1].y) >= SIDE_BY_SIDE / 2 && Math.hypot(a[0].x - b[0].x, a[0].y - b[0].y) >= SIDE_BY_SIDE / 2));
    if (!ok || !apart) continue;
    next.forEach((route, k) => {
      group[k][1] = { ...route!, labelAt: 0.5 };
    });
  }
}

/** Coordinates as written to the file: to the tenth of a pixel. */
const tidy = (n: number) => Math.round(n * 10) / 10;
