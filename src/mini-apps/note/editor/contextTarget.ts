/**
 * What a right-click in the editor was aimed at.
 *
 * The order below is the whole point. A link lives inside a paragraph and a
 * paragraph may live inside a table cell, so "which menu should open" is a
 * question about which rule gets asked first, not about which selector
 * matches — several of them always will.
 *
 * Getting it wrong is not theoretical: the block rule used to be asked before
 * the link rule, so right-clicking a link — the one gesture everybody reaches
 * for on a link — offered "Copy Block Link" and nothing about the link at all.
 */
export type ContextTarget =
  | { kind: 'table' }
  | { kind: 'link'; href: string }
  | { kind: 'block' }
  | { kind: 'none' };

export function contextTargetFor(target: Element | null): ContextTarget {
  if (!target) return { kind: 'none' };

  // A table cell first: its own controls handle rows and columns, and a link
  // inside a cell is still reached by the link rule below only when the click
  // was not on the cell chrome.
  if (target.closest('td, th') && target.closest('table')) return { kind: 'table' };

  // Then links. `a` without an `href` is not a link anybody can follow.
  const href = target.closest('a[href]')?.getAttribute('href');
  if (href) return { kind: 'link', href };

  // Then the block, which is what block references are copied from.
  if (target.closest('p, h1, h2, h3, h4, h5, h6')) return { kind: 'block' };

  return { kind: 'none' };
}
