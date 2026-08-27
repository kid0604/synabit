import { describe, it, expect } from 'vitest';
import { parseAnnualDate, nextOccurrence, daysUntilAnnual } from '../composables/anniversaries';

describe('parseAnnualDate', () => {
  it('accepts a date with or without its year', () => {
    // Both shapes are in the vault: the contact form writes the first, and
    // somebody who knows the day but not the year writes the second. The
    // notification engine accepts both, so these screens have to as well.
    expect(parseAnnualDate('1994-03-02')).toEqual({ month: 3, day: 2 });
    expect(parseAnnualDate('03-02')).toEqual({ month: 3, day: 2 });
    expect(parseAnnualDate(' 1994-3-2 ')).toEqual({ month: 3, day: 2 });
  });

  it('accepts the 29th of February', () => {
    expect(parseAnnualDate('2028-02-29')).toEqual({ month: 2, day: 29 });
  });

  it('refuses anything that is not a date', () => {
    for (const bad of ['', 'March', '1994', '1994-13-02', '1994-02-30', '1994-03-02-01', 'x-y']) {
      expect(parseAnnualDate(bad), bad).toBeNull();
    }
  });
});

describe('nextOccurrence', () => {
  it('stays in this year when the day is still ahead', () => {
    const got = nextOccurrence({ month: 12, day: 25 }, new Date(2026, 7, 25));
    expect(got).toEqual(new Date(2026, 11, 25));
  });

  it('counts today as this year, not next', () => {
    const got = nextOccurrence({ month: 8, day: 25 }, new Date(2026, 7, 25, 18, 30));
    expect(got).toEqual(new Date(2026, 7, 25));
  });

  it('rolls into next year once the day has gone', () => {
    const got = nextOccurrence({ month: 3, day: 2 }, new Date(2026, 7, 25));
    expect(got).toEqual(new Date(2027, 2, 2));
  });

  it('keeps a 29 February birthday on the 28th in a common year', () => {
    // The same rule the reminder engine and the recurring-event engine apply.
    // If these disagreed, the countdown here would point at one day and the
    // phone would ring on another.
    expect(nextOccurrence({ month: 2, day: 29 }, new Date(2027, 0, 1)))
      .toEqual(new Date(2027, 1, 28));
    expect(nextOccurrence({ month: 2, day: 29 }, new Date(2028, 0, 1)))
      .toEqual(new Date(2028, 1, 29));
  });
});

describe('daysUntilAnnual', () => {
  it('returns 0 on the day itself', () => {
    expect(daysUntilAnnual('1994-08-25', new Date(2026, 7, 25, 23, 59))).toBe(0);
  });

  it('counts whole days regardless of the time of day', () => {
    expect(daysUntilAnnual('1994-08-26', new Date(2026, 7, 25, 0, 1))).toBe(1);
    expect(daysUntilAnnual('1994-08-26', new Date(2026, 7, 25, 23, 59))).toBe(1);
  });

  it('crosses a daylight-saving boundary without losing or gaining a day', () => {
    // Northern-hemisphere clocks move on the last Sunday of October; a naive
    // millisecond division lands on 30.958… days and floors to 30.
    expect(daysUntilAnnual('1994-11-15', new Date(2026, 9, 15, 12))).toBe(31);
  });

  it('tells "not a date" apart from "today"', () => {
    expect(daysUntilAnnual('', new Date(2026, 7, 25))).toBeNull();
    expect(daysUntilAnnual('sometime in March', new Date(2026, 7, 25))).toBeNull();
  });
});
