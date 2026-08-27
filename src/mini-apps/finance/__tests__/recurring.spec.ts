import { describe, it, expect } from 'vitest';
import {
  dueTransactions,
  pendingByMonth,
  nextOccurrenceAfter,
  occurrenceId,
  occurrencesUpTo,
  todayStr,
  type RecurringRule,
} from '../recurring';

const rule = (over: Partial<RecurringRule> = {}): RecurringRule => ({
  id: 'rule-1',
  recurrence: 'monthly',
  startDate: '2026-01-15',
  template: {
    type: 'expense',
    amount: 150_000,
    category: 'Housing',
    accountId: 'cash',
    note: 'Rent',
  },
  ...over,
});

describe('occurrencesUpTo', () => {
  it('starts on the day the series starts', () => {
    expect(occurrencesUpTo(rule(), '2026-01-15')).toEqual(['2026-01-15']);
  });

  it('produces nothing before the series starts', () => {
    expect(occurrencesUpTo(rule(), '2026-01-14')).toEqual([]);
  });

  it('steps a day, a week, a month and a year', () => {
    expect(occurrencesUpTo(rule({ recurrence: 'daily' }), '2026-01-18')).toEqual([
      '2026-01-15', '2026-01-16', '2026-01-17', '2026-01-18',
    ]);
    expect(occurrencesUpTo(rule({ recurrence: 'weekly' }), '2026-02-05')).toEqual([
      '2026-01-15', '2026-01-22', '2026-01-29', '2026-02-05',
    ]);
    expect(occurrencesUpTo(rule({ recurrence: 'monthly' }), '2026-03-15')).toEqual([
      '2026-01-15', '2026-02-15', '2026-03-15',
    ]);
    expect(occurrencesUpTo(rule({ recurrence: 'yearly' }), '2028-01-15')).toEqual([
      '2026-01-15', '2027-01-15', '2028-01-15',
    ]);
  });

  /**
   * The reason this does not reuse the task module's stepping. Rent due on the
   * 31st has to come back to the 31st: clamping to February and then counting
   * a month from *there* moves rent to the 28th for the rest of the year.
   */
  it('comes back to the 31st after a month that has no 31st', () => {
    const rent = rule({ startDate: '2026-01-31' });
    expect(occurrencesUpTo(rent, '2026-05-31')).toEqual([
      '2026-01-31', '2026-02-28', '2026-03-31', '2026-04-30', '2026-05-31',
    ]);
  });

  it('lands on the 29th in a leap February', () => {
    const rent = rule({ startDate: '2028-01-31' });
    expect(occurrencesUpTo(rent, '2028-02-29')).toContain('2028-02-29');
  });

  it('handles a yearly rule anchored on a leap day', () => {
    const yearly = rule({ recurrence: 'yearly', startDate: '2028-02-29' });
    expect(occurrencesUpTo(yearly, '2029-12-31')).toEqual(['2028-02-29', '2029-02-28']);
  });

  it('stops at the end of the series', () => {
    const ending = rule({ endDate: '2026-02-15' });
    expect(occurrencesUpTo(ending, '2026-06-15')).toEqual(['2026-01-15', '2026-02-15']);
  });

  it('produces nothing while paused', () => {
    expect(occurrencesUpTo(rule({ paused: true }), '2026-06-15')).toEqual([]);
  });

  it('refuses a start date it cannot read', () => {
    expect(occurrencesUpTo(rule({ startDate: 'sometime' }), '2026-06-15')).toEqual([]);
    expect(occurrencesUpTo(rule({ startDate: '2026-02-30' }), '2026-06-15')).toEqual([]);
  });
});

describe('nextOccurrenceAfter', () => {
  it('gives the next one due', () => {
    expect(nextOccurrenceAfter(rule(), '2026-01-15')).toBe('2026-02-15');
    expect(nextOccurrenceAfter(rule(), '2026-01-20')).toBe('2026-02-15');
  });

  it('gives the first one for a series that has not started', () => {
    expect(nextOccurrenceAfter(rule(), '2025-12-01')).toBe('2026-01-15');
  });

  it('gives nothing once the series has ended', () => {
    expect(nextOccurrenceAfter(rule({ endDate: '2026-02-15' }), '2026-02-15')).toBeNull();
  });

  it('gives nothing while paused', () => {
    expect(nextOccurrenceAfter(rule({ paused: true }), '2026-01-15')).toBeNull();
  });
});

describe('dueTransactions', () => {
  /**
   * The property that makes generating safe to repeat. Opening the app twice,
   * or on two devices, writes the same rows rather than two sets of them.
   */
  it('gives every occurrence the same id every time it is asked', () => {
    const once = dueTransactions(rule(), '2026-03-15').map((tx) => tx.id);
    const twice = dueTransactions(rule(), '2026-03-15').map((tx) => tx.id);

    expect(once).toEqual(twice);
    expect(once).toEqual([
      occurrenceId('rule-1', '2026-01-15'),
      occurrenceId('rule-1', '2026-02-15'),
      occurrenceId('rule-1', '2026-03-15'),
    ]);
  });

  it('carries the whole template onto each occurrence', () => {
    const [first] = dueTransactions(rule(), '2026-01-15');
    expect(first).toMatchObject({
      type: 'expense',
      amount: 150_000,
      category: 'Housing',
      accountId: 'cash',
      note: 'Rent',
      recurringRuleId: 'rule-1',
    });
  });

  /**
   * A vault left alone for three months comes back with three months of rent
   * in it, rather than one payment and two that never happened.
   */
  it('catches up on everything that was missed', () => {
    expect(dueTransactions(rule(), '2026-04-15')).toHaveLength(4);
  });

  /**
   * Midnight is the previous day in a time zone to the west, which would move
   * a rent payment onto the 30th for anyone who travels.
   */
  it('files each occurrence at midday, not midnight', () => {
    const [first] = dueTransactions(rule(), '2026-01-15');
    expect(first.date).toBe('2026-01-15T12:00:00.000Z');
  });

  it('gives nothing for a rule that is not due yet', () => {
    expect(dueTransactions(rule(), '2025-12-31')).toEqual([]);
  });
});

describe('todayStr', () => {
  it('reads a date the way the series counts days', () => {
    expect(todayStr(new Date(2026, 7, 5))).toBe('2026-08-05');
  });
});

describe('pendingByMonth', () => {
  /**
   * Writing is per file. A rule running since January owes a transaction in
   * each month since, and they go down one file at a time rather than one
   * transaction at a time.
   */
  it('gathers months of rent into the months they belong in', () => {
    const months = pendingByMonth([rule()], '2026-03-15');

    expect(months.map((m) => m.relPath)).toEqual([
      'Finance/2026-01.json',
      'Finance/2026-02.json',
      'Finance/2026-03.json',
    ]);
    expect(months[0].title).toBe('Month 01/2026');
    expect(months[0].transactions).toHaveLength(1);
  });

  it('puts two rules that fall in one month into one write', () => {
    const rent = rule({ id: 'rent', startDate: '2026-01-01' });
    const salary = rule({ id: 'salary', startDate: '2026-01-25' });

    const months = pendingByMonth([rent, salary], '2026-01-31');
    expect(months).toHaveLength(1);
    expect(months[0].transactions.map((tx) => tx.recurringRuleId).sort()).toEqual([
      'rent',
      'salary',
    ]);
  });

  it('has nothing to do when nothing is due', () => {
    expect(pendingByMonth([rule()], '2025-12-01')).toEqual([]);
    expect(pendingByMonth([], '2026-06-01')).toEqual([]);
  });
});
