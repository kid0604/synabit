import { ref } from 'vue';
import { getStroke } from 'perfect-freehand';

export { getStroke };

export interface StrokePoint {
  x: number;
  y: number;
  pressure: number;
}

export function getSvgPathFromStroke(stroke: number[][]) {
  if (!stroke.length) return '';

  const d = stroke.reduce(
    (acc, [x0, y0], i, arr) => {
      const [x1, y1] = arr[(i + 1) % arr.length];
      acc.push(x0, y0, (x0 + x1) / 2, (y0 + y1) / 2);
      return acc;
    },
    ['M', ...stroke[0], 'Q'] as (string | number)[]
  );

  d.push('Z');
  return d.join(' ');
}

export function useFreeDrawing(options: {
  color: { value: string };
  size: { value: number };
  onStrokeComplete: (svgPath: string, points: number[][], color: string, size: number, minX: number, minY: number) => void;
}) {
  const isDrawing = ref(false);
  const currentPoints = ref<number[][]>([]);
  const previewPath = ref('');

  function startDraw(e: PointerEvent, canvasRect: DOMRect, viewport: { x: number; y: number; zoom: number }) {
    isDrawing.value = true;
    currentPoints.value = [];
    const x = (e.clientX - canvasRect.left - viewport.x) / viewport.zoom;
    const y = (e.clientY - canvasRect.top - viewport.y) / viewport.zoom;
    currentPoints.value.push([x, y, e.pressure || 0.5]);
  }

  function continueDraw(e: PointerEvent, canvasRect: DOMRect, viewport: { x: number; y: number; zoom: number }) {
    if (!isDrawing.value) return;
    const x = (e.clientX - canvasRect.left - viewport.x) / viewport.zoom;
    const y = (e.clientY - canvasRect.top - viewport.y) / viewport.zoom;
    currentPoints.value.push([x, y, e.pressure || 0.5]);

    // Generate preview
    const stroke = getStroke(currentPoints.value, {
      size: options.size.value,
      thinning: 0.5,
      smoothing: 0.5,
      streamline: 0.5,
    });
    previewPath.value = getSvgPathFromStroke(stroke);
  }

  function endDraw() {
    if (!isDrawing.value) return;
    isDrawing.value = false;

    if (currentPoints.value.length < 2) {
      previewPath.value = '';
      currentPoints.value = [];
      return;
    }

    let minX = Infinity;
    let minY = Infinity;
    for (const [x, y] of currentPoints.value) {
      if (x < minX) minX = x;
      if (y < minY) minY = y;
    }

    const normalizedPoints = currentPoints.value.map(([x, y, p]) => [x - minX, y - minY, p]);

    const stroke = getStroke(normalizedPoints, {
      size: options.size.value,
      thinning: 0.5,
      smoothing: 0.5,
      streamline: 0.5,
    });
    const svgPath = getSvgPathFromStroke(stroke);

    if (svgPath) {
      options.onStrokeComplete(
        svgPath,
        normalizedPoints,
        options.color.value,
        options.size.value,
        minX,
        minY
      );
    }

    previewPath.value = '';
    currentPoints.value = [];
  }

  return {
    isDrawing,
    previewPath,
    startDraw,
    continueDraw,
    endDraw,
  };
}
