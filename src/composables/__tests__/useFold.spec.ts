import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { nextTick } from 'vue';

import { useFold } from '../useFold';

/** A sidebar section folded today is folded when the app opens tomorrow. */

const fakeStorage = () => {
  const map = new Map<string, string>();
  return {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, v),
    removeItem: (k: string) => void map.delete(k),
    clear: () => map.clear(),
    key: () => null,
    length: 0,
  } as unknown as Storage;
};

beforeEach(() => vi.stubGlobal('localStorage', fakeStorage()));
afterEach(() => vi.unstubAllGlobals());

describe('folding a sidebar section', () => {
  it('starts open, unless told otherwise', () => {
    expect(useFold('a').folded.value).toBe(false);
    expect(useFold('b', 'folded').folded.value).toBe(true);
  });

  it('stays the way it was left', async () => {
    const first = useFold('tags');
    first.toggle();
    await nextTick();
    expect(useFold('tags').folded.value).toBe(true);

    useFold('tags').toggle();
    await nextTick();
    expect(useFold('tags').folded.value).toBe(false);
  });

  it('opens when something is asked of it, and stays open if it was', async () => {
    const recent = useFold('recent');
    recent.toggle();
    recent.open();
    expect(recent.folded.value).toBe(false);
    recent.open();
    await nextTick();
    expect(useFold('recent').folded.value).toBe(false);
  });

  it('reads a value it does not understand as open', () => {
    localStorage.setItem('odd', 'half');
    expect(useFold('odd').folded.value).toBe(false);
  });
});
