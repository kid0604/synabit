import type { WBEdge } from '../whiteboard/boardFile';
import { paint } from '../whiteboard/ink';

/**
 * A line as the canvas needs it: colour, weight and dashes as the app draws
 * them (`buildVfEdge`). Shared by the pane beside the conversation and the
 * picture in it, so a reply drawn dashed in the file is dashed in both.
 */
export const edgeForCanvas = (e: WBEdge) => {
  const d = (e.data ?? {}) as { label?: string; color?: string; strokeWidth?: number; dashStyle?: string };
  return {
    ...e,
    type: e.type || 'default',
    label: d.label || '',
    style: {
      stroke: paint(d.color) || undefined,
      strokeWidth: d.strokeWidth ? `${d.strokeWidth}px` : undefined,
      strokeDasharray: d.dashStyle === 'dashed' ? '8 4' : d.dashStyle === 'dotted' ? '2 4' : undefined,
    },
    zIndex: 10001,
  };
};

/** The smallest zoom at which a 13px label is still about 10px on screen. */
export const READABLE = 0.75;
/** Room around the drawing, in screen pixels. */
const PAD = 20;
/** As tall as a picture in an answer gets; the rest is dragged into view. */
const TALLEST = 720;
const SHORTEST = 140;

export interface Bounds { left: number; top: number; right: number; bottom: number; span: number; drop: number }

/**
 * How a board sits in a picture `width` pixels wide: the zoom, the height, and
 * whether it has to be dragged to be seen whole.
 *
 * As large as fits, up to its own size, and never below `READABLE`: a board
 * that would have to shrink further keeps that size and overflows, because a
 * drawing whose words are smudges has not been shown. Centred when the drawing
 * fits, even if only by giving up some of its margin; from its top-left
 * corner when it has to be dragged.
 */
export function fitPicture(b: Bounds, width: number) {
  const zoom = Math.max(Math.min(1, (width - 2 * PAD) / b.span, (TALLEST - 2 * PAD) / b.drop), READABLE);
  const height = Math.round(Math.min(Math.max(b.drop * zoom + 2 * PAD, SHORTEST), TALLEST));
  const across = b.span * zoom <= width - 2;
  const down = b.drop * zoom <= height - 2;
  const x = across ? (width - b.span * zoom) / 2 - b.left * zoom : PAD - b.left * zoom;
  const y = down ? (height - b.drop * zoom) / 2 - b.top * zoom : PAD - b.top * zoom;
  const margin = PAD / zoom;
  return {
    zoom,
    height,
    viewport: { x, y, zoom },
    overflows: !across || !down,
    extent: [
      [b.left - margin, b.top - margin],
      [b.right + margin, b.bottom + margin],
    ] as [[number, number], [number, number]],
  };
}
