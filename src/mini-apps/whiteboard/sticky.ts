/**
 * Sticky-note paper. Light colours on purpose: a sticky note is paper on any
 * canvas, light or dark, and its writing is always dark ink on it.
 */
export interface StickyColor {
  value: string;
  fill: string;
  /** The darker band along the bottom edge that makes it read as paper. */
  edge: string;
  labelKey: string;
}

export const STICKY_COLORS: StickyColor[] = [
  { value: 'yellow', fill: '#fef08a', edge: '#eab308', labelKey: 'whiteboard.colors.yellow' },
  { value: 'orange', fill: '#fed7aa', edge: '#f97316', labelKey: 'whiteboard.colors.amber' },
  { value: 'pink', fill: '#fbcfe8', edge: '#ec4899', labelKey: 'whiteboard.colors.pink' },
  { value: 'purple', fill: '#ddd6fe', edge: '#8b5cf6', labelKey: 'whiteboard.colors.purple' },
  { value: 'blue', fill: '#bfdbfe', edge: '#3b82f6', labelKey: 'whiteboard.colors.blue' },
  { value: 'green', fill: '#bbf7d0', edge: '#22c55e', labelKey: 'whiteboard.colors.green' },
  { value: 'gray', fill: '#e5e7eb', edge: '#9ca3af', labelKey: 'whiteboard.colors.gray' },
];

/** The paper for a stored colour name; yellow for anything unknown. */
export function stickyColor(value: string | undefined): StickyColor {
  return STICKY_COLORS.find((c) => c.value === value) ?? STICKY_COLORS[0];
}

export const STICKY_SIZE = 200;
