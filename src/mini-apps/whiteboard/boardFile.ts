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

export interface WBNode {
  id: string;
  type: 'shape' | 'stroke' | 'mindmap' | 'text' | 'note' | 'image';
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
