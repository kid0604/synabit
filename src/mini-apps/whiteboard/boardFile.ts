/**
 * The shape of a `.whiteboard.json` file, and the only place that decides what
 * one means.
 *
 * A board is stored as a single opaque JSON document. That is cheap until the
 * day the format changes: a build that reads a file it does not understand and
 * writes it back has destroyed the parts it did not know about, and there is
 * no version in the file to notice with. So the file carries one, this module
 * is the only thing that reads it, and a file from a newer build is refused
 * rather than quietly rewritten.
 */

import { strokeSize } from './inkLayer';

export interface WBNode {
  id: string;
  type: 'shape' | 'stroke' | 'mindmap' | 'text' | 'note' | 'image' | 'sticky' | 'frame' | 'card' | 'comment';
  position: { x: number; y: number };
  data: Record<string, any>;
  /**
   * When this item last changed, in milliseconds since the epoch.
   *
   * Two devices editing one board cannot both win: the file is opaque, so the
   * whole document is resolved in favour of one side. Per-item stamps are what
   * a real merge would need to keep both, and they have to be in the data
   * before that merge exists — a board saved today without them can never be
   * merged later. `0` means "written before this was recorded", which loses to
   * anything that carries a real time.
   */
  updated?: number;
}

export interface WBEdge {
  id: string;
  source: string;
  sourceHandle?: string;
  target: string;
  targetHandle?: string;
  type: string;
  data?: Record<string, any>;
  /** See `WBNode.updated`. */
  updated?: number;
}

export interface WhiteboardData {
  /** Which version of this format the file was written as. */
  schemaVersion?: number;
  title: string;
  tags: string[];
  created_at: string;
  /**
   * Everything the board file carries that is about the board rather than on
   * it. Kept open because a board may reach us with keys this app never
   * wrote — `linked_projects`, put there when a board is created from a
   * project — and a save must hand them back untouched.
   */
  metadata?: Record<string, any>;
  viewport: { x: number; y: number; zoom: number };
  nodes: WBNode[];
  edges: WBEdge[];
}

/**
 * The version this build writes.
 *
 * 1 — the first numbered version. Boards written before it declare nothing;
 *     `migrate` brings them here by filling the fields that were always
 *     assumed to exist and giving every item a change stamp.
 *
 * # When to raise this
 *
 * Only when an older build would *lose* something by opening the file and
 * saving it, because raising it locks every older build out of every board —
 * including the boards that use nothing new.
 *
 * Adding an item type is not that. An older build draws an item it does not
 * recognise as an empty box, which is wrong on screen, but the item itself
 * survives: the file is parsed and re-serialised whole, so what it does not
 * understand it also does not touch. Losing a version of the app for a
 * fortnight of boxes is the worse trade. `image` was added this way.
 */
export const BOARD_SCHEMA_VERSION = 1;

export type BoardRead =
  | { ok: true; data: WhiteboardData }
  | { ok: false; reason: 'unreadable' }
  | { ok: false; reason: 'too-new'; fileVersion: number };

/** Mark an item as changed now. */
export function stampElement(element: { updated?: number }): void {
  element.updated = Date.now();
}

/** The board's own last-save time as a number, or 0 when it has none. */
function boardTime(raw: any): number {
  const stamp = raw?.metadata?.updated_at ?? raw?.created_at;
  const parsed = typeof stamp === 'string' ? Date.parse(stamp) : NaN;
  return Number.isNaN(parsed) ? 0 : parsed;
}

/**
 * Bring a board written by an older build up to the current format.
 *
 * Everything here has to be safe to run on a file that has already been
 * migrated, because a board is migrated on every open rather than once.
 */
