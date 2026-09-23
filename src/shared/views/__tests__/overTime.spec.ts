import { describe, it, expect } from 'vitest';
import { autoWindow, bucketsOf, datedRows, dayOf, grainFor, inRange, isoOf, outside, rangeOf, windowRange, within } from '../overTime';
import type { QueryResult } from '../types';

const answer = (days: string[]): QueryResult => ({
  columns: ['when', 'title'],
  rows: days.map((day, i) => ({ id: `e${i}`, node_type: 'note', title: `t${i}`, cells: [day, `t${i}`] })),
  total: days.length,
  query_time_ms: 1,
});

describe('Reading a day', () => {
  /// `new Date('2026-04-30')` is UTC, which in Hà Nội is the evening before.
  it('is the reader’s own midnight, not UTC', () => {
    const day = dayOf('2026-04-30')!;
    expect([day.getFullYear(), day.getMonth(), day.getDate(), day.getHours()]).toEqual([2026, 3, 30, 0]);
    expect(isoOf(day)).toBe('2026-04-30');
  });

  /// `Date` would quietly roll the 30th of February into March.
  it('refuses a day that does not exist', () => {
    expect(dayOf('2026-02-30')).toBeNull();
    expect(dayOf('2026-13-01')).toBeNull();
    expect(dayOf('hôm qua')).toBeNull();
    expect(dayOf(undefined)).toBeNull();
  });

  it('counts the rows it could not place', () => {
    const result = answer(['2026-04-30', '2026-05-01']);
    result.rows.push({ id: 'x', node_type: 'note', title: 'x', cells: ['', 'x'] });
    const { dated, undated } = datedRows(result);
    expect(dated).toHaveLength(2);
    expect(undated).toBe(1);
  });
});

describe('How wide a bar is', () => {
  const d = (iso: string) => dayOf(iso)!;
  it('follows the span the answer covers', () => {
    expect(grainFor(d('2026-04-01'), d('2026-05-15'))).toBe('day');
    expect(grainFor(d('2026-01-01'), d('2026-09-01'))).toBe('week');
    expect(grainFor(d('2020-01-01'), d('2026-09-01'))).toBe('month');
    expect(grainFor(d('2009-01-01'), d('2026-09-01'))).toBe('year');
  });
});

describe('Counting over time', () => {
  /// Three in one month and nothing for five is an answer; closing the gap
  /// up would hide it.
  it('keeps the empty buckets between the busy ones', () => {
    const { dated } = datedRows(answer(['2026-01-05', '2026-01-20', '2026-01-28', '2026-07-02']));
    const buckets = bucketsOf(dated, 'month');
    expect(buckets.map(b => b.count)).toEqual([3, 0, 0, 0, 0, 0, 1]);
  });

  it('puts every row in exactly one bucket', () => {
    const days = ['2026-03-02', '2026-03-08', '2026-03-09', '2026-03-15', '2026-03-16'];
    const { dated } = datedRows(answer(days));
    const buckets = bucketsOf(dated, 'week');
    expect(buckets.reduce((sum, b) => sum + b.count, 0)).toBe(days.length);
    // Monday-start weeks: 2 → 8 is one week, 9 → 15 the next, 16 the third.
    expect(buckets.map(b => b.count)).toEqual([2, 2, 1]);
  });

  it('has nothing to count in an empty answer', () => {
    expect(bucketsOf([], 'day')).toEqual([]);
  });

  it('gives each bucket a range with both ends included', () => {
    const { dated } = datedRows(answer(['2026-02-10']));
    expect(rangeOf(bucketsOf(dated, 'month')[0])).toEqual({ from: '2026-02-01', to: '2026-02-28' });
  });
});

