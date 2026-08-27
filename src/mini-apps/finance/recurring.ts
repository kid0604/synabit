/**
 * Transactions that happen again: rent, salary, a subscription.
 *
 * The largest thing Finance was missing. Most of what an ordinary person's
 * ledger contains is the same handful of amounts arriving on the same handful
 * of days, and typing them in every month is the reason people stop keeping a
 * ledger at all.
 *
 * # Why this does not reuse the task module's stepping
 *
 * `task/recurrence.ts` computes each occurrence from the previous one, which
 * is right for a task — completing it moves it forward, and where it moves to
 * depends on when you completed it. Applied to money it goes wrong within two
 * months: rent due on the 31st is clamped to the 28th of February, and the
 * *next* step is then a month after the 28th. Rent has quietly moved to the
 * 28th for the rest of the year.
 *
 * Every occurrence here is measured from the rule's own start date instead, so
 * the 31st comes back as soon as there is a 31st to come back to: 31 January,
 * 28 February, 31 March. The vocabulary — daily, weekly, monthly, yearly — is
 * still the task module's, because there is no reason for the app to have two.
 *
 * # Why the generated ids are not random
 *
 * A rule that has fallen due produces the same transactions however many times
 * it is asked. The id of an occurrence is its rule and its date, so asking
 * twice writes the same row twice rather than two rows — which is what makes
 * opening the app twice, or on two devices, harmless.
 */

import { isRecurrence, type Recurrence } from '../task/recurrence';
import { monthNodePath, monthNodeTitle } from './ledger';
import type { Transaction } from './types';

export type { Recurrence };
export { isRecurrence };

/** The recurrences money is kept in. `none` is not one of them here. */
export const FINANCE_RECURRENCES: Exclude<Recurrence, 'none'>[] = [
  'daily',
  'weekly',
  'monthly',
  'yearly',
];

/**
 * A transaction the vault should keep making.
 *
 * `template` is everything a transaction needs except the two things that
 * differ per occurrence: which one it is, and when.
 */
export interface RecurringRule {
  id: string;
  recurrence: Exclude<Recurrence, 'none'>;
  /** The first occurrence, and the anchor every later one is measured from. */
  startDate: string;
  /** The last date the series may reach, if it ends at all. */
  endDate?: string;
  /** A rule the user has stopped without deleting. */
  paused?: boolean;
  template: Omit<Transaction, 'id' | 'date'>;
}

const pad = (n: number) => String(n).padStart(2, '0');
const daysInMonth = (year: number, month: number) => new Date(year, month, 0).getDate();

const parse = (dateStr: string): [number, number, number] | null => {
  const match = /^(\d{4})-(\d{2})-(\d{2})/.exec((dateStr || '').trim());
  if (!match) return null;
  const [y, m, d] = [Number(match[1]), Number(match[2]), Number(match[3])];
  if (m < 1 || m > 12 || d < 1 || d > daysInMonth(y, m)) return null;
  return [y, m, d];
};

const toStr = (y: number, m: number, d: number) => `${y}-${pad(m)}-${pad(d)}`;

/** The date `days` after the anchor. */
const anchorPlusDays = ([y, m, d]: [number, number, number], days: number): string => {
  const date = new Date(y, m - 1, d);
  date.setDate(date.getDate() + days);
  return toStr(date.getFullYear(), date.getMonth() + 1, date.getDate());
};

/**
 * The date `months` after the anchor, on the anchor's own day of the month.
 *
 * Clamped to a day the month actually has, and clamped *from the anchor* every
 * time — which is the whole difference from stepping. The 31st becomes the
 * 28th only in February; in March it is the 31st again.
 */
const anchorPlusMonths = ([y, m, d]: [number, number, number], months: number): string => {
  const total = y * 12 + (m - 1) + months;
  const year = Math.floor(total / 12);
  const month = (total % 12) + 1;
  return toStr(year, month, Math.min(d, daysInMonth(year, month)));
};

/** How far apart two occurrences of this recurrence are, in its own units. */
const step = (anchor: [number, number, number], recurrence: Recurrence, n: number): string => {
  switch (recurrence) {
    case 'daily':
      return anchorPlusDays(anchor, n);
    case 'weekly':
      return anchorPlusDays(anchor, n * 7);
    case 'monthly':
      return anchorPlusMonths(anchor, n);
    case 'yearly':
      return anchorPlusMonths(anchor, n * 12);
    default:
      return toStr(anchor[0], anchor[1], anchor[2]);
  }
};

