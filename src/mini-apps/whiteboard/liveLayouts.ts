import type { WBNode } from './boardFile';
import { CARD_SIZE, today } from './vaultCards';
import { todayIso } from '../../shared/localDay';

/**
 * How a live frame lays out its answers.
 *
 * `grid` — in the order the question returned them. `kanban` — in a column per
 * value of one field (a task's `status`, by default), so dragging a card to
 * another column changes that field in the vault. `timeline` — along a line
 * of days by one date field (`due_date`, by default), so dragging a card along
 * it moves the date. The board becomes a way of looking at the vault, and of
 * changing it, not a copy of it.
 *
 * Pure: positions relative to nothing but the frame, for testing.
 */
export type LiveLayout = 'grid' | 'kanban' | 'timeline';

export const PAD = 24;
export const GAP = 16;
/** Room at the top of a lane or a time line for its heading. */
export const HEAD = 40;
const LANE_PAD = 12;
export const LANE_WIDTH = CARD_SIZE.width + LANE_PAD * 2;

/** A task's statuses, in the order work moves through them (see `TASK_STATUSES`). */
export const TASK_STATUS_ORDER = ['backlog', 'todo', 'in_progress', 'done'];

export interface Lane { value: string; x: number; width: number; count: number }
export interface Placed { positions: Map<string, { x: number; y: number }>; width: number; height: number }

/**
 * Columns, one per value, each card under the heading of its own value.
 *
 * A task board shows every status even when none is in it — there has to be
 * a "done" to drag the first card into. Other values come in the order the
 * question returned them; cards without one sit in a last column of their own.
 */
export function kanbanLayout(
  rows: { id: string; value: string }[],
  field: string,
): Placed & { lanes: Lane[] } {
  const values: string[] = field === 'status' ? [...TASK_STATUS_ORDER] : [];
  for (const r of rows) if (r.value && !values.includes(r.value)) values.push(r.value);
  if (rows.some((r) => !r.value)) values.push('');
  const lanes: Lane[] = values.map((value, i) => ({
    value,
    x: PAD + i * (LANE_WIDTH + GAP),
    width: LANE_WIDTH,
    count: rows.filter((r) => r.value === value).length,
  }));
  const positions = new Map<string, { x: number; y: number }>();
  const filled = new Map<string, number>();
  for (const r of rows) {
    const lane = lanes.find((l) => l.value === r.value)!;
    const k = filled.get(r.value) ?? 0;
    filled.set(r.value, k + 1);
    positions.set(r.id, { x: lane.x + LANE_PAD, y: PAD + HEAD + k * (CARD_SIZE.height + GAP) });
  }
  const deepest = Math.max(1, ...lanes.map((l) => l.count));
  return {
    lanes,
    positions,
    width: PAD * 2 + lanes.length * LANE_WIDTH + (lanes.length - 1) * GAP,
    height: PAD * 2 + HEAD + deepest * CARD_SIZE.height + (deepest - 1) * GAP,
  };
}

/** The column a point across the frame falls in: the one it is over, else the nearest. */
export function laneAt(lanes: Lane[], x: number): Lane | null {
  if (!lanes.length) return null;
  const over = lanes.find((l) => x >= l.x && x <= l.x + l.width);
  if (over) return over;
  return lanes.reduce((best, l) => (Math.abs(l.x + l.width / 2 - x) < Math.abs(best.x + best.width / 2 - x) ? l : best));
}

/** `YYYY-MM-DD` at the start of a value, or null: what a date field holds, whatever follows it. */
export function dayOf(value: string | null | undefined): string | null {
  const m = /^(\d{4}-\d{2}-\d{2})/.exec(String(value ?? '').trim());
  return m && !Number.isNaN(Date.parse(m[1])) ? m[1] : null;
}

const DAY = 864e5;
const toDay = (iso: string) => Math.round(Date.parse(iso) / DAY);
// Day numbers count UTC midnights (`Date.parse` of a bare date is UTC), so
// UTC is the zone that turns one back into the same day.
// eslint-disable-next-line no-restricted-syntax
const fromDay = (n: number) => new Date(n * DAY).toISOString().slice(0, 10);

/** Where a time line's days fall across the frame. */
export interface TimeScale { d0: number; d1: number; x0: number; x1: number }
export interface Tick { day: string; x: number }

