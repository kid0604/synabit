import { describe, it, expect, afterEach, vi } from 'vitest';
import { getTodayStr, isOverdue, type TaskMetadata } from '../types';

/**
 * `toISOString` reports the UTC date. Three places used it to stamp
 * `completed_at` and to label a date "Today", while the views compared against
 * the local date — so east of Greenwich a task ticked after midnight was
 * stamped with yesterday and vanished from the Today view that had just shown
 * it. These pin the local reading.
 */
describe('getTodayStr', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  /**
   * `en-CA` formats as YYYY-MM-DD and, unlike `toISOString`, does it in the
   * runner's own zone — an oracle that is right in every timezone, so this
   * fails on the UTC reading wherever the offset is not zero.
   */
  const localDate = () => new Date().toLocaleDateString('en-CA');

  it('reports the local date, not the UTC one', () => {
    expect(getTodayStr()).toBe(localDate());
  });

  it('still reports the local date late in the evening', () => {
    vi.useFakeTimers();
    // 23:30 local, whatever local is — the hour where a positive UTC offset
    // has already rolled the UTC date over.
    const late = new Date();
    late.setHours(23, 30, 0, 0);
    vi.setSystemTime(late);
    expect(getTodayStr()).toBe(localDate());
  });

  it('still reports the local date just after midnight', () => {
    vi.useFakeTimers();
    const justAfterMidnight = new Date();
    justAfterMidnight.setHours(0, 30, 0, 0);
    vi.setSystemTime(justAfterMidnight);
    expect(getTodayStr()).toBe(localDate());
  });

  it('is a plain YYYY-MM-DD', () => {
    expect(getTodayStr()).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });
});

describe('isOverdue', () => {
  const task = (over: Partial<TaskMetadata>): TaskMetadata =>
    ({ status: 'todo', due_date: '', ...over }) as TaskMetadata;

  it('counts a past due date', () => {
    expect(isOverdue(task({ due_date: '2000-01-01' }))).toBe(true);
  });

  it('does not count today', () => {
    expect(isOverdue(task({ due_date: getTodayStr() }))).toBe(false);
  });

  it('does not count a finished task, however late', () => {
    expect(isOverdue(task({ due_date: '2000-01-01', status: 'done' }))).toBe(false);
  });

  it('does not count a task with no due date', () => {
    expect(isOverdue(task({}))).toBe(false);
  });
});
