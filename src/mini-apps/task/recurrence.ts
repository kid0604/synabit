/**
 * Repeating tasks.
 *
 * A repeating *event* is one file that the calendar draws on many days. A task
 * cannot work that way: each occurrence has its own status, its own completion
 * date, possibly its own notes. Spawning a file per occurrence would answer
 * that, at the cost of a vault that grows without bound and a sync that
 * carries it — a daily task is 365 files a year.
 *
 * So a repeating task is one file that moves forward. Completing it does not
 * mark it done; it advances its dates to the next occurrence and leaves it to
 * do again. That is the model Todoist and Things use, and the one users
 * already have in their heads.
 *
 * The cost, stated plainly because it is a real one: the vault does not record
 * that you paid rent in July, only that the next rent is due in September.
 * Keeping that history needs a per-occurrence record, which is the other model.
 */

import type { TaskMetadata } from './types';

export const RECURRENCE_OPTIONS = ['none', 'daily', 'weekly', 'monthly', 'yearly'] as const;
export type Recurrence = (typeof RECURRENCE_OPTIONS)[number];

export const isRecurrence = (value: string): value is Recurrence =>
  (RECURRENCE_OPTIONS as readonly string[]).includes(value);

export const repeats = (task: Pick<TaskMetadata, 'recurrence'>): boolean =>
  !!task.recurrence && task.recurrence !== 'none' && isRecurrence(task.recurrence);

const pad = (n: number) => String(n).padStart(2, '0');
const toStr = (y: number, m: number, d: number) => `${y}-${pad(m)}-${pad(d)}`;

/** Days in a given month, 1-indexed. */
const daysInMonth = (year: number, month: number): number => new Date(year, month, 0).getDate();

const parse = (dateStr: string): [number, number, number] | null => {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec((dateStr || '').trim());
  if (!match) return null;
  const [y, m, d] = [Number(match[1]), Number(match[2]), Number(match[3])];
  if (m < 1 || m > 12 || d < 1 || d > daysInMonth(y, m)) return null;
  return [y, m, d];
};

const addDays = (y: number, m: number, d: number, days: number): string => {
  const date = new Date(y, m - 1, d);
  date.setDate(date.getDate() + days);
  return toStr(date.getFullYear(), date.getMonth() + 1, date.getDate());
};

/**
 * Months on, with the day clamped to one that exists.
 *
 * The 31st plus a month is not the 31st of a 30-day month. `setMonth` answers
 * that by rolling into the next month — the 31st of January becomes the 3rd of
 * March — which turns "the last day of the month" into a date drifting a few
 * days later every time it repeats. Clamping keeps it on the 28th, 29th or
 * 30th, which is what the user meant, and what the 29th of February needs in
 * every year that does not have one.
 */
const addMonths = (y: number, m: number, d: number, months: number): string => {
  const totalMonths = (y * 12 + (m - 1)) + months;
  const year = Math.floor(totalMonths / 12);
  const month = (totalMonths % 12) + 1;
  return toStr(year, month, Math.min(d, daysInMonth(year, month)));
};

/** The next date in the series after `dateStr`, or `null` if it does not repeat. */
export function nextOccurrence(dateStr: string, recurrence: string): string | null {
  if (!isRecurrence(recurrence) || recurrence === 'none') return null;
  const parsed = parse(dateStr);
  if (!parsed) return null;
  const [y, m, d] = parsed;
  switch (recurrence) {
    case 'daily': return addDays(y, m, d, 1);
    case 'weekly': return addDays(y, m, d, 7);
    case 'monthly': return addMonths(y, m, d, 1);
    case 'yearly': return addMonths(y, m, d, 12);
  }
}

/**
 * The first occurrence strictly after `after`.
 *
 * Completing a daily task that has been sitting overdue for a week should not
 * leave it overdue by six days. It steps until it is genuinely in the future,
 * so ticking it off means "done, next one is tomorrow" however long it was
 * neglected.
 *
 * The step count is bounded: a corrupt date or a recurrence this does not
 * understand must not spin forever inside a click handler. Ten thousand steps
 * is twenty-seven years of a daily task, and any real series reaches its end
 * long before that.
 */
export function nextOccurrenceAfter(
  dateStr: string,
  recurrence: string,
  after: string,
): string | null {
  let current = nextOccurrence(dateStr, recurrence);
  for (let steps = 0; current && current <= after && steps < 10_000; steps += 1) {
    const following = nextOccurrence(current, recurrence);
    if (!following || following === current) return current;
    current = following;
  }
  return current;
}

/** What completing a repeating task should do. */
export type RecurrenceAdvance =
  | { kind: 'complete' }
  | { kind: 'advance'; start_date: string; due_date: string };

/**
 * Where a repeating task goes when it is ticked off.
 *
 * `complete` when the series has run out — either there is no date to count
 * from, or the next occurrence is past `recurrence_end_at`. A series that has
 * ended is a task like any other, and finishing it should finish it.
 *
 * `start_date` moves by the same number of days the due date did, so a task
 * that opens three days before it is due keeps that window. It is not
 * recomputed from the recurrence: a start date on a different weekday from the
 * due date would otherwise drift apart from it.
 */
export function advanceRecurrence(
  task: Pick<TaskMetadata, 'recurrence' | 'recurrence_end_at' | 'start_date' | 'due_date'>,
  today: string,
): RecurrenceAdvance {
  if (!repeats(task)) return { kind: 'complete' };

  // Anchored on the due date, falling back to the start date: a repeating task
  // with only a start date is still a repeating task.
  const anchor = task.due_date || task.start_date;
  if (!parse(anchor)) return { kind: 'complete' };

  const next = nextOccurrenceAfter(anchor, task.recurrence, today);
  if (!next) return { kind: 'complete' };
  if (task.recurrence_end_at && next > task.recurrence_end_at) return { kind: 'complete' };

  let start = task.start_date;
  if (task.start_date && task.due_date) {
    const shift = daysBetween(task.due_date, next);
    start = shift === null ? task.start_date : addDaysToStr(task.start_date, shift);
  } else if (task.start_date && !task.due_date) {
    start = next;
  }

  return { kind: 'advance', start_date: start, due_date: task.due_date ? next : '' };
}

const daysBetween = (from: string, to: string): number | null => {
  const a = parse(from);
  const b = parse(to);
  if (!a || !b) return null;
  const ms = new Date(b[0], b[1] - 1, b[2]).getTime() - new Date(a[0], a[1] - 1, a[2]).getTime();
  return Math.round(ms / 86_400_000);
};

const addDaysToStr = (dateStr: string, days: number): string => {
  const parsed = parse(dateStr);
  if (!parsed) return dateStr;
  return addDays(parsed[0], parsed[1], parsed[2], days);
};