/**
 * Cards along a line of days, by one date field. A card stands where its day
 * is; cards whose days are too close to share a row step down to the next.
 * Cards with no date wait in a row of their own under the line.
 */
export function timelineLayout(
  rows: { id: string; value: string }[],
  frameWidth: number,
): Placed & { scale: TimeScale; ticks: Tick[]; undatedY: number | null } {
  const dated = rows.map((r) => ({ id: r.id, day: dayOf(r.value) })).filter((r): r is { id: string; day: string } => !!r.day);
  const undated = rows.filter((r) => !dayOf(r.value));
  const days = dated.map((r) => toDay(r.day));
  let d0 = days.length ? Math.min(...days) : toDay(todayIso());
  let d1 = days.length ? Math.max(...days) : d0 + 7;
  if (d1 - d0 < 7) { d0 -= 1; d1 = d0 + 8; }
  // Wide enough that the cards of a busy stretch are not all stacked.
  const inner = Math.max(frameWidth - PAD * 2, Math.min(dated.length, 8) * (CARD_SIZE.width * 0.75) + CARD_SIZE.width, 640);
  const scale: TimeScale = { d0, d1, x0: PAD, x1: PAD + inner - CARD_SIZE.width };
  const xOf = (d: number) => scale.x0 + ((d - d0) / (d1 - d0)) * (scale.x1 - scale.x0);

  const positions = new Map<string, { x: number; y: number }>();
  const rowEnds: number[] = [];
  for (const r of [...dated].sort((a, b) => a.day.localeCompare(b.day))) {
    const x = xOf(toDay(r.day));
    let row = rowEnds.findIndex((end) => end + GAP <= x);
    if (row < 0) { row = rowEnds.length; rowEnds.push(-Infinity); }
    rowEnds[row] = x + CARD_SIZE.width;
    positions.set(r.id, { x, y: PAD + HEAD + row * (CARD_SIZE.height + GAP) });
  }
  const datedBottom = PAD + HEAD + rowEnds.length * (CARD_SIZE.height + GAP);
  let undatedY: number | null = null;
  if (undated.length) {
    undatedY = datedBottom + GAP;
    const perRow = Math.max(1, Math.floor((inner + GAP) / (CARD_SIZE.width + GAP)));
    undated.forEach((r, i) => positions.set(r.id, {
      x: PAD + (i % perRow) * (CARD_SIZE.width + GAP),
      y: undatedY! + 28 + Math.floor(i / perRow) * (CARD_SIZE.height + GAP),
    }));
  }
  const bottom = Math.max(...[...positions.values()].map((p) => p.y + CARD_SIZE.height), PAD + HEAD + CARD_SIZE.height);

  // A tick at most every so many pixels, on whole days.
  const step = Math.max(1, Math.ceil((d1 - d0) / Math.max(1, Math.floor((scale.x1 - scale.x0) / 110))));
  const ticks: Tick[] = [];
  for (let d = d0; d <= d1; d += step) ticks.push({ day: fromDay(d), x: xOf(d) });
  return { scale, ticks, positions, undatedY, width: PAD * 2 + inner, height: bottom + PAD };
}

/** The day a point across the frame stands for: where a card dropped there would be dated. */
export function dayAt(scale: TimeScale, x: number): string {
  const clamped = Math.min(Math.max(x, scale.x0), scale.x1);
  const span = scale.x1 - scale.x0 || 1;
  return fromDay(Math.round(scale.d0 + ((clamped - scale.x0) / span) * (scale.d1 - scale.d0)));
}

/**
 * What changes in the vault when a card lands in another column or on
 * another day. A task moved into or out of "done" gets the completion date
 * the Tasks app would give it, so its streaks and history agree.
 */
export function propertyChange(field: string, value: string, was: string, kind: string): Record<string, unknown> {
  const patch: Record<string, unknown> = { [field]: value || null };
  if (kind === 'task' && field === 'status') {
    if (value === 'done' && was !== 'done') patch.completed_at = today();
    if (value !== 'done' && was === 'done') patch.completed_at = null;
  }
  return patch;
}

/** The card a frame layout places, by its id, positioned relative to the frame. */
export function inFrame(frame: WBNode, p: { x: number; y: number }) {
  return { x: frame.position.x + p.x, y: frame.position.y + p.y };
}
