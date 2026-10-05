import type { WBNode } from './boardFile';

/** Letters without their marks, so "dien" finds "Điện" — how people type Vietnamese in a hurry. */
export const fold = (s: string) => s.normalize('NFD').replace(/[̀-ͯ]/g, '').replace(/đ/gi, 'd').toLowerCase();

/** The words an item shows, Markdown marks taken out. */
export function wordsOn(n: WBNode): string {
  // A picture's words are what it shows (its alt text); an icon's, what it is of.
  const icon = n.data?.shapeType === 'glyph' && typeof n.data?.glyph?.name === 'string' ? n.data.glyph.name.replace(/-/g, ' ') : undefined;
  const raw = [n.data?.label, n.data?.title, n.data?.noteTitle, n.type === 'image' ? n.data?.alt : undefined, icon].filter((v) => typeof v === 'string').join(' ');
  return raw.replace(/^#+\s*/gm, '').replace(/[*_`>]/g, '').replace(/\s+/g, ' ').trim();
}

/**
 * The items whose words contain `query`, in reading order — top to bottom,
 * then left to right — so stepping through them reads the board as a person
 * would rather than in the order the items happened to be made.
 */
export function findOnBoard(nodes: WBNode[], query: string, hidden: Set<string> = new Set()): WBNode[] {
  const q = fold(query.trim());
  if (!q) return [];
  return nodes
    .filter((n) => !hidden.has(n.id) && fold(wordsOn(n)).includes(q))
    .sort((a, b) => (Math.abs(a.position.y - b.position.y) > 40 ? a.position.y - b.position.y : a.position.x - b.position.x));
}
