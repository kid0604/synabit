import { ref, type Ref } from 'vue';
import type { WBNode } from './useWhiteboardStore';
import { getStroke, getSvgPathFromStroke } from './useFreeDrawing';

export function useEraser(
  store: any,
  vfNodes: Ref<any[]>,
  viewport: any,
  scheduleSave: () => void,
) {
  const isErasing = ref(false);
  const eraserPos = ref<{ x: number; y: number } | null>(null);

  function eraseStrokesNear(e: PointerEvent) {
    const canvasEl = document.querySelector('.vue-flow') as HTMLElement | null;
    if (!canvasEl) return;
    const rect = canvasEl.getBoundingClientRect();
    const cx = (e.clientX - rect.left - viewport.value.x) / viewport.value.zoom;
    const cy = (e.clientY - rect.top - viewport.value.y) / viewport.value.zoom;
    const r = store.activeStrokeSize.value;
    const r2 = r * r;

    const strokeNodes = (store.currentBoardData.value?.nodes || []).filter((n: WBNode) => n.type === 'stroke');
    let changed = false;

    for (const sn of strokeNodes) {
      const pts = sn.data.points as number[][] | undefined;
      if (!pts || pts.length < 2) continue;
      const origSize = sn.data.size as number || 3;
      const origColor = sn.data.color as string || '#000';
      const origOpacity = (sn.data.opacity as number) ?? 0.85;
      const nodeX = sn.position.x;
      const nodeY = sn.position.y;

      // Check if any point is within eraser radius
      let hasHit = false;
      const hitMap = pts.map(([px, py]) => {
        const dx = (nodeX + px) - cx;
        const dy = (nodeY + py) - cy;
        const hit = dx * dx + dy * dy < r2;
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

      // Remove the original stroke
      store.removeNode(sn.id);
      vfNodes.value = vfNodes.value.filter((n: any) => n.id !== sn.id);
      changed = true;

      // Create new stroke nodes from remaining segments
      for (const seg of segments) {
        // Normalize segment to its own bounding box
        let minSX = Infinity, minSY = Infinity;
        for (const [sx, sy] of seg) {
          if (sx < minSX) minSX = sx;
          if (sy < minSY) minSY = sy;
        }
        const normSeg = seg.map(([sx, sy, sp]) => [sx - minSX, sy - minSY, sp]);
        const stroke = getStroke(normSeg, { size: origSize, thinning: 0.5, smoothing: 0.5, streamline: 0.5 });
        const svgPath = getSvgPathFromStroke(stroke);
        if (!svgPath) continue;

        const newNode: WBNode = {
          id: store.generateId('stroke'),
          type: 'stroke',
          position: { x: nodeX + minSX, y: nodeY + minSY },
          data: { svgPath, points: normSeg, color: origColor, size: origSize, opacity: origOpacity },
        };
        store.addNode(newNode);
        vfNodes.value = [...vfNodes.value, { ...newNode, draggable: true }];
      }
    }
    if (changed) scheduleSave();
  }

  return { isErasing, eraserPos, eraseStrokesNear };
}
