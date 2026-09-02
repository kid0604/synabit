/**
 * Which frontmatter keys belong to the app, and which belong to the person.
 *
 * The screen that shows every kind of node has to answer a question no
 * single-type screen ever asks: given a key nobody declared, is this something
 * to show? `full_width` and `species` arrive identically — two strings in a
 * YAML map — and the difference between them is not in the data.
 *
 * So the rule is about direction, not importance:
 *
 * - **Bookkeeping** is written by the app and read by the app. A person never
 *   has an opinion about `node_id`. Never a row.
 * - **App-owned** is decided by the person but *set through an affordance* —
 *   `pinned` is a pin icon in Notes, `order` is a drag. The value is theirs;
 *   the encoding is ours. Showing `pinned  false` is noise, and letting it be
 *   typed into is worse, because `flase` is a value YAML accepts and the
 *   Notes app does not.
 * - **Everything else is theirs**, including every key this file has never
 *   heard of.
 *
 * That last line is the whole design. The list below is a denylist, not an
 * allowlist, and it must stay that way: Things exists to show `animal` and
 * `book` and whatever gets invented next month, and an allowlist would show
 * those nodes as blank. A key we do not recognise is data, and data is shown.
 *
 * Hidden is not dropped. Nothing here is ever removed from the file — see the
 * save path in `useThingsNode`, where these same sets are what keep a hidden
 * key out of the deletion-by-subtraction pass.
 */

/**
 * Written by the app, meaningless to edit, never shown as a row.
 *
 * `node_id` is the one that does damage. It is the file's identity to the sync
 * engine; hand-editing one splits the file into two documents on the next
 * sync, and clearing one hands it a fresh identity while every other device
 * keeps the old.
 */
export const BOOKKEEPING = new Set(['node_id', 'created_at', 'updated_at', 'timestamp']);

/** Shown, because "what kind of thing is this" is worth saying, but not typed into. */
export const GOVERNED = new Set(['type', 'title']);

/**
 * Keys the app owns on every kind, not just one.
 *
 * `pinned` was scoped to `note` while Notes was the only screen that pinned
 * anything. Things pins whatever it shows, so the key graduated: it is now
 * machinery on an `animal` as much as on a note, set by a pin and never typed.
 *
 * The cost is named rather than hidden: an `animal` whose owner meant
 * something of their own by `pinned` no longer sees it as a field. That is the
 * price of the affordance being universal, and it is why this list is two
 * entries long rather than a convenient place to put things.
 */
const APP_OWNED_EVERYWHERE = new Set(['pinned']);

/**
 * Per type: keys another screen already renders properly.
 *
 * Scoped by type rather than global, and that is the point of the shape. A
 * `note` has a `full_width` that belongs to the note editor's layout toggle.
 * An `animal` with a `full_width` means something nobody here can guess, so it
 * is shown. A global denylist would take it away.
 */
const APP_OWNED: Record<string, readonly string[]> = {
  // The note editor's own chrome: a pin in the sidebar, a width toggle, and
  // the link picker that maintains `linked_projects`.
  note: ['full_width', 'linked_projects'],

  // Task machinery. `is_transferred`/`transferred_to`/`track_progress` are
  // written by the transfer flow; `completed_at` is derived from `status`;
  // `order` is a drag position; `parent_id`/`project_id` are raw ids that read
  // as noise and are set by pickers.
  task: [
    'order',
    'completed_at',
    'is_transferred',
    'transferred_to',
    'track_progress',
    'parent_id',
    'project_id',
    'source_link',
  ],

  // A saved view is entirely machine-written; its query and layout are edited
  // by the view bar, not by typing YAML.
  view: ['query', 'layout', 'sort', 'sort_descending', 'group', 'columns', 'home'],
};

/**
 * Whether this key on this type is the app's business rather than the
 * person's.
 *
 * Note what is *not* here: `species`, `author`, `rating`, `relationship_type`,
 * `birthday`, `tags`, `status`, `priority`, `due_date`. Those are set through
 * affordances too, but each of them is a fact about the thing rather than a
 * fact about how the app draws it — and a person reading a record wants to see
 * them.
 */
export function isAppOwned(nodeType: string, key: string): boolean {
  if (BOOKKEEPING.has(key) || APP_OWNED_EVERYWHERE.has(key)) return true;
  return (APP_OWNED[nodeType] ?? []).includes(key);
}

/** Every key this app writes for a type, for the "show them anyway" disclosure. */
export function appOwnedKeys(nodeType: string): readonly string[] {
  return APP_OWNED[nodeType] ?? [];
}

/**
 * `due_date` → `Due date`.
 *
 * Only for display, and the raw key stays reachable, because the raw key is
 * what a query is written against: someone who sees "Due date" and types
 * `Due date:tomorrow` gets nothing.
 */
export function humanizeKey(key: string): string {
  const words = key.replace(/[_-]+/g, ' ').trim();
  if (!words) return key;
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/**
 * Kinds whose content is not a Markdown body, and so are not Things' to make.
 *
 * A whiteboard is a `.whiteboard.json` of nodes and edges; a `file` node is
 * metadata about something that already exists on disk. Things writes
 * frontmatter and a Markdown body, and writing that into `Whiteboards/` would
 * produce a file the scan indexes as a whiteboard, lists in the Whiteboards
 * app, and cannot draw on — a thing that is a whiteboard by every sign except
 * having anything in it.
 *
 * Not a refusal to *show* them. Things reads every kind in the vault and this
 * changes nothing about that. It is only about which kinds it offers to
 * create, and the answer for these is: the app that knows their shape.
 *
 * Deliberately a short list of what is known to break rather than a rule about
 * markdown, because the default has to stay "a kind Things has never heard of
 * is one it can make".
 */
const AUTHORED_ELSEWHERE = new Set(['whiteboard', 'file', 'canvas', 'json']);

export function isAuthoredElsewhere(nodeType: string): boolean {
  return AUTHORED_ELSEWHERE.has(nodeType) || nodeType.startsWith('finance_');
}
