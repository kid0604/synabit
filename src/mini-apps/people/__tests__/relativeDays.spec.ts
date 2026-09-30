import { describe, it, expect } from 'vitest';
import { relativeDays } from '../relativeDays';

describe('relativeDays', () => {
  it('says today, tomorrow and yesterday in words', () => {
    expect(relativeDays(0, 'en')).toBe('today');
    expect(relativeDays(1, 'en')).toBe('tomorrow');
    expect(relativeDays(-1, 'en')).toBe('yesterday');
  });

  it('counts days, then months, then years', () => {
    expect(relativeDays(3, 'en')).toBe('in 3 days');
    expect(relativeDays(-12, 'en')).toBe('12 days ago');
    expect(relativeDays(-65, 'en')).toBe('2 months ago');
    expect(relativeDays(-800, 'en')).toBe('2 years ago');
  });

  it('speaks Vietnamese, not English, for a Vietnamese reader', () => {
    expect(relativeDays(0, 'vi')).not.toBe('today');
    expect(relativeDays(3, 'vi')).not.toMatch(/days/);
    expect(relativeDays(-65, 'vi')).toMatch(/tháng/);
  });
});
