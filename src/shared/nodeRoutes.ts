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
  // A thread opens in Messages, beside every other thing Syn keeps — the
  // conversations, the runs, what it remembers. It is not an app of its own:
  // one family of features split across two sidebar entries is one too many.
  //
  // `messages` and not `thread`, because this map is read two ways and only one
  // of them is a handler name: `ThingsApp::openInOwner` does
  // `router.push({ name: route })`, which needs a route that exists. Several
  // entries above are not — `project`, `person`, `finance_month` and
  // `pdf_highlight` name handlers rather than routes, so that menu entry has
  // never worked for them. That is worth fixing and is not this change.
  syn_thread: 'messages',
  // What Syn remembers and what it knows how to do, opened in the inspector
  // that already lists both.
  //
  // Their own values rather than `messages`, because that arm in `App.vue`
  // dispatches on the route alone and would have no way to tell three
  // destinations apart. The comment above predicted this — "the day a second
  // one does, the node type has to travel with the id" — and this is the
  // cheaper half of that: a distinct route per destination, rather than a third
  // argument threaded through every `open-node` emit in the app.
  //
  // Safe from the `router.push({ name: route })` wart above, and only these
  // two are: both types are in `useObservedTypes`'s INTERNAL set, so Things
  // never lists them and `openInOwner` is never asked to route one.
  //
  // Until now these had no route at all, which is why half of what
  // `syn::notice` finds arrived with no way to go and look at it.
  syn_memory: 'syn_memory',
  syn_skill: 'syn_skill',
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
  // Syn's own memory, prefixed and filed apart so that a user who wants a kind
  // called `memory` — for a language deck, for anything — can have one.
  SynMemory: 'syn_memory',
  SynSkills: 'syn_skill',
  // Prefixed for the same reason, and filed apart for one more: `is_in_unscanned_dir`
  // skips a folder named exactly `Syn`, so a thread written there would be
  // indexed by the write that made it and dropped by the next full scan.
  SynThreads: 'syn_thread',
};

/**
 * Where a new node of a given type is written.
 *
 * The inverse of the map above, and it has to stay the inverse: a file written
 * into a folder this app does not recognise is still found by the scan — the
 * `type:` in its frontmatter is what counts — but a vault whose folders and
 * types disagree is one nobody can read without the app, which is most of what
 * a folder of markdown is for.
 *
 * A type nobody has heard of gets a folder named after it. Not pluralised:
 * Vietnamese has no plural, guessing one for `book` and not for `cá` produces
 * a vault that looks half-translated, and `Animal/` beside `Notes/` reads
 * fine. Capitalised so it sits with the folders that were here first.
 *
 * Everything used to land in `Notes/`, which is not wrong about the data — the
 * type is the truth and the folder is only where it sits — but it puts cats
 * among the notes when the vault is opened in Finder.
 */
export function folderForType(nodeType: string): string {
  const known = Object.entries(TYPE_FOR_DIRECTORY).find(([, type]) => type === nodeType);
  if (known) return known[0];

  const clean = nodeType.trim();
  if (!clean) return 'Notes';
  return clean.charAt(0).toUpperCase() + clean.slice(1);
}

/**
 * What to call a node type on screen, when this app is the one that invented it.
 *
 * Deliberately not a table of names for every type. Things shows a type's own
 * name on purpose — for `note` and `task` that reads as English beside a
 * Vietnamese interface, and for `animal` it is the only name there is, so
 * naming everything from a table would leave a type nobody coded for with no
 * name at all.
 *
 * This is the narrow exception: types *this app* invented, whose stored name
 * carries a prefix the user never chose. `syn_thread` is prefixed so that
 * somebody who wants a kind called `thread` — for sewing, for forum posts —
 * can have one; showing the prefix leaks that reasoning onto the screen.
 *
 * It stays "Syn thread" rather than shortening to "thread", because a rail
 * showing `thread` twice would recreate exactly the collision the prefix was
 * invented to prevent.
 *
 * Falls back to the type itself, which is what keeps the property the comment
 * in `ThingsApp.vue` is about.
 */
const NAME_FOR_NODE_TYPE: Readonly<Record<string, string>> = {
  syn_thread: 'Syn thread',
};

export function nameForNodeType(nodeType: string): string {
  return NAME_FOR_NODE_TYPE[nodeType] ?? nodeType;
}

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
