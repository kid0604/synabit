/**
 * An answer with days in it, counted over time.
 *
 * The arithmetic behind `EventsOverTime.vue`, kept apart so it can be tested
 * without drawing anything: which day each row is on, how wide a bucket
 * should be for the span the answer covers, how many rows fall in each, and
 * which rows are inside a range somebody selected.
 *
 * The days come from the column `shapeFor.dateColumn` picks, so this and the
 * list beside the chart can never disagree about where a row sits in time.
 */
import { timeDay, timeMonday, timeMonth, timeYear, type CountableTimeInterval } from 'd3';
import { dateColumn } from './shapeFor';
import type { QueryResult, QueryRow } from './types';

/** How wide one bar is. */
export type Grain = 'day' | 'week' | 'month' | 'year';

/** A selected stretch of time, both ends included, as `YYYY-MM-DD`. */
export interface DayRange {
  from: string;
  to: string;
}

export interface Dated {
  row: QueryRow;
  day: Date;
  /** The cell as written, `YYYY-MM-DD`. */
  iso: string;
}

export interface Bucket {
  start: Date;
  /** The first day of the next bucket. */
  end: Date;
  count: number;
}

const A_DAY = /^(\d{4})-(\d{2})-(\d{2})$/;

/**
 * A `YYYY-MM-DD` cell as the reader's own midnight, or null.
 *
 * Not `new Date('2026-04-30')`: that parses as UTC, and in Hà Nội it is the
 * evening of the 29th. A day the vault wrote down is a day on the reader's
 * calendar. And an impossible day — `2026-02-30` — is refused rather than
 * rolled into March, which is what `Date` would do with it.
 */
export function dayOf(cell: string | undefined): Date | null {
  const m = A_DAY.exec((cell ?? '').trim());
  if (!m) return null;
  const [year, month, day] = [Number(m[1]), Number(m[2]), Number(m[3])];
  const date = new Date(year, month - 1, day);
  if (date.getFullYear() !== year || date.getMonth() !== month - 1 || date.getDate() !== day) return null;
  return date;
}

