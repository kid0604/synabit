import { toRaw } from 'vue';

/**
 * The states undo goes back to, cheaply.
 *
 * A step back used to be the whole board written out as one string, fifty
 * deep: on a board of a few megabytes of pen strokes, every action wrote all
 * of it again, and the stack held hundreds of megabytes. A state is now a
 * list of one string per item, and an item's string is made once and reused
 * for as long as the item is unchanged — so a step that moved one box holds
 * one new string and pointers to all the others.
 *
 * An item counts as unchanged while each of its own fields is the same value
 * it was: the board is changed by replacing an item's `data` or `position`,
 * never by writing inside them, which is what makes this check enough.
 */

interface Cached { keys: string[]; parts: unknown[]; json: string }
const cache = new WeakMap<object, Cached>();

/** One item as a string, made again only when the item changed. */
export function itemJson(item: object): string {
  const raw = toRaw(item) as Record<string, unknown>;
  const keys = Object.keys(raw);
  const hit = cache.get(raw);
  if (hit && hit.keys.length === keys.length && hit.keys.every((k, i) => k === keys[i] && hit.parts[i] === raw[k])) {
    return hit.json;
  }
  const json = JSON.stringify(raw);
  cache.set(raw, { keys, parts: keys.map((k) => raw[k]), json });
  return json;
}

export interface BoardState {
  nodeIds: string[];
  nodes: string[];
  edgeIds: string[];
  edges: string[];
}

type Item = { id: string; updated?: number };

export function takeState(board: { nodes: Item[]; edges: Item[] }): BoardState {
  return {
    nodeIds: board.nodes.map((n) => n.id),
    nodes: board.nodes.map(itemJson),
    edgeIds: board.edges.map((e) => e.id),
    edges: board.edges.map(itemJson),
  };
}

export function sameState(a: BoardState | undefined, b: BoardState): boolean {
  if (!a) return false;
  const same = (x: string[], y: string[]) => x.length === y.length && x.every((v, i) => v === y[i]);
  return same(a.nodes, b.nodes) && same(a.edges, b.edges);
}

/**
 * The items of a state, made from what is on the board now where it is the
 * same, and stamped `now` where it is not.
 *
 * The stamp is what makes an undo stick. An undone change put back the stamp
 * the item had before it, older than the change it undid — and sync, which
 * keeps the newer copy of an item, brought the undone change straight back
 * from the other device. Going back is a change like any other, made now.
 */
function restore<T extends Item>(ids: string[], jsons: string[], current: T[], now: number): T[] {
  const byId = new Map(current.map((c) => [c.id, c]));
  return jsons.map((json, i) => {
    const here = byId.get(ids[i]);
    if (here && itemJson(here) === json) return toRaw(here);
    const item = JSON.parse(json) as T;
    item.updated = now;
    return item;
  });
}

export function restoreState<N extends Item, E extends Item>(
  state: BoardState,
  board: { nodes: N[]; edges: E[] },
  now = Date.now(),
): { nodes: N[]; edges: E[] } {
  return {
    nodes: restore(state.nodeIds, state.nodes, board.nodes, now),
    edges: restore(state.edgeIds, state.edges, board.edges, now),
  };
}
