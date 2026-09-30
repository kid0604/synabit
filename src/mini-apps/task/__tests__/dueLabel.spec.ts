import { describe, it, expect } from 'vitest';
import { formatDueDate } from '../dueLabel';

// Midday, so no timezone the runner sits in moves "now" to another day.
const NOW = new Date(2026, 9, 1, 12, 0, 0); // Thu 1 Oct 2026

describe('formatDueDate', () => {
  it('says today, tomorrow and yesterday in words, capitalised', () => {
    expect(formatDueDate('2026-10-01', 'en', NOW)).toBe('Today');
    expect(formatDueDate('2026-10-02', 'en', NOW)).toBe('Tomorrow');
    expect(formatDueDate('2026-09-30', 'en', NOW)).toBe('Yesterday');
    expect(formatDueDate('2026-10-01', 'vi', NOW)).toBe('Hôm nay');
    expect(formatDueDate('2026-10-02', 'vi', NOW)).toBe('Ngày mai');
  });

  it('gives weekday, day and month otherwise, in the app language', () => {
    const en = formatDueDate('2026-10-06', 'en', NOW);
    expect(en).toContain('Tue');
    expect(en).toContain('6');
    expect(en).toContain('Oct');
    expect(en).not.toContain('2026');
    expect(formatDueDate('2026-10-06', 'vi', NOW)).not.toBe(en);
  });

  it('adds the year only when it is not this one', () => {
    expect(formatDueDate('2027-01-05', 'en', NOW)).toContain('2027');
  });

  it('leaves anything that is not a date as it was', () => {
    expect(formatDueDate('someday', 'en', NOW)).toBe('someday');
  });
});
