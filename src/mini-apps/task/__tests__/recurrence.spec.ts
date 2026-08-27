import { describe, it, expect } from 'vitest';
import {
  nextOccurrence,
  nextOccurrenceAfter,
  advanceRecurrence,
  repeats,
  isRecurrence,
} from '../recurrence';
import type { TaskMetadata } from '../types';

const task = (over: Partial<TaskMetadata>): TaskMetadata =>
  ({ recurrence: 'none', recurrence_end_at: '', start_date: '', due_date: '', ...over }) as TaskMetadata;

describe('nextOccurrence', () => {
  it('steps a day, a week, a month and a year', () => {
    expect(nextOccurrence('2026-08-23', 'daily')).toBe('2026-08-24');
    expect(nextOccurrence('2026-08-23', 'weekly')).toBe('2026-08-30');
    expect(nextOccurrence('2026-08-23', 'monthly')).toBe('2026-09-23');
    expect(nextOccurrence('2026-08-23', 'yearly')).toBe('2027-08-23');
  });

  it('crosses a month and a year boundary', () => {
    expect(nextOccurrence('2026-08-31', 'daily')).toBe('2026-09-01');
    expect(nextOccurrence('2026-12-31', 'daily')).toBe('2027-01-01');
    expect(nextOccurrence('2026-12-15', 'monthly')).toBe('2027-01-15');
  });

  /**
   * The 31st plus a month is not the 3rd of the month after. Rolling over is
   * what `setMonth` does, and it makes a "last day of the month" task drift a
   * few days later every single time it repeats.
   */
  it('clamps a day the next month does not have', () => {
    expect(nextOccurrence('2026-01-31', 'monthly')).toBe('2026-02-28');
    expect(nextOccurrence('2026-03-31', 'monthly')).toBe('2026-04-30');
    expect(nextOccurrence('2026-05-31', 'monthly')).toBe('2026-06-30');
  });

  it('clamps into a leap February', () => {
    expect(nextOccurrence('2028-01-31', 'monthly')).toBe('2028-02-29');
  });

  it('clamps a leap day repeating yearly', () => {
    expect(nextOccurrence('2028-02-29', 'yearly')).toBe('2029-02-28');
  });

  it('has no next date for something that does not repeat', () => {
    expect(nextOccurrence('2026-08-23', 'none')).toBeNull();
    expect(nextOccurrence('2026-08-23', '')).toBeNull();
    expect(nextOccurrence('2026-08-23', 'fortnightly')).toBeNull();
  });

  it('refuses a date it cannot read', () => {
    for (const bad of ['', 'tomorrow', '2026-13-01', '2026-02-30', '26-08-23', '2026-8-23']) {
      expect(nextOccurrence(bad, 'daily'), bad).toBeNull();
    }
  });
});

describe('nextOccurrenceAfter', () => {
  /** Ticking off a task neglected for a week must not leave it still overdue. */
  it('skips past every occurrence already gone by', () => {
    expect(nextOccurrenceAfter('2026-08-01', 'daily', '2026-08-23')).toBe('2026-08-24');
    expect(nextOccurrenceAfter('2026-01-15', 'monthly', '2026-08-23')).toBe('2026-09-15');
  });

  it('takes a single step when the next one is already ahead', () => {
    expect(nextOccurrenceAfter('2026-08-23', 'weekly', '2026-08-23')).toBe('2026-08-30');
  });

  it('keeps the weekday when catching up a weekly task', () => {
    const next = nextOccurrenceAfter('2026-08-03', 'weekly', '2026-08-23')!;
    expect(new Date(next + 'T00:00:00').getDay()).toBe(new Date('2026-08-03T00:00:00').getDay());
  });

  /** A click handler must not be able to spin. */
  it('terminates on a series it cannot advance', () => {
    expect(nextOccurrenceAfter('2026-08-23', 'none', '2030-01-01')).toBeNull();
  });
});

describe('advanceRecurrence', () => {
  const today = '2026-08-23';

  it('finishes a task that does not repeat', () => {
    expect(advanceRecurrence(task({ due_date: today }), today)).toEqual({ kind: 'complete' });
  });

  it('moves a repeating task to its next due date', () => {
    expect(advanceRecurrence(task({ recurrence: 'monthly', due_date: '2026-08-23' }), today))
      .toEqual({ kind: 'advance', start_date: '', due_date: '2026-09-23' });
  });

  /** The window between opening and being due is the user's, not the recurrence's. */
  it('carries the start date forward by the same number of days', () => {
    expect(
      advanceRecurrence(
        task({ recurrence: 'monthly', start_date: '2026-08-20', due_date: '2026-08-23' }),
        today,
      ),
    ).toEqual({ kind: 'advance', start_date: '2026-09-20', due_date: '2026-09-23' });
  });

  it('repeats on the start date when there is no due date', () => {
    expect(advanceRecurrence(task({ recurrence: 'weekly', start_date: '2026-08-23' }), today))
      .toEqual({ kind: 'advance', start_date: '2026-08-30', due_date: '' });
  });

  /** A series that has run out is an ordinary task, and finishing it finishes it. */
  it('finishes the task when the next one is past the end of the series', () => {
    expect(
      advanceRecurrence(
        task({ recurrence: 'monthly', due_date: '2026-08-23', recurrence_end_at: '2026-09-01' }),
        today,
      ),
    ).toEqual({ kind: 'complete' });
  });

  it('keeps going while the end date is still ahead', () => {
    expect(
      advanceRecurrence(
        task({ recurrence: 'monthly', due_date: '2026-08-23', recurrence_end_at: '2026-12-31' }),
        today,
      ).kind,
    ).toBe('advance');
  });

  it('finishes a repeating task that has no date to count from', () => {
    expect(advanceRecurrence(task({ recurrence: 'daily' }), today)).toEqual({ kind: 'complete' });
  });

  it('catches an overdue repeat up to the future', () => {
    const result = advanceRecurrence(task({ recurrence: 'daily', due_date: '2026-08-10' }), today);
    expect(result).toEqual({ kind: 'advance', start_date: '', due_date: '2026-08-24' });
  });
});

describe('repeats / isRecurrence', () => {
  it('recognises the options the picker offers', () => {
    for (const value of ['none', 'daily', 'weekly', 'monthly', 'yearly']) {
      expect(isRecurrence(value), value).toBe(true);
    }
    expect(isRecurrence('hourly')).toBe(false);
  });

  it('does not treat none, empty or nonsense as repeating', () => {
    expect(repeats(task({ recurrence: 'none' }))).toBe(false);
    expect(repeats(task({ recurrence: '' }))).toBe(false);
    expect(repeats(task({ recurrence: 'sometimes' }))).toBe(false);
    expect(repeats(task({ recurrence: 'daily' }))).toBe(true);
  });
});