/** A local midnight back as `YYYY-MM-DD`. */
export function isoOf(date: Date): string {
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/** The rows that have a day, and how many do not. */
export function datedRows(result: QueryResult | null): { dated: Dated[]; undated: number } {
  if (!result) return { dated: [], undated: 0 };
  const at = dateColumn(result);
  if (at < 0) return { dated: [], undated: result.rows.length };
  const dated: Dated[] = [];
  let undated = 0;
  for (const row of result.rows) {
    const day = dayOf(row.cells[at]);
    if (day) dated.push({ row, day, iso: isoOf(day) });
    else undated += 1;
  }
  return { dated, undated };
}

/**
 * How wide a bar should be for an answer spanning `from` to `to`.
 *
 * Wide enough that the bars are not a comb of ones, narrow enough that three
 * busy weeks and five quiet months still look like what they were.
 */
export function grainFor(from: Date, to: Date): Grain {
  const days = (to.getTime() - from.getTime()) / 86_400_000;
  if (days <= 62) return 'day';
  if (days <= 400) return 'week';
  if (days <= 366 * 8) return 'month';
  return 'year';
}

/** Weeks start on Monday, the way the calendar in both locales does. */
export function intervalOf(grain: Grain): CountableTimeInterval {
  return { day: timeDay, week: timeMonday, month: timeMonth, year: timeYear }[grain];
}

/**
 * Every bucket from the first row to the last, **including the empty ones**.
 *
 * The empty ones are the point. Three things in one week and then nothing for
 * five months is an answer, and a chart that drew only the busy buckets would
 * close the gap up and hide it.
 */
export function bucketsOf(dated: Dated[], grain: Grain, span?: { from: Date; to: Date }): Bucket[] {
  if (!dated.length && !span) return [];
  const interval = intervalOf(grain);
  // A chosen stretch is drawn whole, quiet ends included: "the last year"
  // with nothing in its first eight months is an answer, and fitting the
  // axis to the data would draw it as a busy four.
  let first = span?.from ?? dated[0].day;
  let last = span?.to ?? dated[0].day;
  if (!span) {
    for (const { day } of dated) {
      if (day < first) first = day;
      if (day > last) last = day;
    }
  }
  const start = interval.floor(first);
  const stop = interval.offset(interval.floor(last), 1);
  const buckets: Bucket[] = interval
    .range(start, stop)
    .map(begin => ({ start: begin, end: interval.offset(begin, 1), count: 0 }));
  const index = new Map(buckets.map((bucket, i) => [bucket.start.getTime(), i]));
  for (const { day } of dated) {
    const i = index.get(interval.floor(day).getTime());
    if (i !== undefined) buckets[i].count += 1;
  }
  return buckets;
}

/** The range a bucket covers, both ends included. */
export function rangeOf(bucket: Bucket): DayRange {
  return { from: isoOf(bucket.start), to: isoOf(timeDay.offset(bucket.end, -1)) };
}

/** Whether a `YYYY-MM-DD` day falls inside a range. String order is day order. */
export function inRange(iso: string, range: DayRange): boolean {
  return iso >= range.from && iso <= range.to;
}

/**
 * The answer with only the rows inside a range, for the list beside the chart.
 *
 * Rows with no day are dropped when a range is set: they are not in *any*
 * stretch of time, so they cannot be in the one that was picked. `total`
 * becomes what is shown — the answer to the narrower question.
 */
export function within(result: QueryResult | null, range: DayRange | null): QueryResult | null {
  if (!result || !range) return result;
  const at = dateColumn(result);
  if (at < 0) return result;
  const rows = result.rows.filter(row => {
    const day = dayOf(row.cells[at]);
    return !!day && inRange(isoOf(day), range);
  });
  return { ...result, rows, total: rows.length };
}

// ── the window: which stretch of time is being looked at ─────────────────

/**
 * A stretch to look at: one counted back from today, everything, or two
 * days somebody picked.
 *
 * Separate from the range picked *on* the chart. The window says what the
 * chart and the list are drawn over; a range picked inside it narrows the
 * list further. Changing the window lets that range go.
 */
export type WindowChoice = '30d' | '90d' | '1y' | '5y' | 'all' | DayRange;

export const PRESETS = ['30d', '90d', '1y', '5y', 'all'] as const;
export type Preset = (typeof PRESETS)[number];

const DAYS_BACK: Record<Exclude<Preset, 'all'>, number> = { '30d': 30, '90d': 90, '1y': 365, '5y': 365 * 5 + 1 };

/** The days a window covers, or null for everything. */
export function windowRange(choice: WindowChoice, today: Date): DayRange | null {
  if (typeof choice === 'object') return choice;
  if (choice === 'all') return null;
  const from = timeDay.offset(timeDay.floor(today), -(DAYS_BACK[choice] - 1));
  return { from: isoOf(from), to: isoOf(today) };
}

/** More than this, and "everything" is too much to read at once. */
export const MANY = 50;

/**
 * What to look at when nobody has said.
 *
 * Few rows: all of them — there is nothing to be lost in the view. Many:
 * the last year, because a vault with a handful of things from 1991 and two
 * hundred from this year drew the whole of this year as one column at the
 * right-hand edge of a thirty-five-year axis. Unless the last year is empty,
 * in which case "recent" would be a blank chart and everything is shown.
 */
export function autoWindow(result: QueryResult | null, today: Date): WindowChoice {
  const { dated } = datedRows(result);
  if (dated.length <= MANY) return 'all';
  const lastYear = windowRange('1y', today)!;
  return dated.some(item => inRange(item.iso, lastYear)) ? '1y' : 'all';
}

/** How many dated rows a window leaves out, so the view can say so. */
export function outside(result: QueryResult | null, range: DayRange | null): number {
  if (!range) return 0;
  return datedRows(result).dated.filter(item => !inRange(item.iso, range)).length;
}