function migrate(raw: any): WhiteboardData {
  const data = raw as WhiteboardData;

  // Fields the app has always assumed. A board file has been legal without
  // them since the first version, so this is a migration and not a repair.
  if (!Array.isArray(data.nodes)) data.nodes = [];
  if (!Array.isArray(data.edges)) data.edges = [];
  if (!data.viewport) data.viewport = { x: 0, y: 0, zoom: 1 };
  if (!Array.isArray(data.tags)) data.tags = [];
  if (typeof data.title !== 'string') data.title = '';

  // A connection to something that is not there cannot be drawn, and the
  // canvas reads both ends of every edge it is asked to lay out — with
  // off-screen items left unbuilt, reading the missing end is a crash rather
  // than a blank. Boards from before deletion cleaned up after itself can
  // still carry these.
  const ids = new Set(data.nodes.map((n) => n.id));
  data.edges = data.edges.filter((e) => ids.has(e.source) && ids.has(e.target));

  // Items that predate change stamps are dated by the board they are on: it
  // is the most recent thing that can be said about them truthfully.
  const fallback = boardTime(raw);
  for (const node of data.nodes) {
    if (typeof node.updated !== 'number') node.updated = fallback;
  }
  for (const edge of data.edges) {
    if (typeof edge.updated !== 'number') edge.updated = fallback;
  }

  // A position is two numbers, and everything that draws an item writes it
  // into markup or a transform: a file from elsewhere with anything else there
  // is read as the nearest thing that is one.
  for (const node of data.nodes) {
    const x = Number(node.position?.x);
    const y = Number(node.position?.y);
    if (!(Number.isFinite(x) && Number.isFinite(y) && typeof node.position.x === 'number' && typeof node.position.y === 'number')) {
      node.position = { x: Number.isFinite(x) ? x : 0, y: Number.isFinite(y) ? y : 0 };
    }
  }

  // Strokes from before strokes saved their box. The ink layer culls by it,
  // and everything that measures an item without drawing it reads it.
  for (const node of data.nodes) {
    if (node.type === 'stroke' && node.data && !(node.data.width && node.data.height)) {
      Object.assign(node.data, strokeSize(node.data));
    }
  }

  data.schemaVersion = BOARD_SCHEMA_VERSION;
  return data;
}

/**
 * Read a board file.
 *
 * A file from a newer build comes back as `too-new` rather than as a board
 * with its unknown parts dropped — opening it read-only is a worse outcome
 * than not opening it, only if the app then saves, which it would.
 */
export function readBoardFile(raw: string): BoardRead {
  let parsed: any;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return { ok: false, reason: 'unreadable' };
  }
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
    return { ok: false, reason: 'unreadable' };
  }

  const declared = parsed.schemaVersion;
  if (typeof declared === 'number' && declared > BOARD_SCHEMA_VERSION) {
    return { ok: false, reason: 'too-new', fileVersion: declared };
  }

  return { ok: true, data: migrate(parsed) };
}

/** A new, empty board, ready to be written. */
export function newBoardData(title: string): WhiteboardData {
  const now = new Date().toISOString();
  return {
    schemaVersion: BOARD_SCHEMA_VERSION,
    title,
    tags: [],
    created_at: now,
    metadata: { updated_at: now },
    viewport: { x: 0, y: 0, zoom: 1 },
    nodes: [],
    edges: [],
  };
}

// ─── Deletions ───────────────────────────────────────────

/** How long a deletion is remembered; matches `board_merge.rs`. */
const FORGET_DELETIONS_AFTER_MS = 180 * 24 * 60 * 60 * 1000;

/**
 * Record what was deleted since `before`, in the board's `metadata.deleted`.
 *
 * Sync combines two copies of a board item by item (see `board_merge.rs`),
 * and an item on one side only could be new there or deleted here. This is
 * how it tells: every deleted id with the time it went. Worked out at save
 * time from the copy last written, so every way of deleting — a key, the
 * eraser, a cut, a merge — is covered by one rule. Entries older than half a
 * year are dropped.
 */
export function recordDeletions(before: WhiteboardData | null, now: WhiteboardData, at = Date.now()): void {
  const deleted: Record<string, number> = { ...((now.metadata?.deleted ?? {}) as Record<string, number>) };
  if (before) {
    const present = new Set([...now.nodes, ...now.edges].map((i) => i.id));
    for (const item of [...before.nodes, ...before.edges]) {
      if (!present.has(item.id)) deleted[item.id] = at;
    }
  }
  // An id back on the board — an undo — is no longer deleted.
  for (const item of [...now.nodes, ...now.edges]) delete deleted[item.id];
  for (const [id, when] of Object.entries(deleted)) {
    if (at - when > FORGET_DELETIONS_AFTER_MS) delete deleted[id];
  }
  now.metadata = { ...(now.metadata ?? {}) };
  if (Object.keys(deleted).length) now.metadata.deleted = deleted;
  else delete now.metadata.deleted;
}

