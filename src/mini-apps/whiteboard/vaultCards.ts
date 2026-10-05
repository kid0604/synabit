/**
 * Things from the vault, on a board.
 *
 * A board is where thinking happens; the vault is where the things being
 * thought about live — tasks, people, events, projects, notes. A card is one
 * of them placed on the board: it shows the thing as it is now, not a copy
 * made when it was dropped, and what can be changed from a card (a task
 * ticked off, a title) is changed in the vault.
 *
 * On the board a card is `{ type: 'card', data: { ref, kind, title } }`:
 * `ref` is the thing's id (its vault path), `kind` its node type, and `title`
 * the name it had when last seen — what the board shows, and what search and
 * Syn read, when the thing itself cannot be loaded.
 */

/** The kinds a card can show. A note has its own card, the older `note` item. */
export const CARD_KINDS = ['task', 'person', 'event', 'project', 'note', 'file', 'pdf', 'whiteboard'] as const;

/** What can be searched for in the "Add from the vault" picker. */
export const PICKABLE_KINDS = ['task', 'project', 'event', 'person', 'note'] as const;

export const CARD_SIZE = { width: 260, height: 120 };

/** A `synabit://kind/path` link, as a kind and an id — or null when the text is not one. */
export function parseVaultLink(text: string): { kind: string; id: string } | null {
  const m = /^\s*(?:\[[^\]]*\]\()?synabit:\/\/([a-z_]+)\/([^)\s]+)\)?\s*$/i.exec(text);
  if (!m) return null;
  const kind = m[1].toLowerCase() === 'node' ? '' : m[1].toLowerCase();
  let id = m[2];
  try {
    id = decodeURIComponent(id);
  } catch {
    // Keep it as written.
  }
  return { kind, id };
}

/** Today, as the vault writes dates. */
export function today(): string {
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** The Monday of this week, as the vault writes dates. */
export function weekStart(): string {
  const d = new Date();
  const back = (d.getDay() + 6) % 7;
  d.setDate(d.getDate() - back);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}
