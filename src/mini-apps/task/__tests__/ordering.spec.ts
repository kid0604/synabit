import { describe, it, expect } from 'vitest';
import { keyBetween, keysForSequence, isOrderKey } from '../ordering';

describe('keyBetween', () => {
  it('gives a first key for an empty column', () => {
    expect(keyBetween(null, null)).toBeTruthy();
  });

  it('puts a key after another', () => {
    const a = keyBetween(null, null);
    expect(keyBetween(a, null) > a).toBe(true);
  });

  it('puts a key before another', () => {
    const a = keyBetween(null, null);
    expect(keyBetween(null, a) < a).toBe(true);
  });

  it('puts a key between two others', () => {
    const a = keyBetween(null, null);
    const c = keyBetween(a, null);
    const b = keyBetween(a, c);
    expect(a < b && b < c).toBe(true);
  });

  it('refuses a pair given the wrong way round', () => {
    expect(() => keyBetween('b', 'a')).toThrow();
    expect(() => keyBetween('a', 'a')).toThrow();
  });

  /**
   * The whole reason for the change. A float halves its gap on every insert
   * and runs out of mantissa after about fifty; this has to keep going.
   */
  it('subdivides the same gap five hundred times', () => {
    let low = keyBetween(null, null);
    const high = keyBetween(low, null);
    const seen = new Set<string>([low, high]);
    for (let i = 0; i < 500; i += 1) {
      const mid = keyBetween(low, high);
      expect(low < mid, `step ${i}: ${low} < ${mid}`).toBe(true);
      expect(mid < high, `step ${i}: ${mid} < ${high}`).toBe(true);
      expect(seen.has(mid), `step ${i}: duplicate ${mid}`).toBe(false);
      seen.add(mid);
      low = mid;
    }
  });

  it('subdivides downwards just as far', () => {
    const low = keyBetween(null, null);
    let high = keyBetween(low, null);
    for (let i = 0; i < 500; i += 1) {
      const mid = keyBetween(low, high);
      expect(low < mid && mid < high, `step ${i}`).toBe(true);
      high = mid;
    }
  });

  /**
   * Appending climbs the alphabet from the middle before it has to lengthen,
   * so the realistic gesture — dragging a card to the bottom a few dozen times
   * — costs no extra characters at all. Keys are only ever minted by dragging;
   * quick add writes none.
   */
  it('does not lengthen at all over thirty appends', () => {
    let key = keyBetween(null, null);
    const started = key.length;
    for (let i = 0; i < 30; i += 1) key = keyBetween(key, null);
    expect(key.length).toBe(started);
  });

  /**
   * Past that it grows, and the rate is the price of the simple form — about a
   * character per thirty appends. Pinned so that a change making it worse is
   * noticed rather than discovered in somebody's frontmatter.
   */
  it('grows about a character per thirty appends, and no faster', () => {
    let key = keyBetween(null, null);
    for (let i = 0; i < 1000; i += 1) key = keyBetween(key, null);
    expect(key.length).toBeLessThanOrEqual(35);
  });

  it('grows no faster going the other way', () => {
    let key = keyBetween(null, null);
    for (let i = 0; i < 100; i += 1) key = keyBetween(null, key);
    expect(key.length).toBeLessThanOrEqual(25);
  });
});

/**
 * The property that actually matters, exercised the way a board is: cards
 * dropped at the start, at the end, and between two others, in whatever order
 * somebody happens to drag them.
 */
describe('a column under five thousand random drops', () => {
  it('stays sorted, with every key distinct', () => {
    // A fixed sequence rather than Math.random, so a failure can be re-run.
    let seed = 20260823;
    const next = () => {
      seed = (seed * 1103515245 + 12345) & 0x7fffffff;
      return seed / 0x7fffffff;
    };

    const column: string[] = [keyBetween(null, null)];
    for (let i = 0; i < 5000; i += 1) {
      const at = Math.floor(next() * (column.length + 1));
      const before = at > 0 ? column[at - 1] : null;
      const after = at < column.length ? column[at] : null;
      const key = keyBetween(before, after);
      column.splice(at, 0, key);
    }

    for (let i = 1; i < column.length; i += 1) {
      expect(column[i - 1] < column[i], `position ${i}: ${column[i - 1]} !< ${column[i]}`).toBe(true);
    }
    expect(new Set(column).size).toBe(column.length);
  });

  it('does not let keys grow without bound', () => {
    let low = keyBetween(null, null);
    const high = keyBetween(low, null);
    for (let i = 0; i < 2000; i += 1) low = keyBetween(low, high);
    // Each character buys log2(62) ≈ 6 bits of room, so a few hundred
    // subdivisions of one gap should cost tens of characters, not thousands.
    expect(low.length).toBeLessThan(80);
  });
});

describe('keysForSequence', () => {
  it('gives one key per position, in order', () => {
    const keys = keysForSequence(20);
    expect(keys).toHaveLength(20);
    for (let i = 1; i < keys.length; i += 1) {
      expect(keys[i - 1] < keys[i]).toBe(true);
    }
  });

  it('gives nothing for an empty column', () => {
    expect(keysForSequence(0)).toEqual([]);
  });

  it('leaves room to insert between any two of them', () => {
    const keys = keysForSequence(10);
    for (let i = 1; i < keys.length; i += 1) {
      const mid = keyBetween(keys[i - 1], keys[i]);
      expect(keys[i - 1] < mid && mid < keys[i]).toBe(true);
    }
  });
});

describe('isOrderKey', () => {
  it('recognises a generated key', () => {
    expect(isOrderKey(keyBetween(null, null))).toBe(true);
  });

  /** The legacy float ordering, which has to be told apart to be migrated. */
  it('rejects the numbers the board used to store', () => {
    expect(isOrderKey(1700000000000)).toBe(false);
    expect(isOrderKey('1700000000000.5')).toBe(false);
    expect(isOrderKey(-1.5e12)).toBe(false);
  });

  it('rejects empty, missing and non-alphabet values', () => {
    expect(isOrderKey('')).toBe(false);
    expect(isOrderKey(undefined)).toBe(false);
    expect(isOrderKey(null)).toBe(false);
    expect(isOrderKey('a-b')).toBe(false);
    expect(isOrderKey('a b')).toBe(false);
  });

  /**
   * A plain digit string is a valid key and also parses as a number. It has to
   * count as a key, or a column would be migrated again on every drag.
   */
  it('treats a digits-only key as a key', () => {
    expect(isOrderKey('123')).toBe(true);
  });
});