/** Fields of `metadata` that are bookkeeping, not the board's own. */
const BOOKKEEPING = new Set(['updated_at', 'deleted', 'stamps']);

/**
 * Record when each of the board's own fields last changed, in
 * `metadata.stamps`: `title`, `tags`, and `metadata.<key>` for the rest.
 *
 * Items carry their own stamps; the board's fields did not, so sync settled
 * them by whichever copy was saved last — a board renamed on one device kept
 * its old name if the other device saved a moved box afterwards. Worked out
 * at save time against the copy last written, so every way of changing a
 * field is covered without each one remembering to stamp.
 */
export function stampFields(before: WhiteboardData | null, now: WhiteboardData, at = Date.now()): void {
  if (!before) return;
  const stamps: Record<string, number> = { ...((now.metadata?.stamps ?? {}) as Record<string, number>) };
  if (!sameValue(before.title, now.title)) stamps.title = at;
  if (!sameValue(before.tags, now.tags)) stamps.tags = at;
  const keys = new Set([...Object.keys(before.metadata ?? {}), ...Object.keys(now.metadata ?? {})]);
  for (const key of keys) {
    if (BOOKKEEPING.has(key)) continue;
    if (!sameValue(before.metadata?.[key], now.metadata?.[key])) stamps[`metadata.${key}`] = at;
  }
  if (!Object.keys(stamps).length) return;
  now.metadata = { ...(now.metadata ?? {}), stamps };
}

// ─── Merging ─────────────────────────────────────────────

/**
 * Two values the same, whatever order their keys were written in.
 *
 * The board file has writers in Rust as well as here — Syn, sync — and Rust
 * writes an object's keys in alphabetical order. Compared as written, every
 * item a Rust write passed through looked changed there, and an item deleted
 * here was kept as "worked on there": the deletion undone.
 */
function sameValue(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (typeof a !== 'object' || typeof b !== 'object' || a === null || b === null) return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  if (Array.isArray(a)) {
    const bs = b as unknown[];
    return a.length === bs.length && a.every((v, i) => sameValue(v, bs[i]));
  }
  const ao = a as Record<string, unknown>;
  const bo = b as Record<string, unknown>;
  // A key holding `undefined` is a key JSON never writes.
  const ak = Object.keys(ao).filter((k) => ao[k] !== undefined);
  const bk = Object.keys(bo).filter((k) => bo[k] !== undefined);
  return ak.length === bk.length && ak.every((k) => sameValue(ao[k], bo[k]));
}

/**
 * Whether an item is different from what it was, by what it is rather than
 * when it was stamped. A board written without stamps has them filled in on
 * every read from that read's own save time, so two reads of an item nobody
 * touched can carry different stamps.
 */
const sameItem = (a: { updated?: number }, b: { updated?: number }) =>
  sameValue({ ...a, updated: 0 }, { ...b, updated: 0 });

/**
 * One field of a record, three ways: what changed on one side wins.
 * Changed on both, `hereWins` decides. A key deleted on one side and
 * untouched on the other stays deleted.
 */
function mergeRecord(
  base: Record<string, any> | undefined,
  local: Record<string, any> | undefined,
  disk: Record<string, any> | undefined,
  skip: Set<string> = new Set(),
  hereWins = true,
): Record<string, any> {
  const out: Record<string, any> = {};
  const keys = new Set([...Object.keys(local ?? {}), ...Object.keys(disk ?? {})]);
  for (const key of keys) {
    if (skip.has(key)) continue;
    const changedHere = !sameValue(local?.[key], base?.[key]);
    const changedThere = !sameValue(disk?.[key], base?.[key]);
    const value = changedHere && (!changedThere || hereWins) ? local?.[key] : disk?.[key];
    if (value !== undefined) out[key] = value;
  }
  return out;
}

/**
 * One item changed on both sides: each field from whichever side changed it,
 * so Syn renaming a box while it is dragged here keeps both the new name and
 * the new place. A field both sides changed goes to the later change.
 */
