import { ref } from 'vue';
import { getStroke } from 'perfect-freehand';

/** The feel of a stroke. One set, so a stroke the eraser cuts looks like the stroke it was. */
const STROKE_FEEL = { thinning: 0.5, smoothing: 0.5, streamline: 0.5 };

/** Two decimals: a hundredth of a pixel is below anything a screen can show. */
const round = (n: number) => Math.round(n * 100) / 100;

export function getSvgPathFromStroke(stroke: number[][]) {
  if (!stroke.length) return '';

  const d = stroke.reduce(
    (acc, [x0, y0], i, arr) => {
      const [x1, y1] = arr[(i + 1) % arr.length];
      acc.push(round(x0), round(y0), round((x0 + x1) / 2), round((y0 + y1) / 2));
      return acc;
    },
    ['M', round(stroke[0][0]), round(stroke[0][1]), 'Q'] as (string | number)[]
  );

  d.push('Z');
  return d.join(' ');
}

/**
 * The outline perfect-freehand draws around a run of points.
 *
 * `realPressure` is for a pen that reports how hard it is pressed. Left to
 * itself the library makes pressure up from speed, which overrides what the
 * pen said; a mouse or a finger reports a flat 0.5 and is better served by
 * the made-up kind.
 */
function outlineOf(points: number[][], size: number, realPressure: boolean, last: boolean) {
  return getStroke(points, { size, ...STROKE_FEEL, simulatePressure: !realPressure, last });
}

export interface BuiltStroke {
  /** Where the stroke's box starts on the board. */
  x: number;
  y: number;
  /** The box the ink actually covers, so the canvas can measure and cull it. */
  width: number;
  height: number;
  svgPath: string;
  /** The points, relative to `x`/`y`, which is what the eraser cuts along. */
  points: number[][];
}

/**
 * A finished stroke, from points in board coordinates.
 *
 * The box is the outline's, not the points': the ink reaches half the pen's
 * width past the line it was drawn along. Measured from the points, the box
 * sat inside the ink, and the canvas — which only builds what is on screen —
 * dropped a stroke while its edge was still in view.
 */
export function buildStroke(
  boardPoints: number[][],
  size: number,
  realPressure = false,
): BuiltStroke | null {
  if (boardPoints.length < 2) return null;
  const outline = outlineOf(boardPoints, size, realPressure, true);
  if (!outline.length) return null;

  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const [x, y] of outline) {
    if (x < minX) minX = x;
    if (y < minY) minY = y;
    if (x > maxX) maxX = x;
    if (y > maxY) maxY = y;
  }
  // Whole pixels for the origin, so the numbers in the file stay short.
  const x = Math.floor(minX);
  const y = Math.floor(minY);

  return {
    x,
    y,
    width: Math.max(1, Math.ceil(maxX - x)),
    height: Math.max(1, Math.ceil(maxY - y)),
    svgPath: getSvgPathFromStroke(outline.map(([px, py]) => [px - x, py - y])),
    points: boardPoints.map(([px, py, p]) => [round(px - x), round(py - y), round(p ?? 0.5)]),
  };
}

export function useFreeDrawing(options: {
  color: { value: string };
  /** The width the stroke will be saved at, so the preview is the stroke. */
  size: { value: number };
  onStrokeComplete: (stroke: BuiltStroke, color: string, size: number, realPressure: boolean) => void;
}) {
  const isDrawing = ref(false);
  const previewPath = ref('');

  let points: number[][] = [];
  // The one pointer this stroke belongs to. A second finger on the glass is
  // not more of the same line.
  let pointerId: number | null = null;
  let realPressure = false;
  let frame = 0;

  const toBoard = (e: PointerEvent, rect: DOMRect, viewport: { x: number; y: number; zoom: number }) => [
    (e.clientX - rect.left - viewport.x) / viewport.zoom,
    (e.clientY - rect.top - viewport.y) / viewport.zoom,
    realPressure ? e.pressure : 0.5,
  ];

  /**
   * Redraw the preview, at most once a frame.
   *
   * The outline is worked out from every point so far, so doing it on every
   * pointer move costs more the longer the stroke gets — and a pen reports
   * moves far faster than the screen can show them.
   */
  function schedulePreview() {
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      if (!isDrawing.value) return;
      previewPath.value = getSvgPathFromStroke(outlineOf(points, options.size.value, realPressure, false));
    });
  }

  function startDraw(e: PointerEvent, canvasRect: DOMRect, viewport: { x: number; y: number; zoom: number }) {
    if (isDrawing.value) return;
    isDrawing.value = true;
    pointerId = e.pointerId;
    // A pen that reports nothing reports 0 or 0.5; anything else is a
    // reading.
    realPressure = e.pointerType === 'pen' && e.pressure > 0 && e.pressure !== 0.5;
    points = [toBoard(e, canvasRect, viewport)];
    // Keeps the stroke's moves coming here when the pen strays over a menu,
    // and its end arriving even if it is lifted outside the window.
    (e.currentTarget as Element | null)?.setPointerCapture?.(e.pointerId);
  }

  function continueDraw(e: PointerEvent, canvasRect: DOMRect, viewport: { x: number; y: number; zoom: number }) {
    if (!isDrawing.value || e.pointerId !== pointerId) return;
    points.push(toBoard(e, canvasRect, viewport));
    schedulePreview();
  }

  function endDraw(e?: PointerEvent) {
    if (!isDrawing.value) return;
    if (e && e.pointerId !== pointerId) return;
    isDrawing.value = false;
    pointerId = null;
    if (frame) cancelAnimationFrame(frame);
    frame = 0;

    const built = buildStroke(points, options.size.value, realPressure);
    if (built?.svgPath) {
      options.onStrokeComplete(built, options.color.value, options.size.value, realPressure);
    }

    previewPath.value = '';
    points = [];
  }

  /** Drop the stroke in progress without saving it. */
  function cancelDraw(e?: PointerEvent) {
    if (!isDrawing.value) return;
    if (e && e.pointerId !== pointerId) return;
    isDrawing.value = false;
    pointerId = null;
    if (frame) cancelAnimationFrame(frame);
    frame = 0;
    previewPath.value = '';
    points = [];
  }

  return {
    isDrawing,
    previewPath,
    startDraw,
    continueDraw,
    endDraw,
    cancelDraw,
  };
}
