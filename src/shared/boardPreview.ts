/**
 * A board, small, as a picture you can look at without leaving the page.
 *
 * # Why this is not the board editor
 *
 * Because a conversation that mentions a board should show it, and showing it
 * had meant: click the link, wait for the Whiteboard app to mount, look, come
 * back. Three actions to see a picture that was already drawn.
 *
 * What goes on screen here is an SVG built from the file — no canvas, no
 * dragging, no components per box. A preview of forty boxes is forty
 * rectangles and some text, which costs about as much as an image would; the
 * editing surfaces stay where they are, one click away, for when somebody
 * actually wants to move something.
 *
 * Freehand strokes are drawn too, as the polylines they are, because a board
 * with somebody's handwriting on it and none of it showing is a board they
 * will not recognise.
 */
import type { WhiteboardData, WBNode } from '../mini-apps/whiteboard/boardFile';

/** How big the picture is allowed to be, in the bubble. */
const WIDE = 640;
const TALL = 360;

/** Below this, a label is drawn as a line rather than as words nobody can read. */
const READABLE = 7;

const escape = (text: string): string =>
  text.replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c] ?? c);

const width = (node: WBNode): number => Number(node.data?.width) || 160;
const height = (node: WBNode): number => Number(node.data?.height) || 80;
const label = (node: WBNode): string => String(node.data?.label ?? '');

/**
 * Whether this box is a frame — something drawn behind other things.
 *
 * The same rule the assistant's own reader uses: a board has no notion of a
 * group, so what makes a frame a frame is that it holds at least two others.
 */
const framesIn = (nodes: WBNode[]): Set<string> => {
  const shapes = nodes.filter(n => n.type === 'shape');
  const frames = new Set<string>();
  for (const outer of shapes) {
    const held = shapes.filter(
      inner =>
        inner.id !== outer.id &&
        inner.position.x >= outer.position.x &&
        inner.position.y >= outer.position.y &&
        inner.position.x + width(inner) <= outer.position.x + width(outer) &&
        inner.position.y + height(inner) <= outer.position.y + height(outer),
    ).length;
    if (held >= 2) frames.add(outer.id);
  }
  return frames;
};

/** Where a line leaves and lands: the middle of the box, which is enough at this size. */
const centre = (node: WBNode) => ({
  x: node.position.x + width(node) / 2,
  y: node.position.y + height(node) / 2,
});

/**
 * The board as an `<svg>` string.
 *
 * Empty for a board with nothing on it — a frame around nothing is worse than
 * no picture, because it looks like something failed.
 */
export function boardPreview(board: WhiteboardData): string {
  const nodes = board.nodes ?? [];
  const shapes = nodes.filter(n => n.type === 'shape');
  const strokes = nodes.filter(n => n.type === 'stroke');
  if (!shapes.length && !strokes.length) return '';

  // What the picture has to cover.
  let left = Infinity;
  let top = Infinity;
  let right = -Infinity;
  let bottom = -Infinity;
  for (const node of nodes) {
    left = Math.min(left, node.position.x);
    top = Math.min(top, node.position.y);
    right = Math.max(right, node.position.x + width(node));
    bottom = Math.max(bottom, node.position.y + height(node));
  }
  const span = Math.max(right - left, 1);
  const drop = Math.max(bottom - top, 1);
  const scale = Math.min(WIDE / span, TALL / drop, 1);
  const frames = framesIn(nodes);
  const byId = new Map(nodes.map(n => [n.id, n]));

  const parts: string[] = [];

  // Frames first, so they sit behind what they hold.
  for (const node of shapes.filter(n => frames.has(n.id))) {
    parts.push(
      `<rect x="${node.position.x}" y="${node.position.y}" width="${width(node)}" height="${height(node)}"` +
        ` rx="10" fill="rgba(148,163,184,0.10)" stroke="rgba(148,163,184,0.8)" stroke-width="${1.5 / scale}"/>`,
    );
    const words = label(node);
    if (words && 13 * scale >= READABLE) {
      parts.push(
        `<text x="${node.position.x + 12}" y="${node.position.y + 20 / scale}"` +
          ` font-size="${13 / scale}" fill="#64748b">${escape(words)}</text>`,
      );
    }
  }

  for (const edge of board.edges ?? []) {
    const from = byId.get(edge.source);
    const to = byId.get(edge.target);
    if (!from || !to) continue;
    const a = centre(from);
    const b = centre(to);
    parts.push(
      `<line x1="${a.x}" y1="${a.y}" x2="${b.x}" y2="${b.y}"` +
        ` stroke="rgba(100,116,139,0.55)" stroke-width="${1.2 / scale}"/>`,
    );
  }

  for (const node of strokes) {
    const points = (node.data?.points as [number, number, number?][] | undefined) ?? [];
    if (points.length < 2) continue;
    const path = points
      .map(([x, y]) => `${(node.position.x + x).toFixed(1)},${(node.position.y + y).toFixed(1)}`)
      .join(' ');
    parts.push(
      `<polyline points="${path}" fill="none" stroke="${String(node.data?.color ?? '#7c3aed')}"` +
        ` stroke-width="${1.5 / scale}" stroke-opacity="0.7"/>`,
    );
  }

  for (const node of shapes.filter(n => !frames.has(n.id))) {
    const w = width(node);
    const h = height(node);
    parts.push(
      `<rect x="${node.position.x}" y="${node.position.y}" width="${w}" height="${h}" rx="8"` +
        ` fill="rgba(124,58,237,0.08)" stroke="#7c3aed" stroke-width="${1.2 / scale}"/>`,
    );
    const words = label(node);
    if (!words) continue;

    // A label smaller than a few pixels is a smudge pretending to be a word.
    // Below that size it is drawn as the line of text it would have been.
    if (12 * scale < READABLE) {
      parts.push(
        `<rect x="${node.position.x + 8}" y="${node.position.y + h / 2 - 2 / scale}"` +
          ` width="${Math.max(w - 16, 4)}" height="${4 / scale}" rx="${2 / scale}" fill="rgba(124,58,237,0.35)"/>`,
      );
      continue;
    }
    // How many characters fit, measured in the pixels this will occupy on
    // screen: the box is `w * scale` wide there, and 12px text averages about
    // 6.6px a character.
    const fits = Math.max(Math.floor((w * scale) / 6.6), 4);
    const shown = words.length > fits ? `${words.slice(0, fits - 1)}…` : words;
    parts.push(
      `<text x="${node.position.x + w / 2}" y="${node.position.y + h / 2 + 4 / scale}"` +
        ` text-anchor="middle" font-size="${12 / scale}" fill="#312e81">${escape(shown)}</text>`,
    );
  }

  const pad = 16 / scale;
  return (
    `<svg viewBox="${left - pad} ${top - pad} ${span + pad * 2} ${drop + pad * 2}"` +
    ` width="100%" style="max-width:${Math.round(span * scale)}px;max-height:${Math.round(drop * scale)}px"` +
    ` xmlns="http://www.w3.org/2000/svg" role="img">${parts.join('')}</svg>`
  );
}
