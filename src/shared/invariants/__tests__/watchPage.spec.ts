import { describe, it, expect, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => null) }));
vi.mock('../../../utils/logger', () => ({ logger: { warn: vi.fn(), info: vi.fn(), error: vi.fn() } }));
import { isBlocked, reportLimiter } from '../watchPage';

describe('ui.responsive', () => {
  it('takes a late tick in a window with focus for a block', () => {
    expect(isBlocked(3436, true, true)).toBe(true);
  });

  it('does not take a slowed timer in a window put away for one', () => {
    // A window behind another: visible, no focus, every tick ~900 ms late.
    expect(isBlocked(898, false, true)).toBe(false);
  });

  it('leaves out short delays, sleep and hidden windows', () => {
    expect(isBlocked(300, true, true)).toBe(false);
    expect(isBlocked(120_000, true, true)).toBe(false);
    expect(isBlocked(3000, true, false)).toBe(false);
  });

  it('writes one line per route in a burst, and counts the rest into the next', () => {
    const limit = reportLimiter(10_000);
    expect(limit('/note', 0)).toBe(0);
    expect(limit('/note', 1000)).toBeNull();
    expect(limit('/note', 2000)).toBeNull();
    expect(limit('/feeds', 2000)).toBe(0);
    expect(limit('/note', 10_500)).toBe(2);
    expect(limit('/note', 30_000)).toBe(0);
  });
});