function mergeItem<T extends { updated?: number; data?: Record<string, any> }>(was: T, mine: T, theirs: T): T {
  const hereWins = (mine.updated ?? 0) >= (theirs.updated ?? 0);
  const merged = mergeRecord(was, mine, theirs, new Set(['data', 'updated']), hereWins) as T;
  if (mine.data || theirs.data) merged.data = mergeRecord(was.data, mine.data, theirs.data, new Set(), hereWins);
  merged.updated = Math.max(mine.updated ?? 0, theirs.updated ?? 0);
  return merged;
}

/**
 * The items of a board, three ways, by id.
 *
 * An item changed on one side only takes that side; changed on both, the two
 * are combined field by field. Deleted on one side and changed on the other,
 * it is kept: a deletion of something the other side was still working on is
 * the one outcome that loses work.
 */
function mergeItems<T extends { id: string; updated?: number; data?: Record<string, any> }>(
  base: T[],
  local: T[],
  disk: T[],
): T[] {
  const baseById = new Map(base.map((i) => [i.id, i]));
  const diskById = new Map(disk.map((i) => [i.id, i]));
  const localIds = new Set(local.map((i) => i.id));
  const newer = (a: T, b: T) => ((b.updated ?? 0) > (a.updated ?? 0) ? b : a);
  const out: T[] = [];

  for (const mine of local) {
    const was = baseById.get(mine.id);
    const theirs = diskById.get(mine.id);
    if (!theirs) {
      // Added here, or deleted there: kept if it is new or was worked on here.
      if (!was || !sameItem(mine, was)) out.push(mine);
      continue;
    }
    if (!was) { out.push(newer(mine, theirs)); continue; }
    const changedHere = !sameItem(mine, was);
    const changedThere = !sameItem(theirs, was);
    if (changedHere && changedThere) out.push(mergeItem(was, mine, theirs));
    else out.push(changedThere ? theirs : mine);
  }

  for (const theirs of disk) {
    if (localIds.has(theirs.id)) continue;
    const was = baseById.get(theirs.id);
    // Added there, or deleted here: kept if it is new or was worked on there.
    if (!was || !sameItem(theirs, was)) out.push(theirs);
  }
  return out;
}

/**
 * Combine this window's board with the copy now on disk.
 *
 * `base` is the file as this window last read or wrote it — the point both
 * copies grew from. Without it, an item missing from one side could be an
 * addition on the other or a deletion on this one, and there is no telling
 * which; with it, each side's changes are known and both are kept.
 *
 * The board file has more than one writer: Syn editing a board, a project
 * linking one, the board pane beside a conversation, a sync bringing in
 * another device's copy. Each used to be undone by the next save from here,
 * which wrote this window's older copy back over whatever had arrived.
 */
export function mergeBoards(base: WhiteboardData, local: WhiteboardData, disk: WhiteboardData): WhiteboardData {
  const merged = mergeRecord(base, local, disk, new Set(['nodes', 'edges', 'metadata'])) as WhiteboardData;
  merged.metadata = mergeRecord(base.metadata, local.metadata, disk.metadata, new Set(['deleted', 'stamps']));
  // When each field last changed: the later of the two, field by field.
  const stamps = { ...(disk.metadata?.stamps ?? {}) } as Record<string, number>;
  for (const [key, at] of Object.entries((local.metadata?.stamps ?? {}) as Record<string, number>)) {
    stamps[key] = Math.max(stamps[key] ?? 0, at);
  }
  if (Object.keys(stamps).length) merged.metadata.stamps = stamps;
  // What either side deleted stays deleted — the later time for each id.
  const deleted = { ...(disk.metadata?.deleted ?? {}) } as Record<string, number>;
  for (const [id, at] of Object.entries((local.metadata?.deleted ?? {}) as Record<string, number>)) {
    deleted[id] = Math.max(deleted[id] ?? 0, at);
  }
  if (Object.keys(deleted).length) merged.metadata.deleted = deleted;
  merged.nodes = mergeItems(base.nodes, local.nodes, disk.nodes);
  const ids = new Set(merged.nodes.map((n) => n.id));
  // An edge survives only while both of its ends do.
  merged.edges = mergeItems(base.edges, local.edges, disk.edges).filter(
    (e) => ids.has(e.source) && ids.has(e.target),
  );
  return merged;
}
