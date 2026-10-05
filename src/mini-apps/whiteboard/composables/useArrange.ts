import { PROMOTE_LIMIT } from '../inkLayer';
import { useVueFlow } from '@vue-flow/core';
import { ref, type Ref } from 'vue';
import { stampElement } from '../boardFile';
import type { WBEdge, WBNode, WhiteboardData } from '../boardFile';
import { SHAPES_MAP } from '../shapes';
import { carryWaypoints } from '../waypoints';

/**
 * What a copy from a board looks like on the system clipboard.
 *
 * Plain JSON in the text slot, tagged so a paste can tell it from text the
 * user copied anywhere else. The system clipboard rather than a variable in
 * this window, so a copy in one board pastes into another — or into the same
 * board after the app was restarted.
 */
export const CLIPBOARD_KIND = 'synabit/whiteboard';

export interface BoardClip {
  kind: typeof CLIPBOARD_KIND;
  version: 1;
  nodes: WBNode[];
  edges: WBEdge[];
}

/** The style a node can hand to another: everything that is looks, nothing that is content. */
const STYLE_KEYS = [
  'color', 'fillColor', 'borderWidth', 'dashStyle', 'opacity',
  'fontSize', 'fontWeight', 'fontStyle', 'textAlign', 'backgroundColor',
] as const;

export type AlignMode = 'left' | 'centerX' | 'right' | 'top' | 'centerY' | 'bottom';

interface Box { id: string; x: number; y: number; width: number; height: number }

const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value));

/**
 * A clip whose items are new: fresh ids, edges and groups re-pointed at them.
 * Only edges with both ends in the clip come along — an edge to something
 * that was not copied has nothing to attach to.
 */
export function rekeyClip(
  clip: Pick<BoardClip, 'nodes' | 'edges'>,
  generateId: (prefix: string) => string,
): { nodes: WBNode[]; edges: WBEdge[] } {
  const ids = new Map<string, string>();
  const groups = new Map<string, string>();
  const nodes = clip.nodes.map((n) => {
    const copy = clone(n);
    copy.id = generateId(n.type);
    ids.set(n.id, copy.id);
    // A copied group is its own group, not more members of the original.
    if (copy.data?.groupId) {
      if (!groups.has(copy.data.groupId)) groups.set(copy.data.groupId, generateId('grp'));
      copy.data.groupId = groups.get(copy.data.groupId);
    }
    delete copy.data?.editing;
    return copy;
  });
  // What an item points at by id goes to the copy of it, or is let go: a
  // copied comment is about the copied item, not the original (which it
  // would otherwise follow around), and a copied live card belongs to the
  // copied frame — left on the original's, that frame's next refresh took it
  // for a duplicate of its own and deleted it.
  for (const copy of nodes) {
    for (const key of ['on', 'fromQuery'] as const) {
      const was = copy.data?.[key];
      if (typeof was !== 'string') continue;
      if (ids.has(was)) copy.data[key] = ids.get(was);
      else delete copy.data[key];
    }
  }
  const edges = clip.edges
    .filter((e) => ids.has(e.source) && ids.has(e.target))
    .map((e) => ({ ...clone(e), id: generateId('e'), source: ids.get(e.source)!, target: ids.get(e.target)! }));
  return { nodes, edges };
}

/** Read a board clip out of clipboard text, or null when the text is something else. */
export function parseClip(text: string | null | undefined): BoardClip | null {
  if (!text || !text.startsWith('{')) return null;
  try {
    const value = JSON.parse(text);
    if (value?.kind !== CLIPBOARD_KIND || !Array.isArray(value.nodes)) return null;
    return { kind: CLIPBOARD_KIND, version: 1, nodes: value.nodes, edges: Array.isArray(value.edges) ? value.edges : [] };
  } catch {
    return null;
  }
}

/**
 * Where each box goes to line up along `mode`. Pure, so it can be tested
 * without a canvas: boxes in, positions out.
 */
