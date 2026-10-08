import { invoke } from '@tauri-apps/api/core';
import { stampElement } from '../boardFile';
import type { WBNode, WhiteboardData } from '../boardFile';
import { CARD_SIZE } from '../vaultCards';
import { logger } from '../../../utils/logger';
import { dayAt, dayOf, inFrame, kanbanLayout, laneAt, propertyChange, timelineLayout, type Lane, type LiveLayout, type TimeScale } from '../liveLayouts';
import { errorText } from '../../../shared/errorText';

/** The field a layout sorts by, as the frame names it or by default. */
export function layoutField(frame: WBNode): string | null {
  const layout = frame.data?.layout as LiveLayout | undefined;
  if (layout === 'kanban') return String(frame.data.groupBy || 'status');
  if (layout === 'timeline') return String(frame.data.dateField || 'due_date');
  return null;
}

interface QueryRow { id: string; node_type: string; title: string; cells?: string[] }
interface QueryResult { rows: QueryRow[]; columns?: string[] }

/** How many things a live frame holds at most: a board is not a table. */
const MAX_CARDS = 40;
const GAP = 16;
const PAD = 24;

/**
 * Lay a frame's cards out in a grid, in the order the question returned them,
 * and grow the frame to hold them. Pure, for testing.
 */
export function gridInFrame(frame: WBNode, count: number): { positions: { x: number; y: number }[]; height: number } {
  const width = frame.data.width || 480;
  const cols = Math.max(1, Math.floor((width - PAD * 2 + GAP) / (CARD_SIZE.width + GAP)));
  const positions = Array.from({ length: count }, (_, i) => ({
    x: frame.position.x + PAD + (i % cols) * (CARD_SIZE.width + GAP),
    y: frame.position.y + PAD + Math.floor(i / cols) * (CARD_SIZE.height + GAP),
  }));
  const rows = Math.max(1, Math.ceil(count / cols));
  const height = Math.max(frame.data.height || 320, PAD * 2 + rows * CARD_SIZE.height + (rows - 1) * GAP);
  return { positions, height };
}

/**
 * Frames that are a question to the vault: "my tasks due this week", "people
 * tagged #client", "what I finished since Monday". The frame holds a card for
 * each answer and is asked again whenever the board opens and the vault
 * changes — a board that keeps itself up to date.
 *
 * A live frame is a `frame` with `data.query`. The cards it places carry
 * `data.fromQuery` (the frame's id) and are its to move, add and take away;
 * anything else in the frame is the user's and left alone.
 */
