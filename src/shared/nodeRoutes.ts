/**
 * Which mini-app owns a node, and how to work that out when nobody said.
 *
 * A node's type and the app that opens it are not the same word — an `event`
 * is opened by `calendar`, a `project` by `task` — and three call sites had
 * each written their own version of the mapping. One of them defaulted to
 * `note` for anything it did not recognise, which is how clicking a task
 * reminder in Syn opened the Notes editor on a task file: the notification
 * carried no type at all, and "unknown" and "note" were the same answer.
 *
 * Guessing `note` is the one fallback this must never have. The note editor
 * saves what it holds as `nodeType: 'note'`, so a task opened there becomes a
 * note on the first autosave, and the task is gone. `null` — "I do not know" —
 * is a usable answer; a wrong one is not.
 */

/** Node type as stored in frontmatter → the `open-node` route that handles it. */
export const ROUTE_FOR_NODE_TYPE: Readonly<Record<string, string>> = {
  note: 'note',
  task: 'task',
  project: 'project',
  event: 'calendar',
  person: 'person',
  quickcap: 'quickcap',
  whiteboard: 'whiteboard',
  finance_month: 'finance_month',
  feed_source: 'feed_source',
  file: 'file',
  pdf: 'pdf',
  pdf_highlight: 'pdf_highlight',
};

/** The route for a node type, or `null` when the type is unknown. */
export function routeForNodeType(nodeType: string | null | undefined): string | null {
  if (!nodeType) return null;
  return ROUTE_FOR_NODE_TYPE[nodeType] ?? null;
}

/**
 * The folder each node type is written into.
 *
 * Only a fallback, and only for records already written: notifications sitting
 * in a vault from before they carried a type have nothing else to go on. A
 * node reached any other way should be asked for its type rather than have it
 * inferred from where it happens to live.
 */
const TYPE_FOR_DIRECTORY: Readonly<Record<string, string>> = {
  Tasks: 'task',
  Projects: 'project',
  Events: 'event',
  People: 'person',
  Notes: 'note',
  QuickCaps: 'quickcap',
  Whiteboards: 'whiteboard',
};

/** The node type implied by a vault-relative path, or `null`. */
export function nodeTypeFromPath(relPath: string | null | undefined): string | null {
  if (!relPath) return null;
  // Windows vaults hand back backslashes; the same path must read the same way.
  const top = relPath.replace(/\\/g, '/').split('/')[0];
  return TYPE_FOR_DIRECTORY[top] ?? null;
}

/**
 * The best route available for a node, given whatever is known about it.
 *
 * Returns `null` rather than a guess when nothing identifies the node.
 */
export function routeForNode(
  nodeType: string | null | undefined,
  relPath?: string | null,
): string | null {
  return routeForNodeType(nodeType) ?? routeForNodeType(nodeTypeFromPath(relPath));
}