/**
 * A series cannot run forever inside a loop somebody is waiting on.
 *
 * Ten thousand is twenty-seven years of a daily rule — longer than any ledger
 * this app will hold, and short enough that a corrupt date cannot hang the
 * screen it is generating into.
 */
const MAX_OCCURRENCES = 10_000;

/**
 * Whether a stored recurrence is one this can actually step.
 *
 * Takes a plain string because that is what comes off disk: the rule's type
 * says the value is one of four words, and a file written by an older build,
 * a hand edit, or a future version is under no obligation to agree.
 */
const usable = (recurrence: string): boolean =>
  isRecurrence(recurrence) && recurrence !== 'none';

/** Every date this rule produces from its start up to and including `until`. */
export function occurrencesUpTo(rule: RecurringRule, until: string): string[] {
  if (rule.paused) return [];
  const anchor = parse(rule.startDate);
  if (!anchor || !usable(rule.recurrence)) return [];

  const last = rule.endDate && rule.endDate < until ? rule.endDate : until;
  const dates: string[] = [];

  for (let n = 0; n < MAX_OCCURRENCES; n += 1) {
    const date = step(anchor, rule.recurrence, n);
    if (date > last) break;
    dates.push(date);
  }
  return dates;
}

/** The next date this rule will produce after `after`, or `null` if it is done. */
export function nextOccurrenceAfter(rule: RecurringRule, after: string): string | null {
  if (rule.paused) return null;
  const anchor = parse(rule.startDate);
  if (!anchor || !usable(rule.recurrence)) return null;

  for (let n = 0; n < MAX_OCCURRENCES; n += 1) {
    const date = step(anchor, rule.recurrence, n);
    if (date > after) {
      return rule.endDate && date > rule.endDate ? null : date;
    }
  }
  return null;
}

/** The id an occurrence gets: its rule, and the day it falls on. */
export function occurrenceId(ruleId: string, date: string): string {
  return `tx-${ruleId}-${date}`;
}

/**
 * The transactions this rule owes as of `today`.
 *
 * Every occurrence from the start of the series, not only the ones since the
 * app was last opened: a vault left alone for three months should come back
 * with three months of rent in it, and asking again after that changes
 * nothing, because each occurrence writes to the same id.
 *
 * The time of day is noon rather than midnight. A transaction stored at
 * midnight in one time zone is the previous day in another, and a rent payment
 * that moves to the 30th when the user travels is a bug report nobody can
 * reproduce.
 */
export function dueTransactions(rule: RecurringRule, today: string): Transaction[] {
  return occurrencesUpTo(rule, today).map((date) => ({
    ...rule.template,
    id: occurrenceId(rule.id, date),
    date: `${date}T12:00:00.000Z`,
    recurringRuleId: rule.id,
  }));
}

/** Today, as the series counts days. */
export function todayStr(now: Date = new Date()): string {
  return toStr(now.getFullYear(), now.getMonth() + 1, now.getDate());
}

/** Where a batch of generated transactions has to be written. */
export interface PendingMonth {
  relPath: string;
  title: string;
  transactions: Transaction[];
}

/**
 * Everything a set of rules owes, gathered by the month file it belongs in.
 *
 * Grouped because writing is per file: a rule that has been running for a year
 * owes twelve transactions across twelve months, and sending them one at a time
 * would be twelve writes rather than one per month.
 */
export function pendingByMonth(rules: RecurringRule[], today: string): PendingMonth[] {
  const months = new Map<string, PendingMonth>();

  for (const rule of rules) {
    for (const tx of dueTransactions(rule, today)) {
      const date = new Date(tx.date);
      const relPath = monthNodePath(date);
      let month = months.get(relPath);
      if (!month) {
        month = { relPath, title: monthNodeTitle(date), transactions: [] };
        months.set(relPath, month);
      }
      month.transactions.push(tx);
    }
  }

  return [...months.values()].sort((a, b) => a.relPath.localeCompare(b.relPath));
}