export function alignBoxes(boxes: Box[], mode: AlignMode): Map<string, { x: number; y: number }> {
  const left = Math.min(...boxes.map((b) => b.x));
  const right = Math.max(...boxes.map((b) => b.x + b.width));
  const top = Math.min(...boxes.map((b) => b.y));
  const bottom = Math.max(...boxes.map((b) => b.y + b.height));
  const out = new Map<string, { x: number; y: number }>();
  for (const b of boxes) {
    let { x, y } = b;
    if (mode === 'left') x = left;
    if (mode === 'right') x = right - b.width;
    if (mode === 'centerX') x = (left + right) / 2 - b.width / 2;
    if (mode === 'top') y = top;
    if (mode === 'bottom') y = bottom - b.height;
    if (mode === 'centerY') y = (top + bottom) / 2 - b.height / 2;
    out.set(b.id, { x: Math.round(x), y: Math.round(y) });
  }
  return out;
}

/**
 * Equal gaps between boxes along an axis, the outermost two left where they
 * are — what draw.io and Figma call distribute.
 */
export function distributeBoxes(boxes: Box[], axis: 'x' | 'y'): Map<string, { x: number; y: number }> {
  const size = axis === 'x' ? 'width' : 'height';
  const sorted = [...boxes].sort((a, b) => a[axis] - b[axis]);
  const first = sorted[0];
  const last = sorted[sorted.length - 1];
  const span = last[axis] + last[size] - first[axis];
  const filled = sorted.reduce((sum, b) => sum + b[size], 0);
  const gap = (span - filled) / (sorted.length - 1);
  const out = new Map<string, { x: number; y: number }>();
  let at = first[axis];
  for (const b of sorted) {
    out.set(b.id, axis === 'x' ? { x: Math.round(at), y: b.y } : { x: b.x, y: Math.round(at) });
    at += b[size] + gap;
  }
  return out;
}

/**
 * Everything the user does to items they have selected rather than to the
 * canvas: order, lock, align, copy, paste.
 *
 * Each operation changes the board in the store — one step back, one save —
 * and then has the canvas redrawn from it. Writing the canvas and the store
 * separately, by hand, is how this app got its out-of-step bugs.
 */
