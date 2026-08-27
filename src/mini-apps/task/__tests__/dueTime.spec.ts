import { describe, it, expect, afterEach, vi } from 'vitest';
import { taskDueAt, isOverdue, isValidReminder, taskProperties, type TaskMetadata } from '../types';

const task = (over: Partial<TaskMetadata>): TaskMetadata =>
  ({ status: 'todo', due_date: '', due_time: '', reminders: [], ...over }) as TaskMetadata;

describe('taskDueAt', () => {
  it('has no deadline without a due date', () => {
    expect(taskDueAt(task({ due_time: '15:00' }))).toBeNull();
  });

  /**
   * A whole-day task is due when the day ends, not when it begins. Reading it
   * as midnight would make every one of them overdue for its entire day.
   */
  it('reads a date with no time as the end of that day', () => {
    const due = taskDueAt(task({ due_date: '2026-08-30' }))!;
    expect(due.getFullYear()).toBe(2026);
    expect(due.getMonth()).toBe(7);
    expect(due.getDate()).toBe(30);
    expect(due.getHours()).toBe(23);
    expect(due.getMinutes()).toBe(59);
  });

  it('reads a date and time as that minute', () => {
    const due = taskDueAt(task({ due_date: '2026-08-30', due_time: '15:30' }))!;
    expect(due.getHours()).toBe(15);
    expect(due.getMinutes()).toBe(30);
  });

  /**
   * `new Date('2026-08-30')` is parsed as UTC midnight, which is a different
   * calendar day either side of Greenwich. Built from parts, it is local
   * everywhere — so the date the user typed is the date they get.
   */
  it('lands on the date the user typed, whatever the timezone', () => {
    const due = taskDueAt(task({ due_date: '2026-08-30', due_time: '00:30' }))!;
    expect(due.getDate()).toBe(30);
    expect(due.toLocaleDateString('en-CA')).toBe('2026-08-30');
  });

  it('accepts a seconds-bearing time from an older file', () => {
    const due = taskDueAt(task({ due_date: '2026-08-30', due_time: '09:00:00' }))!;
    expect(due.getHours()).toBe(9);
    expect(due.getMinutes()).toBe(0);
  });

  it('falls back to the end of day on a time it cannot read', () => {
    const due = taskDueAt(task({ due_date: '2026-08-30', due_time: 'sometime' }))!;
    expect(due.getHours()).toBe(23);
  });
});

describe('isOverdue with a time', () => {
  afterEach(() => vi.useRealTimers());

  const at = (iso: string) => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(iso));
  };

  it('is not overdue before the hour on the due day', () => {
    at('2026-08-30T09:00:00');
    expect(isOverdue(task({ due_date: '2026-08-30', due_time: '15:00' }))).toBe(false);
  });

  it('is overdue after the hour on the due day', () => {
    at('2026-08-30T16:00:00');
    expect(isOverdue(task({ due_date: '2026-08-30', due_time: '15:00' }))).toBe(true);
  });

  /** The whole point of the end-of-day reading. */
  it('is not overdue all day when it has no time', () => {
    at('2026-08-30T16:00:00');
    expect(isOverdue(task({ due_date: '2026-08-30' }))).toBe(false);
  });

  it('is overdue the next morning when it has no time', () => {
    at('2026-08-31T08:00:00');
    expect(isOverdue(task({ due_date: '2026-08-30' }))).toBe(true);
  });

  it('is never overdue once done', () => {
    at('2027-01-01T00:00:00');
    expect(isOverdue(task({ due_date: '2026-08-30', due_time: '15:00', status: 'done' }))).toBe(false);
  });
});

/** The shape `chat_engine.rs::parse_duration` understands; anything else is zero. */
describe('isValidReminder', () => {
  it('accepts a number followed by m, h or d', () => {
    for (const good of ['0m', '5m', '45m', '2h', '1d', '30d']) {
      expect(isValidReminder(good), good).toBe(true);
    }
  });

  it('rejects anything the backend would silently read as zero', () => {
    for (const bad of ['', 'm', '45', '2 h', 'soon', '1w', '-5m', '1.5h']) {
      expect(isValidReminder(bad), bad).toBe(false);
    }
  });

  it('is case- and space-insensitive', () => {
    expect(isValidReminder(' 2H ')).toBe(true);
  });
});

describe('the new fields reach the file', () => {
  it('writes due_time and reminders', () => {
    const props = taskProperties({ due_date: '2026-08-30', due_time: '15:00', reminders: ['1d', '30m'] });
    expect(props.due_time).toBe('15:00');
    expect(props.reminders).toEqual(['1d', '30m']);
  });

  /** A task that never had them must not gain empty ones. */
  it('says nothing about them when the caller has neither', () => {
    const props = taskProperties({ status: 'todo' });
    expect(props).not.toHaveProperty('due_time');
    expect(props).not.toHaveProperty('reminders');
  });
});
