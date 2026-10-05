import { ref, type Ref } from 'vue';
import type { WBNode } from './useWhiteboardStore';
import { buildStroke } from './useFreeDrawing';

/** Squared distance from point P to the segment AB. */
function distanceToSegment2(px: number, py: number, ax: number, ay: number, bx: number, by: number) {
  const dx = bx - ax;
  const dy = by - ay;
  const length2 = dx * dx + dy * dy;
  const t = length2 ? Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / length2)) : 0;
  const cx = ax + t * dx - px;
  const cy = ay + t * dy - py;
  return cx * cx + cy * cy;
}

export function useEraser(
  store: any,
  vfNodes: Ref<any[]>,
  viewport: any,
  scheduleSave: () => void,
  canvasEl: () => HTMLElement | null,
) {
  const isErasing = ref(false);
  const eraserPos = ref<{ x: number; y: number } | null>(null);

  // Where the eraser was at the last move of this wipe, in board coordinates.
  // A fast wipe jumps many pixels between moves; testing only where it landed
  // let it pass straight over a thin line.
  let lastAt: { x: number; y: number } | null = null;

  /** The wipe is over; the next one starts from wherever it lands. */
  function endWipe() {
    lastAt = null;
  }

  /**
   * Erase along the pointer.
   *
   * Called on every pointer move while the eraser is down, and each call can
   * remove a stroke and rebuild what is left of it — several operations, all
   * part of one wipe. The caller opens a batch for the gesture so that the
   * whole wipe is a single step back; without it a wipe across a drawing
   * pushed dozens of entries and emptied the history behind them.
   */
  function eraseStrokesNear(e: PointerEvent) {
    const el = canvasEl();
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const cx = (e.clientX - rect.left - viewport.value.x) / viewport.value.zoom;
    const cy = (e.clientY - rect.top - viewport.value.y) / viewport.value.zoom;
    const from = lastAt ?? { x: cx, y: cy };
    lastAt = { x: cx, y: cy };
    const r = store.activeStrokeSize.value;
    const r2 = r * r;

    // The wipe's own box, to pass over strokes nowhere near it without
    // looking at their points.
    const wipeLeft = Math.min(from.x, cx) - r;
    const wipeRight = Math.max(from.x, cx) + r;
    const wipeTop = Math.min(from.y, cy) - r;
    const wipeBottom = Math.max(from.y, cy) + r;

    // A locked stroke is out of reach, as it is for every other way of deleting.
    const strokeNodes = (store.currentBoardData.value?.nodes || []).filter((n: WBNode) => n.type === 'stroke' && !n.data?.locked);
    let changed = false;
    let next = vfNodes.value;

    for (const sn of strokeNodes) {
      const pts = sn.data.points as number[][] | undefined;
      if (!pts || pts.length < 2) continue;
      const nodeX = sn.position.x;
      const nodeY = sn.position.y;
      const w = sn.data.width as number | undefined;
      const h = sn.data.height as number | undefined;
      if (w && h && (nodeX > wipeRight || nodeY > wipeBottom || nodeX + w < wipeLeft || nodeY + h < wipeTop)) {
        continue;
      }

      let hasHit = false;
      const hitMap = pts.map(([px, py]) => {
        const hit = distanceToSegment2(nodeX + px, nodeY + py, from.x, from.y, cx, cy) < r2;
        if (hit) hasHit = true;
        return hit;
      });
      if (!hasHit) continue;

      // Split points into contiguous non-hit segments
      const segments: number[][][] = [];
      let currentSeg: number[][] = [];
      for (let i = 0; i < pts.length; i++) {
        if (!hitMap[i]) {
          currentSeg.push(pts[i]);
        } else {
          if (currentSeg.length >= 2) segments.push(currentSeg);
          currentSeg = [];
        }
      }
      if (currentSeg.length >= 2) segments.push(currentSeg);

      store.removeNode(sn.id);
      next = next.filter((n: any) => n.id !== sn.id);
      changed = true;

      // What is left of the stroke, each run its own stroke, drawn the way
      // the original was.
      const size = (sn.data.size as number) || 3;
      const realPressure = !!sn.data.realPressure;
      for (const seg of segments) {
        const built = buildStroke(
          seg.map(([px, py, p]) => [nodeX + px, nodeY + py, p]),
          size,
          realPressure,
        );
        if (!built?.svgPath) continue;

        const newNode: WBNode = {
          id: store.generateId('stroke'),
          type: 'stroke',
          position: { x: built.x, y: built.y },
          data: {
            ...sn.data,
            svgPath: built.svgPath,
            points: built.points,
            width: built.width,
            height: built.height,
          },
        };
        store.addNode(newNode);
        next = [...next, { ...newNode, draggable: true }];
      }
    }
    if (changed) {
      // Once per move, not once per piece: every new list is a pass over the
      // whole canvas.
      vfNodes.value = next;
      scheduleSave();
    }
  }

  return { isErasing, eraserPos, eraseStrokesNear, endWipe };
}