describe('Narrowing to a range', () => {
  const range = { from: '2026-03-01', to: '2026-03-31' };

  it('includes both ends', () => {
    expect(inRange('2026-03-01', range)).toBe(true);
    expect(inRange('2026-03-31', range)).toBe(true);
    expect(inRange('2026-04-01', range)).toBe(false);
  });

  /// The list beside the chart reads this, so its count must be the count of
  /// what it shows — the answer to the narrower question.
  it('keeps only the rows inside, and says how many', () => {
    const narrowed = within(answer(['2026-02-28', '2026-03-01', '2026-03-15', '2026-04-01']), range)!;
    expect(narrowed.rows.map(r => r.cells[0])).toEqual(['2026-03-01', '2026-03-15']);
    expect(narrowed.total).toBe(2);
  });

  /// Four years at university began before the window, and filled it.
  it('keeps what began before the range and ran into it', () => {
    const result = answer(['2009-09-01', '2026-03-10', '2026-02-01']);
    result.rows[0].until = '2013-06-30';
    result.rows[2].until = '2026-03-05';
    expect(within(result, { from: '2011-01-01', to: '2011-12-31' })!.rows.map(r => r.id)).toEqual(['e0']);
    expect(within(result, range)!.rows.map(r => r.id)).toEqual(['e1', 'e2']);
  });

  /// Were the day column the end, `until` says nothing new — and an `until`
  /// before the day must not stretch it backwards.
  it('never reads an end earlier than the start', () => {
    const result = answer(['2026-03-10']);
    result.rows[0].until = '2026-01-01';
    expect(within(result, { from: '2026-01-01', to: '2026-01-31' })!.rows).toHaveLength(0);
  });

  it('leaves the answer alone when nothing is selected', () => {
    const result = answer(['2026-02-28']);
    expect(within(result, null)).toBe(result);
  });
});

describe('Which stretch to look at', () => {
  const today = dayOf('2026-09-22')!;

  it('counts presets back from today, both ends included', () => {
    expect(windowRange('30d', today)).toEqual({ from: '2026-08-24', to: '2026-09-22' });
    expect(windowRange('1y', today)).toEqual({ from: '2025-09-23', to: '2026-09-22' });
    expect(windowRange('all', today)).toBeNull();
    expect(windowRange({ from: '2020-01-01', to: '2020-12-31' }, today)).toEqual({ from: '2020-01-01', to: '2020-12-31' });
  });

  /// Few rows: nothing to lose by showing them all.
  it('shows everything when there is little', () => {
    expect(autoWindow(answer(['1991-05-01', '2026-09-01']), today)).toBe('all');
  });

  /// The vault that drew this year as one column at the edge of 1991–2026.
  it('shows the last year when there is a lot', () => {
    const days = [...Array.from({ length: 60 }, (_, i) => isoOf(new Date(2026, 5, 1 + i))), '1991-05-01', '1992-01-01'];
    expect(autoWindow(answer(days), today)).toBe('1y');
  });

  /// "Recent" with nothing recent would be a blank chart.
  it('falls back to everything when the last year is empty', () => {
    const days = Array.from({ length: 60 }, (_, i) => isoOf(new Date(2019, 0, 1 + i)));
    expect(autoWindow(answer(days), today)).toBe('all');
  });

  it('says how many it leaves out', () => {
    const result = answer(['1991-05-01', '1992-01-01', '2026-09-01']);
    expect(outside(result, windowRange('1y', today))).toBe(2);
    expect(outside(result, null)).toBe(0);
  });

  /// A job still going is in "the last year", however long ago it began.
  it('does not count as left out what is still going', () => {
    const result = answer(['2019-01-01', '1992-01-01']);
    result.rows[0].until = '2026-09-22';
    expect(outside(result, windowRange('1y', today))).toBe(1);
  });
});

describe('Buckets over a chosen stretch', () => {
  /// A window is drawn whole: the quiet months at its start are still there.
  it('covers the stretch, not just the rows in it', () => {
    const { dated } = datedRows(answer(['2026-09-01']));
    const buckets = bucketsOf(dated, 'month', { from: dayOf('2026-01-01')!, to: dayOf('2026-09-30')! });
    expect(buckets).toHaveLength(9);
    expect(buckets.map(b => b.count)).toEqual([0, 0, 0, 0, 0, 0, 0, 0, 1]);
  });
});
