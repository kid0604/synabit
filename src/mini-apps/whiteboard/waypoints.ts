import { stampElement } from './boardFile';
import type { WBEdge } from './boardFile';

/**
 * Move the bends of lines whose two ends both moved, by the same amount.
 *
 * A waypoint is a place on the board, not a place relative to anything, so
 * moving a whole diagram used to leave its lines' bends behind, pulled into
 * zig-zags across the board. When both ends of a line moved together — a
 * selection dragged, nudged, pasted — the bends go with them. A line with one
 * end moved keeps its bends where they are: the user put them there on
 * purpose, relative to the board.
 *
 * Returns whether any line changed.
 */
export function carryWaypoints(edges: WBEdge[], moved: Map<string, { dx: number; dy: number }>): boolean {
  let changed = false;
  for (const edge of edges) {
    const points = edge.data?.waypoints as { x: number; y: number }[] | undefined;
    if (!points?.length) continue;
    const a = moved.get(edge.source);
    const b = moved.get(edge.target);
    if (!a || !b || a.dx !== b.dx || a.dy !== b.dy || (!a.dx && !a.dy)) continue;
    edge.data = { ...edge.data, waypoints: points.map((p) => ({ x: p.x + a.dx, y: p.y + a.dy })) };
    stampElement(edge);
    changed = true;
  }
  return changed;
}
