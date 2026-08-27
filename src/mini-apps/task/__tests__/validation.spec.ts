import { describe, it, expect } from 'vitest';
import {
  isValidDateString, isValidTimeString, isValidDuration, taskFieldIssues,
  safeStatus, safePriority, safeRecurrence, safeDate, safeTime,
} from '../validation';
import { taskDueAt, isOverdue, type TaskMetadata } from '../types';

/**
 * The strings in these tests are not invented. They are what two devices
 * actually produced when both edited the same frontmatter line before syncing,
 * taken from a run of the real coordinator over the real merge path.
 */
const MERGED_STATUS = 'in_pronegress';   // 'done' against 'in_progress'
const MERGED_DATE = '2026-129-315';      // '2026-09-15' against '2026-12-31'

describe('isValidDateString', () => {
  it('accepts a real date', () => {
    expect(isValidDateString('2026-08-23')).toBe(true);
    expect(isValidDateString('2028-02-29')).toBe(true);
  });

  /** The one that caused the damage. */
  it('rejects the string a merge of two dates produces', () => {
    expect(isValidDateString(MERGED_DATE)).toBe(false);
  });

  it('rejects a day the month does not have', () => {
    expect(isValidDateString('2026-02-30')).toBe(false);
    expect(isValidDateString('2026-04-31')).toBe(false);
    expect(isValidDateString('2026-02-29')).toBe(false); // not a leap year
  });

  it('rejects a month that does not exist', () => {
    expect(isValidDateString('2026-13-01')).toBe(false);
    expect(isValidDateString('2026-00-01')).toBe(false);
  });

  it('rejects anything not shaped like a date at all', () => {
    for (const bad of ['', 'tomorrow', '26-08-23', '2026-8-23', '2026/08/23', null, 42]) {
      expect(isValidDateString(bad as never), String(bad)).toBe(false);
    }
  });
});

describe('isValidTimeString', () => {
  it('accepts a time, with or without seconds', () => {
    expect(isValidTimeString('09:00')).toBe(true);
    expect(isValidTimeString('23:59:59')).toBe(true);
  });

  it('rejects hours and minutes a clock does not have', () => {
    expect(isValidTimeString('24:00')).toBe(false);
    expect(isValidTimeString('12:60')).toBe(false);
    expect(isValidTimeString('1299:0000')).toBe(false);
  });

  it('rejects anything else', () => {
    for (const bad of ['', 'noon', '9', '9-00', null]) {
      expect(isValidTimeString(bad as never), String(bad)).toBe(false);
    }
  });
});

describe('isValidDuration', () => {
  it('accepts what the reminder loop understands', () => {
    for (const good of ['0m', '45m', '2h', '1d']) expect(isValidDuration(good)).toBe(true);
  });

  it('rejects what it would silently read as zero', () => {
    for (const bad of ['', '45', '1w', '2 h', 'soon']) expect(isValidDuration(bad), bad).toBe(false);
  });
});

/**
 * The bug this whole module exists to stop: `'2026-129-315'` split into three
 * truthy numbers, `new Date` rolled them over, and the task acquired a
 * deadline in July 2037 — never overdue, never reminded, gone from every
 * warning for eleven years, with nothing anywhere to say so.
 */
describe('a merged date no longer invents a deadline', () => {
  const task = (over: Partial<TaskMetadata>): TaskMetadata =>
    ({ status: 'todo', due_date: '', due_time: '', ...over }) as TaskMetadata;

  it('has no deadline at all rather than one eleven years out', () => {
    expect(taskDueAt(task({ due_date: MERGED_DATE }))).toBeNull();
  });

  it('does not report a nonsense deadline as comfortably in the future', () => {
    expect(isOverdue(task({ due_date: MERGED_DATE }))).toBe(false);
  });

  it('still reads a real date', () => {
    expect(taskDueAt(task({ due_date: '2026-08-30' }))?.getDate()).toBe(30);
  });

  it('ignores a merged time and falls back to the end of the day', () => {
    const due = taskDueAt(task({ due_date: '2026-08-30', due_time: '1599:0000' }))!;
    expect(due.getHours()).toBe(23);
  });
});

describe('safe values', () => {
  it('treats a merged status as unset rather than acting on it', () => {
    expect(safeStatus(MERGED_STATUS)).toBe('todo');
  });

  it('passes a real status straight through', () => {
    for (const s of ['backlog', 'todo', 'in_progress', 'done']) expect(safeStatus(s)).toBe(s);
  });

  it('clears a priority, recurrence, date or time it cannot read', () => {
    expect(safePriority('PP31')).toBe('');
    expect(safeRecurrence('daweeklyily')).toBe('none');
    expect(safeDate(MERGED_DATE)).toBe('');
    expect(safeTime('99:99')).toBe('');
  });

  it('keeps the good ones', () => {
    expect(safePriority('P2')).toBe('P2');
    expect(safeRecurrence('weekly')).toBe('weekly');
    expect(safeDate('2026-08-23')).toBe('2026-08-23');
    expect(safeTime('15:30')).toBe('15:30');
  });
});

describe('taskFieldIssues', () => {
  it('finds nothing wrong with a healthy task', () => {
    expect(taskFieldIssues({
      status: 'done', priority: 'P1', due_date: '2026-08-23',
      due_time: '15:00', recurrence: 'weekly', reminders: ['1d'],
    })).toEqual([]);
  });

  /** An unset field is not a damaged one. */
  it('says nothing about fields that are simply empty', () => {
    expect(taskFieldIssues({ status: 'todo', priority: '', due_date: '', due_time: '' })).toEqual([]);
  });

  it('names the field and quotes what the file says', () => {
    const issues = taskFieldIssues({ status: MERGED_STATUS });
    expect(issues).toEqual([{ field: 'status', value: MERGED_STATUS }]);
  });

  it('reports every damaged field, not just the first', () => {
    const issues = taskFieldIssues({ status: MERGED_STATUS, due_date: MERGED_DATE });
    expect(issues.map((i) => i.field).sort()).toEqual(['due_date', 'status']);
  });

  it('reports a reminder the loop could not act on', () => {
    const issues = taskFieldIssues({ reminders: ['1d', '1w'] });
    expect(issues).toEqual([{ field: 'reminders', value: '1w' }]);
  });

  /** Older files carry a timestamp here rather than a bare date. */
  it('accepts a completion timestamp as well as a date', () => {
    expect(taskFieldIssues({ completed_at: '2026-08-23 09:31:00' })).toEqual([]);
    expect(taskFieldIssues({ completed_at: '2026-08-23' })).toEqual([]);
  });

  it('copes with no properties at all', () => {
    expect(taskFieldIssues(undefined)).toEqual([]);
    expect(taskFieldIssues(null)).toEqual([]);
  });
});