export function useLiveFrames(ctx: {
  store: any;
  refresh: () => void;
  scheduleSave: () => void;
  onError: (message: string) => void;
  /**
   * Change a field of a vault item: what a card dropped in another column or
   * on another day does. `kind` and `title` say what the item is.
   */
  writeProperty?: (ref: string, kind: string, title: string, patch: Record<string, unknown>, was: string, now: string) => Promise<void>;
  /**
   * A card's item was changed in the vault. The board's own undo cannot take
   * that back — the change is in the item, not on the board — so the caller
   * offers `undo`, which puts the field back as it was.
   */
  onMoved?: (moved: { title: string; field: string; was: string; now: string; undo: () => Promise<void> }) => void;
  /** Changing the item failed; the card has gone back to where it was. */
  onMoveFailed?: (error: string) => void;
}) {
  const board = (): WhiteboardData | null => ctx.store.currentBoardData.value;
  let running = false;

  /**
   * The answers, each with the value of `field` when a layout sorts by one —
   * asked for as the query's one column, whatever columns the query named.
   */
  async function ask(query: string, field: string | null = null): Promise<(QueryRow & { value: string })[]> {
    let q = /\blimit:\d+/.test(query) ? query : `${query} limit:${MAX_CARDS}`;
    if (field) q = `${q.replace(/\bcolumns:\S+/g, '')} columns:${field}`;
    const result = await invoke<QueryResult>('run_node_query', { query: q });
    const at = field ? (result.columns ?? []).indexOf(field) : -1;
    // Timeline rows are moments, not things to put on a card.
    return (result.rows ?? [])
      .filter((r) => r.id && !r.id.includes('#'))
      .slice(0, MAX_CARDS)
      .map((r) => ({ ...r, value: at >= 0 ? String(r.cells?.[at] ?? '').trim() : '' }));
  }

  /**
   * A live card's id, the same on every device: made from the frame and the
   * answer it shows, so two devices filling the same frame make the same card.
   */
  function liveCardId(frameId: string, ref: string): string {
    let h = 0x811c9dc5;
    for (const ch of `${frameId}\u0000${ref}`) {
      h ^= ch.codePointAt(0)!;
      h = Math.imul(h, 0x01000193) >>> 0;
    }
    return `card-live-${h.toString(16).padStart(8, '0')}`;
  }

  /** Ask one frame's question again and bring its cards in line. Returns whether anything changed. */
  async function refreshFrame(frame: WBNode): Promise<boolean> {
    const query = String(frame.data.query ?? '').trim();
    if (!query) return false;
    const field = layoutField(frame);
    let rows: (QueryRow & { value: string })[];
    try {
      rows = await ask(query, field);
    } catch (err) {
      logger.error('A live frame could not ask its question', err as string);
      ctx.onError(errorText(err));
      return false;
    }
    const b = board();
    if (!b || !b.nodes.includes(frame)) return false;

    const mine = b.nodes.filter((n) => n.type === 'card' && n.data?.fromQuery === frame.id);
    // One card per answer. Two devices refreshing before they synced each
    // made one, and sync kept both: the card with the id both would have
    // given it stays, any other copy goes.
    const byRef = new Map<string, WBNode>();
    const extra = new Set<string>();
    for (const n of [...mine].sort((a, c) => Number(c.id === liveCardId(frame.id, c.data.ref)) - Number(a.id === liveCardId(frame.id, a.data.ref)))) {
      if (byRef.has(n.data.ref)) extra.add(n.id);
      else byRef.set(n.data.ref, n);
    }
    const wanted = new Set(rows.map((r) => r.id));
    let changed = false;

    // What no longer answers the question leaves the frame.
    const leaving = new Set([...extra, ...mine.filter((n) => !wanted.has(n.data.ref)).map((n) => n.id)]);
    if (leaving.size) {
      b.nodes = b.nodes.filter((n) => !leaving.has(n.id));
      b.edges = b.edges.filter((e) => !leaving.has(e.source) && !leaving.has(e.target));
      changed = true;
    }

    // Where each card goes, and what the frame shows besides: its columns'
    // headings, or its line of days.
    const layout = (frame.data.layout as LiveLayout | undefined) ?? 'grid';
    let placeOf: (row: QueryRow, i: number) => { x: number; y: number };
    let size: { width?: number; height: number };
    let shows: Record<string, unknown> = { lanes: undefined, ticks: undefined, scale: undefined, undatedY: undefined };
    if (layout === 'kanban') {
      const k = kanbanLayout(rows, field!);
      placeOf = (row) => inFrame(frame, k.positions.get(row.id)!);
      size = { width: k.width, height: k.height };
      shows = { ...shows, lanes: k.lanes };
    } else if (layout === 'timeline') {
      const tl = timelineLayout(rows, frame.data.width || 640);
      placeOf = (row) => inFrame(frame, tl.positions.get(row.id)!);
      size = { width: tl.width, height: tl.height };
      shows = { ...shows, ticks: tl.ticks, scale: tl.scale, undatedY: tl.undatedY };
    } else {
      const g = gridInFrame(frame, rows.length);
      placeOf = (_row, i) => g.positions[i];
      size = { height: g.height };
    }
    rows.forEach((row, i) => {
      const at = placeOf(row, i);
      let card = byRef.get(row.id);
      if (!card) {
        card = {
          id: liveCardId(frame.id, row.id),
          type: 'card',
          position: at,
          data: { ref: row.id, kind: row.node_type, title: row.title, fromQuery: frame.id, ...(field ? { value: row.value } : {}), ...CARD_SIZE },
        };
        stampElement(card);
        b.nodes.push(card);
        changed = true;
        return;
      }
      const value = field ? row.value : card.data.value;
      if (card.position.x !== at.x || card.position.y !== at.y || card.data.title !== row.title || card.data.value !== value) {
        card.position = at;
        card.data = { ...card.data, title: row.title, value };
        stampElement(card);
        changed = true;
      }
    });
    const next: Record<string, any> = { ...frame.data, ...shows, height: size.height, ...(size.width ? { width: size.width } : {}) };
    for (const key of Object.keys(shows)) if (next[key] === undefined) delete next[key];
    if (JSON.stringify(next) !== JSON.stringify(frame.data)) {
      frame.data = next;
      stampElement(frame);
      changed = true;
    }
    return changed;
  }

  /**
   * A live card was dropped. In a kanban frame, a card in another column takes
   * that column's value; in a timeline, a card moved along the line takes the
   * day it was dropped on. The change is made in the vault — where the Tasks
   * app and everything else see it — and the frame is asked again, which puts
   * the card where it now belongs. Anywhere else, the frame puts it back.
   */
  async function moveCard(card: WBNode): Promise<boolean> {
    const b = board();
    const frame = b?.nodes.find((n) => n.id === card.data?.fromQuery);
    if (!b || !frame || !ctx.writeProperty) return false;
    const field = layoutField(frame);
    const was = String(card.data.value ?? '');
    let now: string | null = null;
    // Dropped outside the frame: not a choice of column or day. The frame
    // puts it back; nothing in the vault changes. (The nearest column used to
    // take it, so a card dragged out to the right was marked done.)
    const cx = card.position.x + CARD_SIZE.width / 2 - frame.position.x;
    const cy = card.position.y + CARD_SIZE.height / 2 - frame.position.y;
    const inside = cx >= 0 && cy >= 0 && cx <= (Number(frame.data.width) || 0) && cy <= (Number(frame.data.height) || 0);
    if (!inside) {
      // Leave `now` empty: the refresh below puts the card back.
    } else if (frame.data.layout === 'kanban') {
      const centre = card.position.x + CARD_SIZE.width / 2 - frame.position.x;
      const lane = laneAt((frame.data.lanes ?? []) as Lane[], centre);
      now = lane ? lane.value : null;
    } else if (frame.data.layout === 'timeline' && frame.data.scale) {
      const undatedY = frame.data.undatedY as number | null | undefined;
      const relY = card.position.y - frame.position.y;
      // Dropped among the cards with no date: it has none either.
      now = undatedY != null && relY >= undatedY ? '' : dayAt(frame.data.scale as TimeScale, card.position.x - frame.position.x);
      if (dayOf(was) === now) now = null;
    }
    if (field && now !== null && now !== was) {
      const { ref, kind, title } = card.data;
      const change = now;
      try {
        await ctx.writeProperty(ref, kind, title, propertyChange(field, change, was, kind), was, change);
        ctx.onMoved?.({
          title, field, was, now: change,
          undo: async () => {
            await ctx.writeProperty!(ref, kind, title, propertyChange(field, was, change, kind), change, was);
            const f = board()?.nodes.find((n) => n.id === frame.id);
            if (f && (await refreshFrame(f))) ctx.scheduleSave();
            ctx.refresh();
          },
        });
      } catch (err) {
        logger.error('Could not change the item from the board', err as string);
        (ctx.onMoveFailed ?? ctx.onError)(errorText(err));
      }
    }
    const changed = await refreshFrame(frame);
    ctx.refresh();
    if (changed) ctx.scheduleSave();
    return changed;
  }

  /** Every live frame on the open board, one at a time. */
  async function refreshAll() {
    if (running) return;
    running = true;
    try {
      let changed = false;
      for (const frame of (board()?.nodes ?? []).filter((n) => n.type === 'frame' && n.data?.query)) {
        changed = (await refreshFrame(frame)) || changed;
      }
      if (changed) {
        ctx.refresh();
        ctx.scheduleSave();
      }
    } finally {
      running = false;
    }
  }

  /** A new live frame at `at`, asked at once, laid out as `layout` says. */
  async function addLiveFrame(query: string, title: string, at: { x: number; y: number }, layout: LiveLayout = 'grid', field?: string) {
    const b = board();
    if (!b) return;
    ctx.store.pushUndoState();
    const frame: WBNode = {
      id: ctx.store.generateId('frame'),
      type: 'frame',
      position: { x: Math.round(at.x), y: Math.round(at.y) },
      data: {
        label: title || query,
        query,
        width: CARD_SIZE.width * 2 + GAP + PAD * 2,
        height: 200,
        ...(layout !== 'grid' ? { layout } : {}),
        ...(layout === 'kanban' && field ? { groupBy: field } : {}),
        ...(layout === 'timeline' && field ? { dateField: field } : {}),
      },
    };
    stampElement(frame);
    b.nodes.push(frame);
    await refreshFrame(frame);
    ctx.refresh();
    ctx.scheduleSave();
  }

  /** Show a live frame another way, and ask it again. */
  async function setLayout(frame: WBNode, layout: LiveLayout) {
    ctx.store.pushUndoState();
    frame.data = { ...frame.data, layout: layout === 'grid' ? undefined : layout };
    stampElement(frame);
    await refreshFrame(frame);
    ctx.refresh();
    ctx.scheduleSave();
  }

  return { refreshAll, refreshFrame, addLiveFrame, moveCard, setLayout };
}
