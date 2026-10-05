import { ref } from 'vue';
import { stampElement } from '../boardFile';

export interface Guide {
  /** `x`: a vertical line at `at`; `y`: a horizontal one. */
  axis: 'x' | 'y';
  at: number;
  from: number;
  to: number;
}

interface Box { left: number; top: number; right: number; bottom: number }

const xs = (b: Box) => [b.left, (b.left + b.right) / 2, b.right];
const ys = (b: Box) => [b.top, (b.top + b.bottom) / 2, b.bottom];

/**
 * How far to move a box so one of its edges or its middle lines up with one
 * of another's, along one axis — the nearest within `threshold`, or none.
 * Pure, for testing.
 */
export function nearestSnap(moving: number[], targets: number[], threshold: number): { delta: number; at: number } | null {
  let best: { delta: number; at: number } | null = null;
  for (const m of moving) {
    for (const t of targets) {
      const delta = t - m;
      if (Math.abs(delta) <= threshold && (!best || Math.abs(delta) < Math.abs(best.delta))) best = { delta, at: t };
    }
  }
  return best;
}

/**
 * Lines that appear while something is dragged, when its edges or middle line
 * up with another item's, and pull it the last few pixels into line — what
 * Miro, Figma and draw.io all do, and what makes a tidy diagram take seconds
 * rather than squinting.
 *
 * Works on the canvas's own nodes during the drag; when it ends, the board is
 * told where they settled.
 */
export function useSmartGuides(ctx: {
  store: any;
  /** The canvas's zoom, so the pull is the same few pixels on screen at any zoom. */
  zoom: () => number;
  /** Every node on the canvas, as the canvas measures it. */
  allNodes: () => any[];
  enabled: () => boolean;
}) {
  const guides = ref<Guide[]>([]);
  let others: Box[] = [];

  /**
   * A node's box. Dragged nodes are measured by `position`, which the drag
   * has just set; the drawn position catches up a frame later.
   */
  const boxOf = (n: any, live = false): Box | null => {
    const w = n.dimensions?.width;
    const h = n.dimensions?.height;
    if (!w || !h) return null;
    const p = live ? n.position : (n.computedPosition ?? n.position);
    return { left: p.x, top: p.y, right: p.x + w, bottom: p.y + h };
  };

  function onDragStart(dragged: any[]) {
    const moving = new Set(dragged.map((n) => n.id));
    // Freehand strokes are not something to line up with: there are many of
    // them and their boxes mean little.
    others = ctx.allNodes()
      .filter((n) => !moving.has(n.id) && n.type !== 'stroke')
      .map((n) => boxOf(n))
      .filter((b): b is Box => !!b);
  }

  /** Where the dragged items would line up, and the lines that show it. */
  function snapFor(dragged: any[]): { dx: number; dy: number; lines: Guide[] } | null {
    if (!ctx.enabled() || !others.length || !dragged.length) return null;
    const boxes = dragged.map((n) => boxOf(n, true)).filter((b): b is Box => !!b);
    if (!boxes.length) return null;
    const group: Box = {
      left: Math.min(...boxes.map((b) => b.left)),
      top: Math.min(...boxes.map((b) => b.top)),
      right: Math.max(...boxes.map((b) => b.right)),
      bottom: Math.max(...boxes.map((b) => b.bottom)),
    };
    const threshold = 6 / Math.max(ctx.zoom(), 0.1);
    const snapX = nearestSnap(xs(group), others.flatMap(xs), threshold);
    const snapY = nearestSnap(ys(group), others.flatMap(ys), threshold);
    const dx = snapX?.delta ?? 0;
    const dy = snapY?.delta ?? 0;

    const moved: Box = { left: group.left + dx, right: group.right + dx, top: group.top + dy, bottom: group.bottom + dy };
    const lines: Guide[] = [];
    if (snapX) {
      const lined = others.filter((o) => xs(o).some((v) => Math.abs(v - snapX.at) < 0.5));
      lines.push({
        axis: 'x', at: snapX.at,
        from: Math.min(moved.top, ...lined.map((o) => o.top)),
        to: Math.max(moved.bottom, ...lined.map((o) => o.bottom)),
      });
    }
    if (snapY) {
      const lined = others.filter((o) => ys(o).some((v) => Math.abs(v - snapY.at) < 0.5));
      lines.push({
        axis: 'y', at: snapY.at,
        from: Math.min(moved.left, ...lined.map((o) => o.left)),
        to: Math.max(moved.right, ...lined.map((o) => o.right)),
      });
    }
    return { dx, dy, lines };
  }

  /**
   * While dragging: show where the items would line up. The pull itself
   * waits for the drop — the canvas drags its own copies of the items and
   * redraws them from the pointer every frame, so a nudge mid-drag would not
   * hold.
   */
  function onDrag(dragged: any[]) {
    guides.value = snapFor(dragged)?.lines ?? [];
  }

  /**
   * The drag is over: pull the items into the line that was showing, and put
   * where they settled into the board. Returns how far they were pulled.
   */
  function onDragStop(dragged: any[]): { dx: number; dy: number } {
    const snap = snapFor(dragged);
    guides.value = [];
    others = [];
    const dx = snap?.dx ?? 0;
    const dy = snap?.dy ?? 0;
    if (dx || dy) {
      for (const n of dragged) n.position = { x: n.position.x + dx, y: n.position.y + dy };
    }
    const board = ctx.store.currentBoardData.value;
    if (board) {
      for (const n of dragged) {
        const node = board.nodes.find((b: any) => b.id === n.id);
        if (!node) continue;
        if (node.position.x === n.position.x && node.position.y === n.position.y) continue;
        node.position = { x: n.position.x, y: n.position.y };
        stampElement(node);
      }
    }
    return { dx, dy };
  }

  return { guides, onDragStart, onDrag, onDragStop };
}
