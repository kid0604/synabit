/**
 * The board's "black", drawn so it can be seen on either theme.
 *
 * Every palette on the board offers a near-black first, and a board stores
 * that colour as a hex value. Painted as stored, black ink on the dark canvas
 * is invisible — the text, the strokes and the outlines are all still there,
 * and the user sees an empty board. So the near-blacks are not painted as
 * themselves: they are painted with `--wb-ink`, which the canvas sets to a
 * dark grey on the light theme and a light grey on the dark one.
 *
 * Only the painting changes. The file keeps the hex it always held, so a
 * board written before this, or by Syn, is read the same way, and an older
 * build that knows nothing of ink still shows black.
 */
const INK_VALUES = new Set(['#000', '#000000', '#1e1e1e']);

// The fallback is for a board drawn outside the Whiteboard app — the pane
// beside a conversation — where the variable is not set.
export const INK = 'var(--wb-ink, #1e1e1e)';

export function isInk(color: string | null | undefined): boolean {
  return !!color && INK_VALUES.has(color.trim().toLowerCase());
}

/** The colour to paint with: `color` itself, or the theme's ink. */
export function paint(color: string): string;
export function paint(color: string | null | undefined): string | undefined;
export function paint(color: string | null | undefined): string | undefined {
  if (!color) return color ?? undefined;
  return isInk(color) ? INK : color;
}

/**
 * Whether a board colour is dark enough that ink on it has to be light.
 *
 * A board's own background is the user's choice and is the same on either
 * theme, so the theme cannot decide the ink: dark paper in the light theme
 * hid the default ink entirely, and pale paper in the dark theme hid it the
 * other way. The paper decides instead. Hex colours only; anything else is
 * taken as light, which is what every palette offers.
 */
export function isDarkPaper(color: string | null | undefined): boolean {
  const hex = color?.trim().replace(/^#/, '') ?? '';
  const full = hex.length === 3 ? hex.replace(/./g, (c) => c + c) : hex;
  if (!/^[0-9a-f]{6}$/i.test(full)) return false;
  const [r, g, b] = [0, 2, 4].map((i) => parseInt(full.slice(i, i + 2), 16) / 255);
  const lin = (c: number) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b) < 0.18;
}

/** Board backgrounds: pale papers that keep every colour readable, and one dark one. */
export const PAPERS = [
  { value: 'transparent', labelKey: 'whiteboard.colors.default' },
  { value: '#fefce8', labelKey: 'whiteboard.colors.cream' },
  { value: '#f0fdf4', labelKey: 'whiteboard.colors.mint' },
  { value: '#eff6ff', labelKey: 'whiteboard.colors.sky' },
  { value: '#fdf2f8', labelKey: 'whiteboard.colors.rose' },
  { value: '#f4f4f5', labelKey: 'whiteboard.colors.gray' },
  { value: '#1f2937', labelKey: 'whiteboard.colors.slate' },
];

/**
 * The colour words are written in on a filled shape: dark on a light fill,
 * light on a dark one. Written in the theme's text colour whatever the fill,
 * a label was light-on-amber in the dark theme and black-on-black on a shape
 * filled with ink — under 2:1, unreadable. With no fill (or a faint one) the
 * label is on the board itself, so it takes the board's ink, which follows
 * the theme and the board's own paper.
 */
export function labelOn(fill: string | null | undefined): string {
  const f = fill?.trim().toLowerCase();
  if (!f || f === 'none' || f === 'transparent') return 'var(--wb-ink, currentColor)';
  // Ink is painted as the theme's ink: the words take the paper's colour.
  if (isInk(f)) return 'var(--wb-paper, #fdfdfc)';
  const hex = f.replace(/^#/, '');
  if (hex.length === 8 && parseInt(hex.slice(6), 16) < 0x80) return 'var(--wb-ink, currentColor)';
  return isDarkPaper(`#${hex.slice(0, 6)}`) ? '#fafafa' : '#18181b';
}