export function useArrange(ctx: {
  store: any;
  vfNodes: Ref<any[]>;
  /** Redraw the canvas from the board, selecting `select` if given. */
  refresh: (select?: Iterable<string>) => void;
  /**
   * Show that these items moved, and nothing else changed: the fast way, for
   * an arrow key held down. Without it, `refresh` redraws the whole board.
   */
  moved?: (ids: string[]) => void;
  scheduleSave: () => void;
}) {
  const { store, vfNodes, refresh, scheduleSave } = ctx;
  const { findNode } = useVueFlow({ id: 'whiteboard-flow' });

  const board = (): WhiteboardData | null => store.currentBoardData.value;
  const nodeById = (id: string): WBNode | undefined => board()?.nodes.find((n) => n.id === id);

  /** What is selected now, in board order. */
  function selectedIds(): string[] {
    return vfNodes.value.filter((n: any) => n.selected).map((n: any) => n.id);
  }

  /** The selection minus what is locked: what may be moved or resized. */
  function movableIds(ids = selectedIds()): string[] {
    return ids.filter((id) => !nodeById(id)?.data?.locked);
  }

  /** The box a node takes up on the board, as drawn. */
  function boxOf(id: string): Box | null {
    const node = nodeById(id);
    if (!node) return null;
    const drawn = findNode(id);
    const width = drawn?.dimensions?.width || node.data.width || 0;
    const height = drawn?.dimensions?.height || node.data.height || 0;
    return { id, x: node.position.x, y: node.position.y, width, height };
  }

  /** Change the board as one step back, then save and redraw. */
  function change(work: () => void, select?: Iterable<string>, undoKey?: string, onlyMoved?: string[]) {
    if (!board()) return;
    if (undoKey) store.pushUndoState(undoKey);
    store.beginUndoBatch();
    if (!undoKey) store.pushUndoState();
    try {
      work();
    } finally {
      store.endUndoBatch();
    }
    if (onlyMoved && ctx.moved) ctx.moved(onlyMoved);
    else refresh(select);
    scheduleSave();
  }

  function setPositions(positions: Map<string, { x: number; y: number }>) {
    const moved = new Map<string, { dx: number; dy: number }>();
    for (const [id, at] of positions) {
      const node = nodeById(id);
      if (!node || (node.position.x === at.x && node.position.y === at.y)) continue;
      moved.set(id, { dx: at.x - node.position.x, dy: at.y - node.position.y });
      node.position = { ...at };
      stampElement(node);
    }
    carryWaypoints(board()!.edges, moved);
  }

  function setData(id: string, data: Record<string, any>) {
    const node = nodeById(id);
    if (!node) return;
    node.data = { ...node.data, ...data };
    stampElement(node);
  }

  // ─── Order ──────────────────────────────────────────────

  /** The stacking value a node is drawn with now. */
  const stackOf = (id: string): number => vfNodes.value.find((n: any) => n.id === id)?.zIndex ?? 0;

  /**
   * Put the selection above, or below, everything else — keeping its own
   * order. The value is stored on the node; until a node has one it is
   * stacked the way it always was, smaller things over bigger ones.
   */
  function reorder(where: 'front' | 'back', ids = selectedIds()) {
    // A stroke given a place in the stacking order becomes a canvas item for
    // good (inkLayer.ts). For a few, that is the point; for a whole drawing,
    // it would undo the ink layer and every later open would build thousands
    // of items. Past the limit, the strokes keep their place under the rest.
    const strokes = ids.filter((id) => nodeById(id)?.type === 'stroke');
    if (strokes.length > PROMOTE_LIMIT) {
      const keep = new Set(strokes);
      ids = ids.filter((id) => !keep.has(id));
    }
    if (!ids.length) return;
    const all = (board()?.nodes ?? []).map((n) => stackOf(n.id));
    const chosen = [...ids].sort((a, b) => stackOf(a) - stackOf(b));
    change(() => {
      if (where === 'front') {
        let z = Math.max(0, ...all);
        for (const id of chosen) setData(id, { z: ++z });
      } else {
        let z = Math.min(1, ...all);
        for (const id of [...chosen].reverse()) setData(id, { z: --z });
      }
    }, ids);
  }

  // ─── Lock ───────────────────────────────────────────────

  /** Lock the selection, or unlock it when every item in it is locked. */
  function toggleLock(ids = selectedIds()) {
    if (!ids.length) return;
    const lock = !ids.every((id) => nodeById(id)?.data?.locked);
    change(() => {
      for (const id of ids) setData(id, { locked: lock || undefined });
    }, ids);
  }

  // ─── Layout ─────────────────────────────────────────────

  function align(mode: AlignMode, ids = movableIds()) {
    const boxes = ids.map(boxOf).filter((b): b is Box => !!b);
    if (boxes.length < 2) return;
    change(() => setPositions(alignBoxes(boxes, mode)), selectedIds());
  }

  function distribute(axis: 'x' | 'y', ids = movableIds()) {
    const boxes = ids.map(boxOf).filter((b): b is Box => !!b);
    if (boxes.length < 3) return;
    change(() => setPositions(distributeBoxes(boxes, axis)), selectedIds());
  }

  /** Give the selection the width or height of its largest item. */
  function matchSize(dimension: 'width' | 'height', ids = movableIds()) {
    const resizable = ids.filter((id) => ['shape', 'image', 'note', 'text', 'sticky', 'frame'].includes(nodeById(id)?.type ?? ''));
    const boxes = resizable.map(boxOf).filter((b): b is Box => !!b);
    if (boxes.length < 2) return;
    const target = Math.max(...boxes.map((b) => b[dimension]));
    change(() => {
      for (const b of boxes) setData(b.id, { [dimension]: Math.round(target) });
    }, selectedIds());
  }

  /** Move the selection by a few pixels; a run of presses is one step back. */
  function nudge(dx: number, dy: number, ids = movableIds()) {
    if (!ids.length) return;
    const positions = new Map(
      ids.map((id) => {
        const p = nodeById(id)!.position;
        return [id, { x: p.x + dx, y: p.y + dy }];
      }),
    );
    change(() => setPositions(positions), selectedIds(), 'nudge', ids);
  }

  function selectAll() {
    refresh((board()?.nodes ?? []).map((n) => n.id));
  }

  // ─── Copy, paste, duplicate ─────────────────────────────

  /** The selection as a clip: its items, and the edges among them. */
  function clipOf(ids = selectedIds()): BoardClip | null {
    if (!ids.length || !board()) return null;
    const chosen = new Set(ids);
    return {
      kind: CLIPBOARD_KIND,
      version: 1,
      nodes: board()!.nodes.filter((n) => chosen.has(n.id)).map(clone),
      edges: board()!.edges.filter((e) => chosen.has(e.source) && chosen.has(e.target)).map(clone),
    };
  }

  /**
   * Put a clip on the board, its top-left corner at `at` or, without a place,
   * just below and right of where it came from. The pasted items end up
   * selected, so they can be moved straight away.
   */
  function paste(clip: Pick<BoardClip, 'nodes' | 'edges'>, at?: { x: number; y: number }) {
    if (!board() || !clip.nodes.length) return;
    const fresh = rekeyClip(clip, (prefix) => store.generateId(prefix));
    const left = Math.min(...fresh.nodes.map((n) => n.position.x));
    const top = Math.min(...fresh.nodes.map((n) => n.position.y));
    const dx = at ? at.x - left : 24;
    const dy = at ? at.y - top : 24;
    change(() => {
      for (const node of fresh.nodes) {
        node.position = { x: Math.round(node.position.x + dx), y: Math.round(node.position.y + dy) };
        stampElement(node);
        board()!.nodes.push(node);
      }
      for (const edge of fresh.edges) {
        // Waypoints are places on the board; they move with what they join.
        if (Array.isArray(edge.data?.waypoints)) {
          edge.data!.waypoints = edge.data!.waypoints.map((p: { x: number; y: number }) => ({ x: p.x + dx, y: p.y + dy }));
        }
        stampElement(edge);
        board()!.edges.push(edge);
      }
    }, fresh.nodes.map((n) => n.id));
  }

  function duplicate(ids = selectedIds()) {
    const clip = clipOf(ids);
    if (clip) paste(clip);
  }

  /** Remove the selection — what is locked stays. */
  function removeSelection(ids = selectedIds()) {
    const removable = new Set(movableIds(ids));
    if (!removable.size) return;
    change(() => {
      const b = board()!;
      b.nodes = b.nodes.filter((n) => !removable.has(n.id));
      b.edges = b.edges.filter((e) => !removable.has(e.source) && !removable.has(e.target));
    }, []);
  }

  /**
   * Put the selection in a new frame that fits around it, and select the
   * frame. Room is left above for the frame's title.
   */
  function frameSelection(ids = selectedIds(), title = '') {
    const boxes = ids.map(boxOf).filter((b): b is Box => !!b);
    if (!boxes.length) return;
    const pad = 40;
    const left = Math.min(...boxes.map((b) => b.x)) - pad;
    const top = Math.min(...boxes.map((b) => b.y)) - pad;
    const right = Math.max(...boxes.map((b) => b.x + b.width)) + pad;
    const bottom = Math.max(...boxes.map((b) => b.y + b.height)) + pad;
    const frame: WBNode = {
      id: store.generateId('frame'),
      type: 'frame',
      position: { x: Math.round(left), y: Math.round(top) },
      data: { label: title, width: Math.round(right - left), height: Math.round(bottom - top) },
    };
    change(() => {
      stampElement(frame);
      board()!.nodes.push(frame);
    }, [frame.id]);
  }

  // ─── Style ──────────────────────────────────────────────

  const copiedStyle = ref<Record<string, any> | null>(null);

  function copyStyle(id = selectedIds()[0]) {
    const node = id ? nodeById(id) : undefined;
    if (!node) return;
    copiedStyle.value = Object.fromEntries(STYLE_KEYS.filter((k) => node.data[k] !== undefined).map((k) => [k, node.data[k]]));
  }

  function pasteStyle(ids = movableIds()) {
    if (!copiedStyle.value || !ids.length) return;
    const style = copiedStyle.value;
    change(() => {
      for (const id of ids) setData(id, style);
    }, ids);
  }

  const hasCopiedStyle = () => !!copiedStyle.value;

  // ─── Shape ──────────────────────────────────────────────

  /** Turn a shape into another shape, keeping its size, place, label and style. */
  function changeShapeType(id: string, shapeType: string) {
    const node = nodeById(id);
    if (!node || node.type !== 'shape' || node.data.locked || !SHAPES_MAP[shapeType] || node.data.shapeType === shapeType) return;
    change(() => setData(id, { shapeType }), [id]);
  }

  return {
    selectedIds,
    reorder,
    toggleLock,
    align,
    distribute,
    matchSize,
    nudge,
    selectAll,
    clipOf,
    paste,
    duplicate,
    removeSelection,
    copyStyle,
    pasteStyle,
    hasCopiedStyle,
    changeShapeType,
    frameSelection,
  };
}
