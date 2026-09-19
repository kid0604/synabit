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

/** How a lens wants its answer drawn. Step 4 gives these meaning. */
export type Render = 'auto' | 'list' | 'table' | 'strip' | 'quotes' | 'bars' | 'one';

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

export const SHELF_QUERY = `is:lens columns:${COLUMNS.join(',')} sort:title limit:100`;

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

const RENDERS: Render[] = ['auto', 'list', 'table', 'strip', 'quotes', 'bars', 'one'];

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

export async function readShelf(vaultPath: string): Promise<Lens[]> {
  const result = await invoke<QueryResult>('run_node_query', {
    vaultPath,
    query: SHELF_QUERY,
    offset: 0,
  });
  return lensesFrom(result);
}
