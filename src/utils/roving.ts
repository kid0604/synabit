/**
 * Where a key press moves focus in a group that has one tab stop — a
 * radiogroup, for one: Tab enters and leaves it, the arrows move inside it.
 *
 * Wraps at both ends, as the ARIA radio pattern does. `null` for a key that is
 * not a move, so the caller leaves it alone (Tab, Space, typing).
 */
export function rovingIndex(key: string, index: number, count: number): number | null {
  if (count <= 0) return null;
  switch (key) {
    case 'ArrowRight':
    case 'ArrowDown':
      return (index + 1) % count;
    case 'ArrowLeft':
    case 'ArrowUp':
      return (index - 1 + count) % count;
    case 'Home':
      return 0;
    case 'End':
      return count - 1;
    default:
      return null;
  }
}
