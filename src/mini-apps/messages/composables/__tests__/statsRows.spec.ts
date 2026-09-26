import { describe, it, expect } from 'vitest';

import { barWidth, share, ENDINGS, CEILINGS } from '../useSynStats';
import type { RunState } from '../../types';

describe('the numbers screen', () => {
  /**
   * No whole, no share. A vault whose runs all predate the measurement has a
   * denominator of zero, and "0% of runs carried memory" would be a confident
   * claim about runs nobody measured.
   */
  it('has no share of nothing', () => {
    expect(share(0, 0)).toBeNull();
    expect(share(3, 12)).toBe(25);
    expect(share(1, 3)).toBe(33);
  });

  /** The longest bar fills its row, and a count of one is still visible. */
  it('draws bars against the largest in their own table', () => {
    expect(barWidth(10, 10)).toBe(100);
    expect(barWidth(5, 10)).toBe(50);
    expect(barWidth(1, 1000)).toBe(2);
    expect(barWidth(0, 10)).toBe(0);
    expect(barWidth(0, 0)).toBe(0);
  });

  /**
   * Every way a run can end has a row, so a zero reads as a zero. Checked
   * against the union by assignment: a state added to `RunState` and not here
   * is a row the screen silently never draws.
   */
  it('lists every ending once', () => {
    const all: Record<RunState, true> = {
      working: true, done: true, failed: true, cancelled: true, budget_exhausted: true,
      awaiting_consent: true, awaiting_choice: true, interrupted: true,
    };
    expect([...ENDINGS].sort()).toEqual(Object.keys(all).sort());
    expect(new Set(CEILINGS).size).toBe(CEILINGS.length);
  });
});
