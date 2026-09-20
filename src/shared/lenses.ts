/**
 * Thấu kính — a saved question, kept as an ordinary node.
 *
 * The design is `docs/nexus-lenses-2026-09-19.md`, step 2.
 *
 * # Why a node, and why that costs almost nothing
 *
 * A lens could have been a file in `Timeline/`, like a seal or a hush. It is a
 * node instead, and the reasons are all about what a node already gets for
 * free: it syncs, it can be tagged and searched, it shows up on the graph, it
 * can be shared, and it has a history.
 *
 * The part worth noticing is that *listing* lenses needs no new read API at
 * all. `is:lens` is a query, and `run_node_query` has answered queries for a
 * long time — so the shelf is built out of the same machine the shelf is for.
 * If saving a question had needed a new table, a new command and a new sync
 * path, that would have been the design telling us it was wrong.
 */
import { invoke } from '@tauri-apps/api/core';
import type { QueryResult } from './views/types';

/**
 * How a lens wants its answer drawn.
 *
 * `dated`, `list` and `table` have renderers today (`views/shapeFor`). The
 * rest are named in §7 of the design and arrive with the transforms that
 * produce their data; a lens carrying one of them falls back to `auto` until
 * then, which is the reading that shows the person their answer rather than
 * an error.
 */
export type Render = 'auto' | 'dated' | 'list' | 'table' | 'quotes' | 'bars' | 'one';

export interface Lens {
  /** The node's path, which is its id. */
  id: string;
  title: string;
  /** The question, in the language `run_node_query` reads. */
  query: string;
  render: Render;
  /** A lucide name, or empty to let the shelf choose. */
  icon: string;
}

/** The columns a lens is read back through, in the order `cells` arrives in. */
const COLUMNS = ['title', 'query', 'render', 'icon'] as const;

// `nodes` first: a shelf is a list of saved questions, which are nodes, and
// naming the table means the question cannot drift to the timeline if a word
// in it ever changes meaning (§4).
export const SHELF_QUERY = `nodes type:lens columns:${COLUMNS.join(',')} sort:title limit:100`;

/**
 * Where a lens file goes.
 *
 * `folderForType` would answer `Lens`, and it is what every other type uses —
 * but it is not imported here so that this module stays a pure description of
 * what a lens *is*, testable without the node-routing table.
 */
export const LENS_FOLDER = 'Lens';

export function lensPath(): string {
  return `${LENS_FOLDER}/${crypto.randomUUID()}.md`;
}

/** Read a shelf out of a query result, dropping rows that are not a question. */
export function lensesFrom(result: QueryResult): Lens[] {
  const at = (name: string) => result.columns.indexOf(name);
  const title = at('title');
  const query = at('query');
  const render = at('render');
  const icon = at('icon');

  return result.rows
    .map(row => ({
      id: row.id,
      title: (title >= 0 ? row.cells[title] : '') || row.title,
      query: query >= 0 ? (row.cells[query] ?? '') : '',
      render: normalise(render >= 0 ? row.cells[render] : ''),
      icon: icon >= 0 ? (row.cells[icon] ?? '') : '',
    }))
    // A lens with no question is not a lens. It can happen: somebody makes a
    // node of this type by hand, or empties the field. Showing it would put a
    // button on the shelf that does nothing when pressed.
    .filter(lens => lens.query.trim().length > 0);
}

const RENDERS: Render[] = ['auto', 'dated', 'list', 'table', 'quotes', 'bars', 'one'];

/** Anything unknown means "let the answer choose", which is the default. */
export function normalise(value: string | undefined): Render {
  const clean = (value ?? '').trim().toLowerCase();
  return (RENDERS as string[]).includes(clean) ? (clean as Render) : 'auto';
}

/** What a lens is written into the vault as. */
export function propertiesOf(lens: Omit<Lens, 'id'>): Record<string, unknown> {
  const properties: Record<string, unknown> = { query: lens.query.trim() };
  // Only what was chosen. Writing `render: auto` and `icon: ''` into every
  // saved question would turn two defaults into two commitments, and a person
  // reading the file would think they had picked them.
  if (lens.render !== 'auto') properties.render = lens.render;
  if (lens.icon.trim()) properties.icon = lens.icon.trim();
  return properties;
}

/**
 * A name for a question nobody named.
 *
 * The query itself, which is honest and short, rather than a model's guess at
 * what the person meant by it.
 */
export function nameFor(query: string): string {
  const clean = query.trim().replace(/\s+/g, ' ');
  return clean.length > 60 ? `${clean.slice(0, 57)}…` : clean;
}

/**
 * Two questions, so the bar is not empty — and only two.
 *
 * # Why not twelve
 *
 * §8 promised a dozen: *"a new vault comes with about twelve lenses, and they
 * can be edited — that is the curriculum."* Built and looked at, twelve was a
 * wall of chips across the bottom of the screen, and a wall of chips is
 * **the same thing as the eight panels** this whole design was written to get
 * rid of. §2 of that document diagnosed the panels as a symptom; twelve
 * starters is the symptom coming back wearing the cure's clothes.
 *
 * The principle, in the words it was corrected with: *build the database in
 * every detail; using it has to be flexible, and the person has to be the one
 * in charge.* A shelf that arrives full is the app deciding what somebody is
 * interested in.
 *
 * So: an example, not a menu. One of each half of the language —
 *
 * - **Năm nay** is the shortest true sentence there is: name the table, name a
 *   time. It teaches that `events` is a thing you can say.
 * - **Nhịp sống** is the other half: a pipeline, something worked out rather
 *   than listed, drawn as bars because the answer says it is a comparison.
 *
 * Short enough to read at a glance, which is the whole of what an example has
 * to be. Everything else is learnt the two ways that scale: asking in words
 * and keeping what the assistant wrote (`KeepAsLens`), or editing one of
 * these.
 *
 * `id` is not a path: a starter has no file. The shelf keys rows by it and
 * `LensShelf` refuses to delete one, because there is nothing to delete.
 */
export const STARTERS: Lens[] = [
  {
    id: 'starter:this-year',
    title: 'Năm nay',
    query: 'events when:this-year',
    render: 'dated',
    icon: 'calendar',
  },
  {
    id: 'starter:rhythm',
    title: 'Nhịp sống',
    query: 'events when:2016..2026 | stats count by month',
    render: 'bars',
    icon: 'activity',
  },
];

/** Whether a lens is one of the suggestions rather than one somebody kept. */
export function isStarter(lens: Lens): boolean {
  return lens.id.startsWith('starter:');
}

/**
 * The shelf: what somebody kept, then the suggestions they have not.
 *
 * Matched by the question rather than by the name, because the name is theirs
 * to change and the question is what the starter was for. Renaming a kept lens
 * must not make its suggestion come back.
 */
export function lensesOn(kept: Lens[]): Lens[] {
  const already = new Set(kept.map(lens => lens.query.trim()));
  return [...kept, ...STARTERS.filter(starter => !already.has(starter.query.trim()))];
}

export async function readShelf(vaultPath: string): Promise<Lens[]> {
  const result = await invoke<QueryResult>('run_node_query', {
    vaultPath,
    query: SHELF_QUERY,
    offset: 0,
  });
  return lensesOn(lensesFrom(result));
}
